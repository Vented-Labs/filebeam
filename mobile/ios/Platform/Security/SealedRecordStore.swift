import CryptoKit
import Foundation

/// Small authenticated records for durable local state; payload decoding stays with its owner.
final class SealedRecordStore: Sendable {
    private let file: URL
    private let context: String
    private let keychain: IOSKeychainStore
    // Native callbacks can construct separate stores for the same record path.
    private static let coordinationLock = NSLock()

    init(directory: URL, name: String, context: String, keychain: IOSKeychainStore) {
        file = directory.appendingPathComponent(name)
        self.context = context
        self.keychain = keychain
    }

    func load() throws -> Data? {
        Self.coordinationLock.lock()
        defer { Self.coordinationLock.unlock() }
        guard FileManager.default.fileExists(atPath: file.path) else { return nil }
        let sealed = try AES.GCM.SealedBox(combined: Data(contentsOf: file))
        return try AES.GCM.open(sealed, using: keychain.encryptKey(context, protectedRecordExists: true), authenticating: associatedData)
    }

    func save(_ data: Data) throws {
        Self.coordinationLock.lock()
        defer { Self.coordinationLock.unlock() }
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        let sealed = try AES.GCM.seal(data, using: keychain.encryptKey(context, protectedRecordExists: FileManager.default.fileExists(atPath: file.path)), authenticating: associatedData).combined!
        let temporary = file.deletingLastPathComponent().appendingPathComponent(".\(file.lastPathComponent).\(UUID().uuidString).tmp")
        try sealed.write(to: temporary, options: .atomic)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: temporary.path)
        var values = URLResourceValues(); values.isExcludedFromBackup = true
        var protected = temporary; try protected.setResourceValues(values)
        if FileManager.default.fileExists(atPath: file.path) { _ = try FileManager.default.replaceItemAt(file, withItemAt: temporary) }
        else { try FileManager.default.moveItem(at: temporary, to: file) }
    }

    private var associatedData: Data {
        Data("filebeam.sealed-record.v1\\u{0}\(context)\\u{0}\(file.lastPathComponent)".utf8)
    }
}
