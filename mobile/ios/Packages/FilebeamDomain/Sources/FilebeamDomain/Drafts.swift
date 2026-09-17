import Foundation

public enum SourceState: Codable, Hashable, Sendable { case ready, sizePending, importing, importFailed(String) }

/// A selected local source. `location` is a platform-owned path or security-scoped identity, never file bytes.
public struct FileSource: Codable, Hashable, Sendable, Identifiable {
    public let id: UUID
    public var name: String
    public var location: String
    public var sizeBytes: UInt64?
    public var state: SourceState
    public init(id: UUID = UUID(), name: String, location: String, sizeBytes: UInt64? = nil, state: SourceState = .ready) {
        self.id = id; self.name = name; self.location = location; self.sizeBytes = sizeBytes; self.state = state
    }
}

public struct Recipient: Codable, Hashable, Sendable, Identifiable {
    public let userID: UInt64
    public let username: String
    public let keyBundleID: UInt64
    public let publicKey: String
    public let fingerprint: String
    public var id: UInt64 { userID }
    public init(userID: UInt64, username: String, keyBundleID: UInt64, publicKey: String, fingerprint: String) {
        self.userID = userID; self.username = username; self.keyBundleID = keyBundleID; self.publicKey = publicKey; self.fingerprint = fingerprint
    }
}

public struct FileTransferOptions: Codable, Hashable, Sendable {
    public var transport: Transport
    public var turbo: Bool
    public var archive: Bool
    public var passwordEnabled: Bool
    public var retentionHours: UInt64?
    public var recipient: Recipient?
    public var includeKeyInLink: Bool
    public var driver: String?
    public init(transport: Transport = .http, turbo: Bool = false, archive: Bool = false, passwordEnabled: Bool = false, retentionHours: UInt64? = nil, recipient: Recipient? = nil, includeKeyInLink: Bool = true, driver: String? = nil) {
        self.transport = transport; self.turbo = turbo; self.archive = archive; self.passwordEnabled = passwordEnabled; self.retentionHours = retentionHours; self.recipient = recipient; self.includeKeyInLink = includeKeyInLink; self.driver = driver
    }
}

public struct FileDraft: Codable, Hashable, Sendable, Identifiable {
    public let id: UUID
    public var sources: [FileSource]
    public var options: FileTransferOptions
    public init(id: UUID = UUID(), sources: [FileSource] = [], options: FileTransferOptions = .init()) { self.id = id; self.sources = sources; self.options = options }
}

public enum NoteLanguage: String, Codable, CaseIterable, Sendable { case plain, php, dotenv, javascript, typescript, json, markdown, css, html }

public struct NoteOptions: Codable, Hashable, Sendable {
    public var live: Bool
    public var passwordEnabled: Bool
    public var burnOnRead: Bool
    public var retentionHours: UInt64?
    public var includeKeyInLink: Bool
    public var driver: String?
    public init(live: Bool = false, passwordEnabled: Bool = false, burnOnRead: Bool = false, retentionHours: UInt64? = nil, includeKeyInLink: Bool = true, driver: String? = nil) {
        self.live = live; self.passwordEnabled = passwordEnabled; self.burnOnRead = burnOnRead; self.retentionHours = retentionHours; self.includeKeyInLink = includeKeyInLink; self.driver = driver
    }
}

public struct NoteDraft: Codable, Hashable, Sendable, Identifiable {
    public let id: UUID
    public var title: String
    public var text: String
    public var language: NoteLanguage
    public var options: NoteOptions
    public init(id: UUID = UUID(), title: String = "", text: String = "", language: NoteLanguage = .plain, options: NoteOptions = .init()) { self.id = id; self.title = title; self.text = text; self.language = language; self.options = options }
}

public struct ComposerDrafts: Codable, Hashable, Sendable {
    public var files: FileDraft
    public var note: NoteDraft
    public init(files: FileDraft = .init(), note: NoteDraft = .init()) { self.files = files; self.note = note }
}

/// A sheet edits `draft`; dismissing it leaves the original composer options untouched.
public struct FileOptionsTransaction: Codable, Hashable, Sendable {
    public let original: FileTransferOptions
    public var draft: FileTransferOptions
    public init(options: FileTransferOptions) { self.original = options; self.draft = options }
    public func commit() -> FileTransferOptions { draft }
    public func cancel() -> FileTransferOptions { original }
}

/// Notes have an independent option transaction and never share file-only settings such as archive or recipients.
public struct NoteOptionsTransaction: Codable, Hashable, Sendable {
    public let original: NoteOptions
    public var draft: NoteOptions
    public init(options: NoteOptions) { self.original = options; self.draft = options }
    public func commit() -> NoteOptions { draft }
    public func cancel() -> NoteOptions { original }
}
