#if DEBUG
import SwiftUI
import FilebeamDomain

/// Deliberately has no network behavior; previews can compose production views safely.
final class PreviewService: FilebeamService, @unchecked Sendable {
    private func unavailable<T>() throws -> T { throw FilebeamDomainError.unavailable("Preview data is not available.") }
    func discover(instance: FilebeamInstance) async throws -> InstancePolicy {
        InstancePolicy(instance: instance, anonymousUploads: true, enabledTransports: [.http, .webRTC], defaultDriver: "http", chunkBytes: 4_194_304, retentionOptionsHours: [1, 24, 168], drivers: ["http": DriverPolicy(name: "http", maximumTransferBytes: .finite(2_147_483_648), maximumFileCount: .finite(100), maximumNoteBytes: .finite(65_536)), "webrtc": DriverPolicy(name: "webrtc", maximumTransferBytes: .unlimited, maximumFileCount: .finite(100), maximumNoteBytes: .finite(65_536))], registrationEnabled: true, usernameRoutingEnabled: true, noteRetentionOptionsHours: [1, 24, 720], fileDefaultRetentionHours: 24, noteDefaultRetentionHours: 720, branding: InstanceBranding(name: "Filebeam", version: "1.4.0", changelogEnabled: true))
    }
    func inspectReceive(instance: FilebeamInstance, input: String) async throws -> InputRoute { try unavailable() }
    func startUpload(_ request: UploadStartRequest) async throws -> TransferSnapshot { try unavailable() }
    func startNote(_ request: NoteStartRequest) async throws -> TransferSnapshot { try unavailable() }
    func inspectNote(_ input: String) async throws -> NoteInspection { try unavailable() }
    func startReceiveNote(_ request: NoteReceiveRequest) async throws -> TransferSnapshot { try unavailable() }
    func takeVerifiedNote(jobID: TransferJobID) async throws -> VerifiedNote? { try unavailable() }
    func retryNoteBurn(jobID: TransferJobID) async throws -> Bool { try unavailable() }
    func receive(_ request: ReceiveRequest) async throws -> TransferSnapshot { try unavailable() }
    func startInboxReceive(_ request: InboxReceiveRequest) async throws -> TransferSnapshot { try unavailable() }
    func resumeInboxReceive(_ request: InboxResumeRequest) async throws -> TransferSnapshot { try unavailable() }
    func snapshot(jobID: TransferJobID) async throws -> TransferSnapshot { try unavailable() }
    func savedTransfers() async throws -> [TransferRecord] { [] }
    func pause(jobID: TransferJobID) async throws { try unavailable() as Void }
    func resume(transferID: TransferID) async throws -> TransferSnapshot { try unavailable() }
    func discardLocal(transferID: TransferID) async throws { try unavailable() as Void }
    func revokeRemote(transferID: TransferID) async throws -> TransferSnapshot { try unavailable() }
    func endLive(transferID: TransferID) async throws -> TransferSnapshot { try unavailable() }
    func respondToConsent(jobID: TransferJobID, promptID: UInt64, allowed: Bool) async throws { try unavailable() as Void }
    func respondToPrompt(jobID: TransferJobID, promptID: UInt64, value: String) async throws { try unavailable() as Void }
    func login(_ request: LoginRequest) async throws -> AccountSession { try unavailable() }
    func register(_ request: RegistrationRequest) async throws -> AccountSession { try unavailable() }
    func logout(instance: FilebeamInstance) async throws { try unavailable() as Void }
    func session(instance: FilebeamInstance) async throws -> AccountSession { try unavailable() }
    func requestPasswordReset(instance: FilebeamInstance, email: String) async throws { try unavailable() as Void }
    func resetPassword(instance: FilebeamInstance, email: String, token: String, password: String) async throws { try unavailable() as Void }
    func resendVerification(instance: FilebeamInstance) async throws { try unavailable() as Void }
    func verifyEmail(link: String) async throws { try unavailable() as Void }
    func recipient(instance: FilebeamInstance, username: String) async throws -> Recipient { try unavailable() }
    func inbox(instance: FilebeamInstance) async throws -> [InboxItem] { try unavailable() }
    func inboxUnreadCount(instance: FilebeamInstance) async throws -> InboxUnreadCount { try unavailable() }
    func markInboxNotificationsRead(instance: FilebeamInstance) async throws { try unavailable() as Void }
    func deleteInboxItem(instance: FilebeamInstance, transferID: TransferID) async throws { try unavailable() as Void }
    func inboxMetadata(instance: FilebeamInstance, transferID: TransferID) async throws -> InboxMetadata { try unavailable() }
    func openInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data) async throws -> OpenedInbox { try unavailable() }
    func receiveInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data, outputDirectory: String) async throws -> TransferSnapshot { try unavailable() }
    func accountKeys(instance: FilebeamInstance) async throws -> [AccountKeyBundle] { try unavailable() }
    func keySituation(instance: FilebeamInstance) async throws -> KeySituation { try unavailable() }
    func generateSelfCustodyKey() async throws -> GeneratedAccountKey { try unavailable() }
    func wrapPasswordCustodyKey(privateKey: Data, password: String, userID: UInt64, publicKey: String) async throws -> String { try unavailable() }
    func unwrapPasswordCustodyKey(envelope: String, password: String, userID: UInt64, publicKey: String) async throws -> Data { try unavailable() }
    func validateAccountKeyUpload(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyUploadValidation { try unavailable() }
    func uploadAccountKey(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyBundle { try unavailable() }
    func exportSelfCustodyKey(_ privateKey: Data) async throws -> String { try unavailable() }
    func importSelfCustodyKey(_ encoded: String) async throws -> Data { try unavailable() }
    func preferences(instance: FilebeamInstance) async throws -> AccountPreferences { try unavailable() }
    func updatePreferences(instance: FilebeamInstance, preferences: AccountPreferences) async throws { try unavailable() as Void }
    func receipt(transferID: TransferID, includeKey: Bool) async throws -> ShareReceipt { try unavailable() }
    func inspectInvite(link: String) async throws -> InviteInspection { try unavailable() }
    func acceptInvite(_ request: InviteAcceptanceRequest) async throws -> AccountSession { try unavailable() }
    func report(_ request: ReportRequest) async throws -> ReportReceipt { try unavailable() }
    func requestAccountDeletion(_ request: AccountDeletionRequest) async throws -> AccountDeletionResult { try unavailable() }
}

@MainActor
enum PreviewModels {
    static func app() -> AppModel {
        AppModel(service: PreviewService(), instance: FilebeamInstance(origin: "https://filebeam.io")!, outputDirectory: { FileManager.default.temporaryDirectory })
    }
}

#Preview("Shell") {
    AppShell(model: PreviewModels.app())
}
#endif
