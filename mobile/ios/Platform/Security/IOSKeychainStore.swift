import CryptoKit
import Foundation
import Security
import FilebeamCore

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

    public func deviceIdentity(origin: String) throws -> NativeDeviceIdentity {
        let origin = try canonicalOrigin(origin)
        let name = "device.identity.\(origin)"
        lock.lock(); defer { lock.unlock() }
        if let data = try read(name: name) {
            let record = try JSONDecoder().decode(DeviceIdentityRecord.self, from: data)
            guard record.id.count == 26, record.privateKey.count == 32, record.publicKey.count == 32 else { throw IOSKeychainStoreError.invalidData }
            return NativeDeviceIdentity(id: record.id, privateKey: record.privateKey, publicKey: record.publicKey.base64EncodedString())
        }
        let key = Curve25519.Signing.PrivateKey()
        let record = DeviceIdentityRecord(id: ulid(), privateKey: key.rawRepresentation, publicKey: key.publicKey.rawRepresentation)
        try upsert(try JSONEncoder().encode(record), name: name, accessibility: kSecAttrAccessibleWhenUnlockedThisDeviceOnly)
        return NativeDeviceIdentity(id: record.id, privateKey: record.privateKey, publicKey: record.publicKey.base64EncodedString())
    }
    public func deviceDeployment(origin: String) throws -> String? { try read(name: "device.deployment.\(try canonicalOrigin(origin))").flatMap { String(data: $0, encoding: .utf8) } }
    public func saveDeviceDeployment(origin: String, deploymentID: String?) throws {
        let name = "device.deployment.\(try canonicalOrigin(origin))"
        lock.lock(); defer { lock.unlock() }
        if let deploymentID, !deploymentID.isEmpty { try upsert(Data(deploymentID.utf8), name: name, accessibility: kSecAttrAccessibleWhenUnlockedThisDeviceOnly) }
        else { try delete(name: name) }
    }

    public func signDeviceEnrollment(origin: String, deviceID: String, generation: UInt64, challenge: String) throws -> Data {
        guard let challenge = Data(base64Encoded: challenge), challenge.count == 32 else { throw IOSKeychainStoreError.invalidData }
        let identity = try deviceIdentity(origin: origin)
        return try Curve25519.Signing.PrivateKey(rawRepresentation: identity.privateKey).signature(for: Data("filebeam-device-enrollment-v1\0\(deviceID)\0\(generation)\0".utf8) + challenge)
    }
    public func signDeviceApproval(origin: String, approval: FilebeamCore.DeviceApprovalPayload) throws -> Data {
        guard approval.instanceOrigin == try canonicalOrigin(origin),
               let challenge = Data(base64Encoded: approval.challengeSha256), challenge.count == 32,
              let publicKey = Data(base64Encoded: approval.candidatePublicKey), publicKey.count == 32 else { throw IOSKeychainStoreError.invalidData }
        let identity = try deviceIdentity(origin: origin)
        let fields = ["filebeam-device-approval-v2", approval.instanceOrigin, approval.deploymentId, String(approval.accountId), String(approval.generation), approval.candidateDeviceId, String(approval.candidateKeyVersion), approval.approverDeviceId, String(approval.approverKeyVersion), String(approval.expiresAtUnix)]
        var message = Data(fields.joined(separator: "\0").utf8); message.append(0)
        message.append(challenge); message.append(publicKey)
        return try Curve25519.Signing.PrivateKey(rawRepresentation: identity.privateKey).signature(for: message)
    }
    public func signDevicePeerProof(origin: String, request: DevicePeerProofRequest) throws -> Data {
        guard request.instanceOrigin == try canonicalOrigin(origin), request.signingPayload.starts(with: Data("filebeam:device-proof:v1".utf8)) else { throw IOSKeychainStoreError.invalidData }
        let identity = try deviceIdentity(origin: origin)
        guard request.signerDeviceId == identity.id else { throw IOSKeychainStoreError.invalidData }
        return try Curve25519.Signing.PrivateKey(rawRepresentation: identity.privateKey).signature(for: request.signingPayload)
    }
    public func signDevicePeerReceipt(origin: String, request: DeviceReceiptRequest) throws -> Data {
        guard request.signingPayload.starts(with: Data("filebeam:device-receipt:v1".utf8)) else { throw IOSKeychainStoreError.invalidData }
        let identity = try deviceIdentity(origin: origin)
        guard request.senderDeviceId == identity.id || request.receiverDeviceId == identity.id else { throw IOSKeychainStoreError.invalidData }
        return try Curve25519.Signing.PrivateKey(rawRepresentation: identity.privateKey).signature(for: request.signingPayload)
    }

    public func loadDeviceCredential(origin: String, deviceID: String) throws -> String? {
        guard let data = try read(name: "device.credential.\(try canonicalOrigin(origin)).\(deviceID)") else { return nil }
        return String(data: data, encoding: .utf8)
    }
    public func saveDeviceCredential(origin: String, deviceID: String, credential: String?) throws {
        let name = "device.credential.\(try canonicalOrigin(origin)).\(deviceID)"
        lock.lock(); defer { lock.unlock() }
        if let credential { try upsert(Data(credential.utf8), name: name, accessibility: kSecAttrAccessibleWhenUnlockedThisDeviceOnly) }
        else { try delete(name: name) }
    }
    public func loadPendingDevicePairing(origin: String) throws -> Data? { try read(name: "device.pairing.\(try canonicalOrigin(origin))") }
    public func savePendingDevicePairing(origin: String, data: Data?) throws {
        let name = "device.pairing.\(try canonicalOrigin(origin))"
        lock.lock(); defer { lock.unlock() }
        if let data { try upsert(data, name: name, accessibility: kSecAttrAccessibleWhenUnlockedThisDeviceOnly) } else { try delete(name: name) }
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
    private struct DeviceIdentityRecord: Codable { let id: String; let privateKey: Data; let publicKey: Data }
    private func ulid() -> String {
        let alphabet = Array("0123456789ABCDEFGHJKMNPQRSTVWXYZ")
        var time = UInt64(Date().timeIntervalSince1970 * 1000); var output = Array(repeating: Character("0"), count: 26)
        for index in stride(from: 9, through: 0, by: -1) { output[index] = alphabet[Int(time & 31)]; time >>= 5 }
        var random = Data(count: 16); _ = random.withUnsafeMutableBytes { SecRandomCopyBytes(kSecRandomDefault, 16, $0.baseAddress!) }
        for index in 10..<26 { output[index] = alphabet[Int(random[index - 10]) & 31] }
        return String(output)
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
