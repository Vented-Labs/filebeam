import CryptoKit
import Foundation
import Security

public enum IOSKeychainStoreError: Error { case unavailable(OSStatus), invalidData, checkpointKeyMissing }

/// Device-only custody for native state. Catalog and draft keys are available after first
/// unlock for background recovery; cookies require an unlocked device and never synchronize.
public final class IOSKeychainStore: NativeSecretStorageRemoving, @unchecked Sendable {
    private let namespace: String
    private let stateDirectory: URL
    private let lock = NSLock()

    public init(namespace: String, stateDirectory: URL) {
        self.namespace = namespace
        self.stateDirectory = stateDirectory.standardizedFileURL
    }

    public func checkpointKey(scope: String) throws -> Data {
        guard scope == "checkpoint-catalog-v1" else { throw IOSKeychainStoreError.invalidData }
        return try key(named: "checkpoint.\(scope)", accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, requiresExisting: stateHasCheckpoints())
    }
    /// Native checkpoint removal is explicit: callers must first delete the corresponding state.
    public func removeCheckpointKey(scope: String) throws {
        guard scope == "checkpoint-catalog-v1" else { throw IOSKeychainStoreError.invalidData }
        lock.lock(); defer { lock.unlock() }
        try delete(name: "checkpoint.\(scope)")
    }

    public func loadSession(origin: String) throws -> String? {
        guard let data = try read(name: "session.\(try canonicalOrigin(origin))") else { return nil }
        guard let cookie = String(data: data, encoding: .utf8) else { throw IOSKeychainStoreError.invalidData }
        return cookie
    }

    public func saveSession(origin: String, cookie: String?) throws {
        let name = "session.\(try canonicalOrigin(origin))"
        lock.lock(); defer { lock.unlock() }
        if let cookie { try upsert(Data(cookie.utf8), name: name, accessibility: kSecAttrAccessibleWhenUnlockedThisDeviceOnly) }
        else { try delete(name: name) }
    }

    /// Generic local-data custody key, used for encrypted drafts and sensitive local records.
    public func encryptKey(_ context: String, protectedRecordExists: Bool = false) throws -> SymmetricKey {
        SymmetricKey(data: try key(named: "data.\(context)", accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, requiresExisting: protectedRecordExists))
    }

    private func key(named name: String, accessibility: CFString, requiresExisting: Bool) throws -> Data {
        lock.lock(); defer { lock.unlock() }
        if let existing = try read(name: name) { guard existing.count == 32 else { throw IOSKeychainStoreError.invalidData }; return existing }
        if requiresExisting { throw IOSKeychainStoreError.checkpointKeyMissing }
        var bytes = Data(count: 32)
        let status = bytes.withUnsafeMutableBytes { SecRandomCopyBytes(kSecRandomDefault, 32, $0.baseAddress!) }
        guard status == errSecSuccess else { throw IOSKeychainStoreError.unavailable(status) }
        // Another callback/store may have created the key since the read. Never
        // overwrite an established catalog key; adopt the winning key instead.
        var attributes = query(name)
        attributes[kSecValueData] = bytes
        attributes[kSecAttrAccessible] = accessibility
        let addStatus = SecItemAdd(attributes as CFDictionary, nil)
        if addStatus == errSecDuplicateItem {
            guard let winner = try read(name: name), winner.count == 32 else { throw IOSKeychainStoreError.invalidData }
            return winner
        }
        guard addStatus == errSecSuccess else { throw IOSKeychainStoreError.unavailable(addStatus) }
        return bytes
    }

    private func stateHasCheckpoints() -> Bool {
        // Rust creates the job directory and lock before asking for the catalog
        // key. An empty directory is therefore not evidence of a lost key.
        guard let entries = try? FileManager.default.contentsOfDirectory(at: stateDirectory, includingPropertiesForKeys: nil) else { return false }
        return entries.contains { entry in
            FileManager.default.fileExists(atPath: entry.appendingPathComponent("checkpoint.json").path) ||
                FileManager.default.fileExists(atPath: entry.appendingPathComponent("note-management.json").path)
        }
    }

    private func canonicalOrigin(_ value: String) throws -> String {
        guard var c = URLComponents(string: value), c.scheme?.lowercased() == "https", let host = c.host?.lowercased(), !host.isEmpty,
              c.user == nil, c.password == nil, c.query == nil, c.fragment == nil,
              c.path.isEmpty || c.path == "/" else { throw IOSKeychainStoreError.invalidData }
        c.scheme = "https"; c.host = host; c.path = ""; c.query = nil; c.fragment = nil
        guard let origin = c.url?.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/")), !origin.isEmpty else { throw IOSKeychainStoreError.invalidData }
        return origin
    }

    private func query(_ name: String) -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: "io.filebeam.\(namespace)", kSecAttrAccount: name, kSecAttrSynchronizable: kCFBooleanFalse as Any]
    }
    private func read(name: String) throws -> Data? {
        var q = query(name); q[kSecReturnData] = true; q[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?; let status = SecItemCopyMatching(q as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else { throw IOSKeychainStoreError.unavailable(status) }
        return data
    }
    private func upsert(_ data: Data, name: String, accessibility: CFString) throws {
        let q = query(name); let status = SecItemUpdate(q as CFDictionary, [kSecValueData: data] as CFDictionary)
        if status == errSecSuccess { return }; guard status == errSecItemNotFound else { throw IOSKeychainStoreError.unavailable(status) }
        var add = q; add[kSecValueData] = data; add[kSecAttrAccessible] = accessibility
        let addStatus = SecItemAdd(add as CFDictionary, nil)
        guard addStatus == errSecSuccess || addStatus == errSecDuplicateItem else { throw IOSKeychainStoreError.unavailable(addStatus) }
    }
    private func delete(name: String) throws {
        let status = SecItemDelete(query(name) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw IOSKeychainStoreError.unavailable(status) }
    }
}
