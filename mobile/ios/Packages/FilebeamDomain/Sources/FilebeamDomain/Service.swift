import Foundation

// Request types intentionally are Sendable, not Codable: callers must not serialize passwords, tokens, keys, or cookies.
public struct UploadStartRequest: Sendable { public let instance: FilebeamInstance; public let paths: [String]; public let sources: [FileSource]; public let options: FileTransferOptions; public let password: String?; public init(instance: FilebeamInstance, paths: [String], sources: [FileSource] = [], options: FileTransferOptions, password: String? = nil) { self.instance = instance; self.paths = paths; self.sources = sources; self.options = options; self.password = password } }
public struct NoteStartRequest: Sendable { public let instance: FilebeamInstance; public let draft: NoteDraft; public let password: String?; public init(instance: FilebeamInstance, draft: NoteDraft, password: String? = nil) { self.instance = instance; self.draft = draft; self.password = password } }
public struct ReceiveRequest: Sendable { public let instance: FilebeamInstance; public let input: String; public let outputDirectory: String; public init(instance: FilebeamInstance, input: String, outputDirectory: String) { self.instance = instance; self.input = input; self.outputDirectory = outputDirectory } }
public struct LoginRequest: Sendable { public let instance: FilebeamInstance; public let email: String; public let password: String; public let remember: Bool; public init(instance: FilebeamInstance, email: String, password: String, remember: Bool) { self.instance = instance; self.email = email; self.password = password; self.remember = remember } }
public struct RegistrationRequest: Sendable { public let instance: FilebeamInstance; public let username: String; public let name: String?; public let email: String; public let password: String; public init(instance: FilebeamInstance, username: String, name: String?, email: String, password: String) { self.instance = instance; self.username = username; self.name = name; self.email = email; self.password = password } }

public enum NoteTransport: String, Codable, Sendable { case http, webRTC }
public struct NoteInspection: Codable, Hashable, Sendable { public let id: TransferID; public let status: String; public let burnOnRead: Bool; public let transport: NoteTransport; public let passwordRequired: Bool; public init(id: TransferID, status: String, burnOnRead: Bool, transport: NoteTransport, passwordRequired: Bool) { self.id = id; self.status = status; self.burnOnRead = burnOnRead; self.transport = transport; self.passwordRequired = passwordRequired } }
public struct NoteReceiveRequest: Sendable { public let instance: FilebeamInstance; public let input: String; public let separateKey: String?; public let password: String?; public let burnAcknowledged: Bool; public init(instance: FilebeamInstance, input: String, separateKey: String? = nil, password: String? = nil, burnAcknowledged: Bool) { self.instance = instance; self.input = input; self.separateKey = separateKey; self.password = password; self.burnAcknowledged = burnAcknowledged } }

public struct InboxReceiveRequest: Sendable { public let instance: FilebeamInstance; public let transferID: TransferID; public let workingKey: Data; public let cookieContext: String; public let outputDirectory: String; public init(instance: FilebeamInstance, transferID: TransferID, workingKey: Data, cookieContext: String, outputDirectory: String) { self.instance = instance; self.transferID = transferID; self.workingKey = workingKey; self.cookieContext = cookieContext; self.outputDirectory = outputDirectory } }
public struct InboxResumeRequest: Sendable { public let checkpointID: String; public let instance: FilebeamInstance; public let workingKey: Data; public let cookieContext: String; public init(checkpointID: String, instance: FilebeamInstance, workingKey: Data, cookieContext: String) { self.checkpointID = checkpointID; self.instance = instance; self.workingKey = workingKey; self.cookieContext = cookieContext } }

