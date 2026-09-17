#if DEBUG
import SwiftUI
import FilebeamDomain

/// Deliberately has no network behavior; full-screen previews can opt into it safely.
final class PreviewService: FilebeamService, @unchecked Sendable {
    private func unavailable<T>() throws -> T { throw FilebeamDomainError.unavailable("Preview data is not available.") }
    func discover(instance: FilebeamInstance) async throws -> InstancePolicy { try unavailable() }
    func inspectReceive(instance: FilebeamInstance, input: String) async throws -> InputRoute { try unavailable() }
    func inspectPayload(instance: FilebeamInstance, input: String) async throws -> TransferInspection { try unavailable() }
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
    func transferActivity(checkpointID: String) async throws -> TransferActivity { try unavailable() }
    func savedTransfers() async throws -> [TransferRecord] { try unavailable() }
    func pause(jobID: TransferJobID) async throws { try unavailable() as Void }
    func resume(transferID: TransferID) async throws -> TransferSnapshot { try unavailable() }
    func discardLocal(transferID: TransferID) async throws { try unavailable() as Void }
    func recordExport(jobID: TransferJobID, paths: [String]) async throws { try unavailable() as Void }
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
    func downloadCommand(link: String) async throws -> String { try unavailable() }
    func inspectInvite(link: String) async throws -> InviteInspection { try unavailable() }
    func acceptInvite(_ request: InviteAcceptanceRequest) async throws -> AccountSession { try unavailable() }
    func report(_ request: ReportRequest) async throws -> ReportReceipt { try unavailable() }
    func requestAccountDeletion(_ request: AccountDeletionRequest) async throws -> AccountDeletionResult { try unavailable() }
}

// Previews intentionally compose the production views rather than a parallel demo UI.
struct FeaturePreviewCatalogue: View {
    private let receiving = TransferSnapshot(
        id: UUID(), transferID: TransferID("preview-transfer"), checkpointID: "preview-checkpoint",
        lifecycle: .running, phase: .receiving, direction: .download, kind: .files,
        transport: .http, completedBytes: 50, totalBytes: 100
    )

    var body: some View {
        List {
            Section("Receiving") { TransferCard(snapshot: receiving); ReceiveProgress(snapshot: receiving) }
            Section("Verified") {
                ReceiveProgress(snapshot: TransferSnapshot(id: UUID(), transferID: nil, checkpointID: nil, lifecycle: .complete, phase: .verifiedAwaitingExport, direction: .download, kind: .note, transport: .http, completedBytes: 100, totalBytes: 100))
            }
        }
    }
}

#Preview("Feature catalogue") { FeaturePreviewCatalogue() }
#endif
