import CryptoKit
import Foundation
import Security

struct SharedDraft: Codable, Sendable, Identifiable {
    let id: UUID
    let files: [URL]
    let text: String?
    let links: [String]
    let sourceReferences: [String]
    let failure: String?
}

enum SharedDraftInboxError: Error { case missingKeychainAccessGroup, folderNotSupported, sourceUnavailable(String) }
private final class SharedDraftCoordinationResult { var error: Error? }

/// Encrypted app-group inbox. A sealed record is published only after all staged bytes are valid.
final class SharedDraftInbox {
    private let directory: URL
    private let accessGroup: String
    private let lock = NSLock()

    init(groupURL: URL, accessGroup: String? = nil) throws {
        directory = groupURL.appendingPathComponent("shared-drafts", isDirectory: true)
        guard let group = accessGroup ?? (Bundle.main.object(forInfoDictionaryKey: "KeychainAccessGroup") as? String), !group.isEmpty else { throw SharedDraftInboxError.missingKeychainAccessGroup }
        self.accessGroup = group
    }

    func pending() throws -> [SharedDraft] {
        lock.lock()
        defer { lock.unlock() }
        try prepareDirectory()
        return try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "sealed" }.sorted { $0.lastPathComponent < $1.lastPathComponent }.map(read)
    }

    /// Copies only file representations in bounded buffers while provider grants remain valid.
    /// A failed source becomes an encrypted pending failure with its retry/reselect reference.
    func stage(files: [URL], text: String?, links: [String], failureHint: String? = nil) throws -> UUID {
        lock.lock()
        defer { lock.unlock() }
        try prepareDirectory()
        let id = UUID()
        let staging = directory.appendingPathComponent(".\(id.uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
        do {
            if let failureHint { throw SharedDraftInboxError.sourceUnavailable(failureHint) }
            guard files.count <= 32 else { throw SharedDraftInboxError.sourceUnavailable("Too many shared files") }
            var staged: [URL] = []
            for (index, source) in files.enumerated() {
                let target = staging.appendingPathComponent("\(index + 1)-\(safeName(source.lastPathComponent, fallback: "item-\(index + 1)"))")
                try copyProvider(source, to: target)
                staged.append(target)
            }
            let published = directory.appendingPathComponent(id.uuidString, isDirectory: true)
            let draft = SharedDraft(id: id, files: staged.map { published.appendingPathComponent($0.lastPathComponent) }, text: text, links: links, sourceReferences: files.map(\.absoluteString), failure: nil)
            try publish(draft, staging: staging, publishedDirectory: published)
        } catch {
            try? FileManager.default.removeItem(at: staging) // Never touches the external provider.
            let failed = SharedDraft(id: id, files: [], text: text, links: links, sourceReferences: files.map(\.absoluteString), failure: "Could not stage shared item: \(error.localizedDescription). Re-select it to retry.")
            try publish(failed, staging: nil, publishedDirectory: nil)
        }
        return id
    }

    /// Read without removal. Import these files first, then call `acknowledge(_:)` on success.
    func peek(_ id: UUID) throws -> SharedDraft? {
        lock.lock()
        defer { lock.unlock() }
        let url = recordURL(id)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        return try read(url)
    }
    /// Compatibility alias with non-destructive transactional semantics.
    func consume(_ id: UUID) throws -> SharedDraft? { try peek(id) }
    func acknowledge(_ id: UUID) throws {
        lock.lock()
        defer { lock.unlock() }
        try? FileManager.default.removeItem(at: recordURL(id))
        try? FileManager.default.removeItem(at: directory.appendingPathComponent(id.uuidString, isDirectory: true))
    }
    func discardFiles(for id: UUID) {
        lock.lock()
        defer { lock.unlock() }
        try? FileManager.default.removeItem(at: directory.appendingPathComponent(id.uuidString, isDirectory: true))
    }

    private func copyProvider(_ source: URL, to target: URL) throws {
        let accessing = source.startAccessingSecurityScopedResource()
        defer { if accessing { source.stopAccessingSecurityScopedResource() } }
        var coordinationError: NSError?
        let result = SharedDraftCoordinationResult()
        NSFileCoordinator().coordinate(readingItemAt: source, options: [], error: &coordinationError) { coordinated in
            do { try Self.copyFile(coordinated, to: target) } catch { result.error = error }
        }
        if let coordinationError { throw coordinationError }
        if let error = result.error { throw error }
    }

    private static func copyFile(_ source: URL, to target: URL) throws {
        let values = try source.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey])
        if values.isDirectory == true { throw SharedDraftInboxError.folderNotSupported }
        if values.isSymbolicLink == true { throw SharedDraftInboxError.sourceUnavailable("Symbolic links cannot be shared") }
        guard FileManager.default.createFile(atPath: target.path, contents: nil), let input = InputStream(url: source), let output = OutputStream(url: target, append: false) else { throw SharedDraftInboxError.sourceUnavailable(source.lastPathComponent) }
        input.open(); output.open(); defer { input.close(); output.close() }
        var copied = 0
        var buffer = [UInt8](repeating: 0, count: 64 * 1024)
        while true {
            let count = input.read(&buffer, maxLength: buffer.count)
            guard count >= 0 else { throw input.streamError ?? CocoaError(.fileReadUnknown) }
            if count == 0 { break }
            var offset = 0
            while offset < count {
                let written = buffer.withUnsafeBufferPointer { output.write($0.baseAddress!.advanced(by: offset), maxLength: count - offset) }
                guard written > 0 else { throw output.streamError ?? CocoaError(.fileWriteUnknown) }
                offset += written
            }
            copied += count
        }
        if let expected = values.fileSize, copied != expected { throw SharedDraftInboxError.sourceUnavailable("The provider changed while copying") }
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: target.path)
        var protection = URLResourceValues(); protection.isExcludedFromBackup = true
        var protected = target; try protected.setResourceValues(protection)
    }

    private func publish(_ draft: SharedDraft, staging: URL?, publishedDirectory: URL?) throws {
        let temporary = directory.appendingPathComponent(".\(draft.id.uuidString).tmp")
        do {
            try AES.GCM.seal(JSONEncoder().encode(draft), using: key(), authenticating: associatedData(for: draft.id)).combined!.write(to: temporary, options: .atomic)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: temporary.path)
            var protection = URLResourceValues(); protection.isExcludedFromBackup = true
            var protected = temporary; try protected.setResourceValues(protection)
            if let staging, let publishedDirectory { try FileManager.default.moveItem(at: staging, to: publishedDirectory) }
            try FileManager.default.moveItem(at: temporary, to: recordURL(draft.id))
        } catch { try? FileManager.default.removeItem(at: temporary); if let publishedDirectory { try? FileManager.default.removeItem(at: publishedDirectory) }; throw error }
    }
    private func prepareDirectory() throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: directory.path)
        var values = URLResourceValues(); values.isExcludedFromBackup = true
        var mutable = directory; try mutable.setResourceValues(values)
    }
    private func recordURL(_ id: UUID) -> URL { directory.appendingPathComponent("\(id.uuidString).sealed") }
    private func read(_ url: URL) throws -> SharedDraft {
        let id = UUID(uuidString: url.deletingPathExtension().lastPathComponent)
        guard let id else { throw SharedDraftInboxError.sourceUnavailable("The shared draft identifier is invalid") }
        return try JSONDecoder().decode(SharedDraft.self, from: AES.GCM.open(AES.GCM.SealedBox(combined: Data(contentsOf: url)), using: try key(), authenticating: associatedData(for: id)))
    }
    private func safeName(_ value: String, fallback: String) -> String { let name = value.replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "\\", with: "_"); return name.isEmpty || name == "." || name == ".." ? fallback : name }
    private func key() throws -> SymmetricKey {
        let query: [CFString: Any] = [kSecClass: kSecClassGenericPassword, kSecAttrService: "io.filebeam.shared-drafts", kSecAttrAccount: "v1", kSecAttrAccessGroup: accessGroup, kSecAttrSynchronizable: kCFBooleanFalse as Any]
        var lookup = query; lookup[kSecReturnData] = true
        var result: CFTypeRef?; let status = SecItemCopyMatching(lookup as CFDictionary, &result)
        if status == errSecSuccess, let data = result as? Data, data.count == 32 { return SymmetricKey(data: data) }
        guard status == errSecItemNotFound else { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
        let existing = try FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
        guard !existing.contains(where: { $0.pathExtension == "sealed" }) else {
            throw SharedDraftInboxError.sourceUnavailable("The shared-draft custody key is unavailable")
        }
        var data = Data(count: 32); guard data.withUnsafeMutableBytes({ SecRandomCopyBytes(kSecRandomDefault, 32, $0.baseAddress!) }) == errSecSuccess else { throw CocoaError(.fileWriteUnknown) }
        var add = query; add[kSecValueData] = data; add[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let addStatus = SecItemAdd(add as CFDictionary, nil)
        if addStatus == errSecSuccess { return SymmetricKey(data: data) }
        guard addStatus == errSecDuplicateItem else { throw NSError(domain: NSOSStatusErrorDomain, code: Int(addStatus)) }
        return try key()
    }
    private func associatedData(for id: UUID) -> Data { Data("filebeam.shared-draft.v1\\u{0}\(id.uuidString)".utf8) }
}