public struct GeneratedAccountKey: Sendable { public let privateKey: Data; public let publicKey: String; public let fingerprint: String; public init(privateKey: Data, publicKey: String, fingerprint: String) { self.privateKey = privateKey; self.publicKey = publicKey; self.fingerprint = fingerprint } }
public struct AccountKeyUploadRequest: Sendable { public let publicKey: String; public let fingerprint: String; public let custody: KeyCustody; public let encryptedPrivateKey: String?; public let currentPassword: String?; public let replace: Bool; public init(publicKey: String, fingerprint: String, custody: KeyCustody, encryptedPrivateKey: String?, currentPassword: String?, replace: Bool) { self.publicKey = publicKey; self.fingerprint = fingerprint; self.custody = custody; self.encryptedPrivateKey = encryptedPrivateKey; self.currentPassword = currentPassword; self.replace = replace } }
public struct AccountKeyUploadValidation: Codable, Hashable, Sendable { public let isValid: Bool; public let reason: String?; public let replacementAcknowledgementRequired: Bool; public init(isValid: Bool, reason: String?, replacementAcknowledgementRequired: Bool) { self.isValid = isValid; self.reason = reason; self.replacementAcknowledgementRequired = replacementAcknowledgementRequired } }

public struct InboxUnreadCount: Codable, Hashable, Sendable { public let count: UInt64; public init(count: UInt64) { self.count = count } }
public struct InviteInspection: Codable, Hashable, Sendable { public let instance: FilebeamInstance; public let invitationID: String; public let email: String?; public let expiresAt: Date?; public init(instance: FilebeamInstance, invitationID: String, email: String?, expiresAt: Date?) { self.instance = instance; self.invitationID = invitationID; self.email = email; self.expiresAt = expiresAt } }
public struct InviteAcceptanceRequest: Sendable { public let link: String; public let username: String; public let name: String?; public let email: String; public let password: String; public init(link: String, username: String, name: String?, email: String, password: String) { self.link = link; self.username = username; self.name = name; self.email = email; self.password = password } }
public enum ReportCategory: String, Codable, CaseIterable, Sendable { case spam, malware, illegalContent = "illegal_content", privacy, copyright, other }
public struct ReportRequest: Sendable { public let instance: FilebeamInstance; public let transferID: TransferID; public let category: ReportCategory; public let description: String; public let email: String?; public init(instance: FilebeamInstance, transferID: TransferID, category: ReportCategory, description: String, email: String? = nil) { self.instance = instance; self.transferID = transferID; self.category = category; self.description = description; self.email = email } }
public struct ReportReceipt: Codable, Hashable, Sendable { public let reportID: String?; public let submittedAt: Date?; public init(reportID: String?, submittedAt: Date?) { self.reportID = reportID; self.submittedAt = submittedAt } }
public struct AccountDeletionRequest: Sendable { public let instance: FilebeamInstance; public let currentPassword: String; public let confirmation: String; public init(instance: FilebeamInstance, currentPassword: String, confirmation: String) { self.instance = instance; self.currentPassword = currentPassword; self.confirmation = confirmation } }
public struct AccountDeletionResult: Codable, Hashable, Sendable { public let accepted: Bool; public let effectiveAt: Date?; public init(accepted: Bool, effectiveAt: Date?) { self.accepted = accepted; self.effectiveAt = effectiveAt } }

