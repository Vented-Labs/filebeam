import Foundation

public typealias TransferJobID = UUID
public enum TransferDirection: String, Codable, Sendable { case upload, download, inboxDownload, unknown }
public enum TransferKind: String, Codable, Sendable { case files, note, unknown }
public enum TransferPhase: String, Codable, Sendable { case preparing, encrypting, uploading, receiving, waitingForRecipient, waitingForSender, awaitingInput, pausing, paused, verifying, verifiedAwaitingExport, exporting, complete, failed, expired, revoked, ended, unknown }
public enum JobLifecycle: String, Codable, Sendable { case running, pausing, paused, complete, failed }
public enum ExportStatus: Codable, Hashable, Sendable { case notAvailable, notExported, exporting, exported, failedRetained(String), failedNotRetained(String) }
public enum ServerAvailability: String, Codable, Sendable { case available, waitingForRecipient, unavailable, unknown }
public enum TransferPromptKind: String, Codable, Sendable { case decryptionKey, password, peerConsent, directoryChoice, filesSelection, shareReady }
public struct TransferSelectionItem: Codable, Hashable, Sendable, Identifiable { public let id: String; public let name: String; public let size: UInt64; public init(id: String, name: String, size: UInt64) { self.id = id; self.name = name; self.size = size } }
public struct TransferPrompt: Codable, Hashable, Sendable, Identifiable { public let id: UInt64; public let kind: TransferPromptKind; public let message: String?; public let selectionItems: [TransferSelectionItem]; public init(id: UInt64, kind: TransferPromptKind, message: String? = nil, selectionItems: [TransferSelectionItem] = []) { self.id = id; self.kind = kind; self.message = message; self.selectionItems = selectionItems } }
public struct TransferActivityRecord: Codable, Hashable, Sendable, Identifiable { public let id: String; public let number: UInt64?; public let progress: Double?; public let state: String; public let selectionCount: UInt64?; public let allFiles: Bool?; public init(id: String, number: UInt64?, progress: Double?, state: String, selectionCount: UInt64?, allFiles: Bool?) { self.id = id; self.number = number; self.progress = progress; self.state = state; self.selectionCount = selectionCount; self.allFiles = allFiles } }
public struct TransferActivity: Codable, Hashable, Sendable { public let records: [TransferActivityRecord]; public let unavailable: Bool; public let retry: Bool; public init(records: [TransferActivityRecord], unavailable: Bool, retry: Bool) { self.records = records; self.unavailable = unavailable; self.retry = retry } }
public struct VerifiedNote: Codable, Hashable, Sendable, Identifiable { public let id: TransferID; public let text: String; public let title: String?; public let language: NoteLanguage; public let consumed: Bool; public init(id: TransferID, text: String, title: String?, language: NoteLanguage, consumed: Bool) { self.id = id; self.text = text; self.title = title; self.language = language; self.consumed = consumed } }

public struct TransferActions: Codable, Hashable, Sendable {
    public var pause: Bool; public var resume: Bool; public var retryExport: Bool; public var retryNoteBurn: Bool; public var discardLocal: Bool; public var revokeRemote: Bool; public var endLive: Bool
    public init(pause: Bool = false, resume: Bool = false, retryExport: Bool = false, retryNoteBurn: Bool = false, discardLocal: Bool = false, revokeRemote: Bool = false, endLive: Bool = false) { self.pause = pause; self.resume = resume; self.retryExport = retryExport; self.retryNoteBurn = retryNoteBurn; self.discardLocal = discardLocal; self.revokeRemote = revokeRemote; self.endLive = endLive }
}

public struct TransferSnapshot: Codable, Hashable, Sendable, Identifiable {
    public let id: TransferJobID
    public let transferID: TransferID?
    public let checkpointID: String?
    public let lifecycle: JobLifecycle
    public let phase: TransferPhase
    public let direction: TransferDirection
    public let kind: TransferKind
    public let transport: Transport?
    public let completedBytes: UInt64
    public let totalBytes: UInt64?
    public let prompt: TransferPrompt?
    public let verifiedFilePaths: [String]
    public let verifiedNote: VerifiedNote?
    public let shareURL: String?
    public let serverAvailability: ServerAvailability
    public let exportStatus: ExportStatus
    public let error: String?
    public let actions: TransferActions
    public init(id: TransferJobID, transferID: TransferID?, checkpointID: String?, lifecycle: JobLifecycle, phase: TransferPhase, direction: TransferDirection, kind: TransferKind, transport: Transport?, completedBytes: UInt64, totalBytes: UInt64?, prompt: TransferPrompt? = nil, verifiedFilePaths: [String] = [], verifiedNote: VerifiedNote? = nil, shareURL: String? = nil, serverAvailability: ServerAvailability = .unknown, exportStatus: ExportStatus = .notAvailable, error: String? = nil, actions: TransferActions = .init()) { self.id = id; self.transferID = transferID; self.checkpointID = checkpointID; self.lifecycle = lifecycle; self.phase = phase; self.direction = direction; self.kind = kind; self.transport = transport; self.completedBytes = completedBytes; self.totalBytes = totalBytes; self.prompt = prompt; self.verifiedFilePaths = verifiedFilePaths; self.verifiedNote = verifiedNote; self.shareURL = shareURL; self.serverAvailability = serverAvailability; self.exportStatus = exportStatus; self.error = error; self.actions = actions }
}

public struct TransferRecord: Codable, Hashable, Sendable, Identifiable {
    public let id: TransferID; public let direction: TransferDirection; public let kind: TransferKind; public let transport: Transport?; public let phase: TransferPhase; public let verifiedPrivately: Bool; public let exportStatus: ExportStatus; public let expiresAt: Date?; public let serverAvailability: ServerAvailability; public let actions: TransferActions
    public init(id: TransferID, direction: TransferDirection, kind: TransferKind, transport: Transport?, phase: TransferPhase, verifiedPrivately: Bool, exportStatus: ExportStatus, expiresAt: Date?, serverAvailability: ServerAvailability, actions: TransferActions) { self.id = id; self.direction = direction; self.kind = kind; self.transport = transport; self.phase = phase; self.verifiedPrivately = verifiedPrivately; self.exportStatus = exportStatus; self.expiresAt = expiresAt; self.serverAvailability = serverAvailability; self.actions = actions }
}

public struct ShareReceipt: Codable, Hashable, Sendable { public let transferID: TransferID; public let link: String; public let separateKey: String?; public let expiresAt: Date?; public let isLive: Bool; public let serverAvailability: ServerAvailability; public init(transferID: TransferID, link: String, separateKey: String?, expiresAt: Date?, isLive: Bool, serverAvailability: ServerAvailability) { self.transferID = transferID; self.link = link; self.separateKey = separateKey; self.expiresAt = expiresAt; self.isLive = isLive; self.serverAvailability = serverAvailability } }
public enum ReceiveIntentState: Codable, Hashable, Sendable { case ready, needsDecryptionKey, needsPassword, needsPeerConsent, needsOneTimeConsent, receiving, verifying, verifiedAwaitingExport, exported, failed(String) }