/// Bridge implementation delegates to UniFFI/current backend APIs. It must not synthesize success, policy, export, or background states.
public protocol FilebeamService: Sendable {
    func discover(instance: FilebeamInstance) async throws -> InstancePolicy
    func inspectReceive(instance: FilebeamInstance, input: String) async throws -> InputRoute
    func inspectPayload(instance: FilebeamInstance, input: String) async throws -> TransferInspection
    func startUpload(_ request: UploadStartRequest) async throws -> TransferSnapshot
    func startNote(_ request: NoteStartRequest) async throws -> TransferSnapshot
    func inspectNote(_ input: String) async throws -> NoteInspection
    func startReceiveNote(_ request: NoteReceiveRequest) async throws -> TransferSnapshot
    func takeVerifiedNote(jobID: TransferJobID) async throws -> VerifiedNote?
    func retryNoteBurn(jobID: TransferJobID) async throws -> Bool
    func receive(_ request: ReceiveRequest) async throws -> TransferSnapshot
    func startInboxReceive(_ request: InboxReceiveRequest) async throws -> TransferSnapshot
    func resumeInboxReceive(_ request: InboxResumeRequest) async throws -> TransferSnapshot
    func snapshot(jobID: TransferJobID) async throws -> TransferSnapshot
    func transferActivity(checkpointID: String) async throws -> TransferActivity
    func savedTransfers() async throws -> [TransferRecord]
    func pause(jobID: TransferJobID) async throws
    func resume(transferID: TransferID) async throws -> TransferSnapshot
    func discardLocal(transferID: TransferID) async throws
    /// Call only after the system document picker has successfully exported all paths.
    func recordExport(jobID: TransferJobID, paths: [String]) async throws
    func revokeRemote(transferID: TransferID) async throws -> TransferSnapshot
    func endLive(transferID: TransferID) async throws -> TransferSnapshot
    func respondToConsent(jobID: TransferJobID, promptID: UInt64, allowed: Bool) async throws
    func respondToPrompt(jobID: TransferJobID, promptID: UInt64, value: String) async throws
    func login(_ request: LoginRequest) async throws -> AccountSession
    func register(_ request: RegistrationRequest) async throws -> AccountSession
    func logout(instance: FilebeamInstance) async throws
    func session(instance: FilebeamInstance) async throws -> AccountSession
    func requestPasswordReset(instance: FilebeamInstance, email: String) async throws
    func resetPassword(instance: FilebeamInstance, email: String, token: String, password: String) async throws
    func resendVerification(instance: FilebeamInstance) async throws
    func verifyEmail(link: String) async throws
    func recipient(instance: FilebeamInstance, username: String) async throws -> Recipient
    func inbox(instance: FilebeamInstance) async throws -> [InboxItem]
    func inboxUnreadCount(instance: FilebeamInstance) async throws -> InboxUnreadCount
    func markInboxNotificationsRead(instance: FilebeamInstance) async throws
    func deleteInboxItem(instance: FilebeamInstance, transferID: TransferID) async throws
    func inboxMetadata(instance: FilebeamInstance, transferID: TransferID) async throws -> InboxMetadata
    func openInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data) async throws -> OpenedInbox
    /// Opens the recipient key in-process and starts an authenticated download without exposing cookies to UI code.
    func receiveInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data, outputDirectory: String) async throws -> TransferSnapshot
    func accountKeys(instance: FilebeamInstance) async throws -> [AccountKeyBundle]
    func keySituation(instance: FilebeamInstance) async throws -> KeySituation
    func generateSelfCustodyKey() async throws -> GeneratedAccountKey
    func wrapPasswordCustodyKey(privateKey: Data, password: String, userID: UInt64, publicKey: String) async throws -> String
    func unwrapPasswordCustodyKey(envelope: String, password: String, userID: UInt64, publicKey: String) async throws -> Data
    func validateAccountKeyUpload(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyUploadValidation
    func uploadAccountKey(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyBundle
    func exportSelfCustodyKey(_ privateKey: Data) async throws -> String
    func importSelfCustodyKey(_ encoded: String) async throws -> Data
    func preferences(instance: FilebeamInstance) async throws -> AccountPreferences
    func updatePreferences(instance: FilebeamInstance, preferences: AccountPreferences) async throws
    func receipt(transferID: TransferID, includeKey: Bool) async throws -> ShareReceipt
    func downloadCommand(link: String) async throws -> String
    func inspectInvite(link: String) async throws -> InviteInspection
    func acceptInvite(_ request: InviteAcceptanceRequest) async throws -> AccountSession
    func report(_ request: ReportRequest) async throws -> ReportReceipt
    func requestAccountDeletion(_ request: AccountDeletionRequest) async throws -> AccountDeletionResult
}

public extension FilebeamService {
    /// Existing adapters remain source-compatible while they migrate their local export catalog.
    func recordExport(jobID: TransferJobID, paths: [String]) async throws { throw FilebeamDomainError.storage("The export receipt could not be retained.") }
    func inspectPayload(instance: FilebeamInstance, input: String) async throws -> TransferInspection { throw FilebeamDomainError.unavailable("Transfer inspection is not available in this build.") }
    func downloadCommand(link: String) async throws -> String { throw FilebeamDomainError.unavailable("CLI download formatting is not available in this build.") }
    func transferActivity(checkpointID: String) async throws -> TransferActivity { throw FilebeamDomainError.unavailable("Transfer activity is not available in this build.") }
}
