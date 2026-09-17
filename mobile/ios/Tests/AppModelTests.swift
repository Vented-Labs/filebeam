import XCTest
@testable import Filebeam
import FilebeamDomain

@MainActor
final class AppModelTests: XCTestCase {
    func testReceiveExternalEntryStagesInputWithoutStartingTransfer() async {
        let model = AppModel(service: NoSuccessService(), instance: instance, outputDirectory: { URL(fileURLWithPath: "/tmp") })
        model.enqueueExternal(.receive(input: "https://filebeam.io/t/example#key", selectedInstance: instance))
        await model.stageExternalRequest()
        XCTAssertEqual(model.receiveInput, "https://filebeam.io/t/example#key")
        XCTAssertEqual(model.selectedTab, .receive)
        XCTAssertNil(model.externalRequests.active)
    }

    func testSharedFilesAreStagedUntilExplicitAcceptance() async {
        let model = AppModel(service: NoSuccessService(), instance: instance, outputDirectory: { URL(fileURLWithPath: "/tmp") })
        let source = FileSource(name: "report.pdf", location: "/tmp/report.pdf")
        model.enqueueExternal(.sharedFiles([source]))
        await model.stageExternalRequest()
        XCTAssertTrue(model.drafts.files.sources.isEmpty)
        model.acceptStagedSharedFiles()
        XCTAssertEqual(model.drafts.files.sources, [source])
        XCTAssertNil(model.externalRequests.active)
    }

    func testSharedTextRequiresAnExplicitMergeChoice() async {
        let model = AppModel(service: NoSuccessService(), instance: instance, outputDirectory: { URL(fileURLWithPath: "/tmp") })
        model.drafts.note.text = "Keep this"
        model.enqueueExternal(.sharedText(text: "Shared text", links: []))
        await model.stageExternalRequest()
        XCTAssertEqual(model.drafts.note.text, "Keep this")
        model.mergeStagedSharedText(replacing: false)
        XCTAssertEqual(model.drafts.note.text, "Keep this\n\nShared text")
        XCTAssertNil(model.externalRequests.active)
    }

    func testInstanceChangeRequiresConfirmationAndClearsRemoteState() async {
        var saved: AppSettings?
        let model = AppModel(service: NoSuccessService(), instance: instance, outputDirectory: { URL(fileURLWithPath: "/tmp") }, saveSettings: { saved = $0 })
        model.drafts.note.text = "retain this draft"
        model.session = AccountSession(id: 1, name: "User", username: "user", email: "user@example.com", emailVerifiedAt: nil, profileURL: nil, inboxEnabled: false, usernameRoutingEnabled: false, notificationChannel: "database")
        model.stageInstance(origin: "https://two.example")
        XCTAssertEqual(model.instance, instance)
        XCTAssertEqual(model.pendingInstance?.origin, "https://two.example")
        _ = await model.confirmInstanceChange()
        XCTAssertEqual(model.instance.origin, "https://two.example")
        XCTAssertNil(model.session)
        XCTAssertEqual(saved?.instanceOrigin, "https://two.example")
    }

    func testPromptsWithMatchingLocalIDsStayBoundToTheirJobs() async {
        let first = UUID(); let second = UUID()
        let service = NoSuccessService(snapshots: [first: snapshot(id: first), second: snapshot(id: second)])
        let model = AppModel(service: service, instance: instance, outputDirectory: { URL(fileURLWithPath: "/tmp") })
        await model.refresh(jobID: first)
        await model.refresh(jobID: second)
        guard let firstContext = model.activePrompt else { return XCTFail("Missing first prompt") }
        await model.dismissPrompt(firstContext)
        XCTAssertEqual(model.activePrompt?.jobID, first)
        await model.respond(context: firstContext, value: "first-password")
        guard let secondContext = model.activePrompt else { return XCTFail("Missing second prompt") }
        await model.respond(context: secondContext, value: "correct-password")
        XCTAssertEqual(service.respondedJobID, second)
        XCTAssertEqual(service.respondedValue, "correct-password")
    }

    private func snapshot(id: UUID) -> TransferSnapshot { TransferSnapshot(id: id, transferID: nil, checkpointID: "checkpoint", lifecycle: .running, phase: .awaitingInput, direction: .download, kind: .files, transport: .http, completedBytes: 0, totalBytes: nil, prompt: .init(id: 1, kind: .password)) }

    private var instance: FilebeamInstance { FilebeamInstance(origin: "https://filebeam.io")! }
}

private final class NoSuccessService: FilebeamService, @unchecked Sendable {
    var snapshots: [TransferJobID: TransferSnapshot]
    var respondedJobID: TransferJobID?
    var respondedValue: String?
    init(snapshots: [TransferJobID: TransferSnapshot] = [:]) { self.snapshots = snapshots }
    func discover(instance: FilebeamInstance) async throws -> InstancePolicy { InstancePolicy(instance: instance, anonymousUploads: true, enabledTransports: [.http], defaultDriver: "http", chunkBytes: 64, retentionOptionsHours: [], drivers: ["http": DriverPolicy(name: "http", maximumTransferBytes: .unlimited, maximumFileCount: .unlimited, maximumNoteBytes: .unlimited)]) }
    func inspectReceive(instance: FilebeamInstance, input: String) async throws -> InputRoute { fatalError() }
    func startUpload(_ request: UploadStartRequest) async throws -> TransferSnapshot { fatalError() }
    func startNote(_ request: NoteStartRequest) async throws -> TransferSnapshot { fatalError() }
    func inspectNote(_ input: String) async throws -> NoteInspection { fatalError() }
    func startReceiveNote(_ request: NoteReceiveRequest) async throws -> TransferSnapshot { fatalError() }
    func takeVerifiedNote(jobID: TransferJobID) async throws -> VerifiedNote? { fatalError() }
    func retryNoteBurn(jobID: TransferJobID) async throws -> Bool { fatalError() }
    func receive(_ request: ReceiveRequest) async throws -> TransferSnapshot { fatalError() }
    func startInboxReceive(_ request: InboxReceiveRequest) async throws -> TransferSnapshot { fatalError() }
    func resumeInboxReceive(_ request: InboxResumeRequest) async throws -> TransferSnapshot { fatalError() }
    func snapshot(jobID: TransferJobID) async throws -> TransferSnapshot { guard let value = snapshots[jobID] else { fatalError() }; return value }
    func savedTransfers() async throws -> [TransferRecord] { fatalError() }
    func pause(jobID: TransferJobID) async throws {}
    func resume(transferID: TransferID) async throws -> TransferSnapshot { fatalError() }
    func discardLocal(transferID: TransferID) async throws { fatalError() }
    func revokeRemote(transferID: TransferID) async throws -> TransferSnapshot { fatalError() }
    func endLive(transferID: TransferID) async throws -> TransferSnapshot { fatalError() }
    func respondToConsent(jobID: TransferJobID, promptID: UInt64, allowed: Bool) async throws { fatalError() }
    func respondToPrompt(jobID: TransferJobID, promptID: UInt64, value: String) async throws { respondedJobID = jobID; respondedValue = value }
    func login(_ request: LoginRequest) async throws -> AccountSession { fatalError() }
    func register(_ request: RegistrationRequest) async throws -> AccountSession { fatalError() }
    func logout(instance: FilebeamInstance) async throws { fatalError() }
    func session(instance: FilebeamInstance) async throws -> AccountSession { throw FilebeamDomainError.unavailable("No test session") }
    func requestPasswordReset(instance: FilebeamInstance, email: String) async throws { fatalError() }
    func resetPassword(instance: FilebeamInstance, email: String, token: String, password: String) async throws { fatalError() }
    func resendVerification(instance: FilebeamInstance) async throws { fatalError() }
    func verifyEmail(link: String) async throws { fatalError() }
    func recipient(instance: FilebeamInstance, username: String) async throws -> Recipient { fatalError() }
    func inbox(instance: FilebeamInstance) async throws -> [InboxItem] { fatalError() }
    func inboxUnreadCount(instance: FilebeamInstance) async throws -> InboxUnreadCount { fatalError() }
    func markInboxNotificationsRead(instance: FilebeamInstance) async throws { fatalError() }
    func deleteInboxItem(instance: FilebeamInstance, transferID: TransferID) async throws { fatalError() }
    func inboxMetadata(instance: FilebeamInstance, transferID: TransferID) async throws -> InboxMetadata { fatalError() }
    func openInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data) async throws -> OpenedInbox { fatalError() }
    func receiveInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data, outputDirectory: String) async throws -> TransferSnapshot { fatalError() }
    func accountKeys(instance: FilebeamInstance) async throws -> [AccountKeyBundle] { fatalError() }
    func keySituation(instance: FilebeamInstance) async throws -> KeySituation { fatalError() }
    func generateSelfCustodyKey() async throws -> GeneratedAccountKey { fatalError() }
    func wrapPasswordCustodyKey(privateKey: Data, password: String, userID: UInt64, publicKey: String) async throws -> String { fatalError() }
    func unwrapPasswordCustodyKey(envelope: String, password: String, userID: UInt64, publicKey: String) async throws -> Data { fatalError() }
    func validateAccountKeyUpload(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyUploadValidation { fatalError() }
    func uploadAccountKey(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyBundle { fatalError() }
    func exportSelfCustodyKey(_ privateKey: Data) async throws -> String { fatalError() }
    func importSelfCustodyKey(_ encoded: String) async throws -> Data { fatalError() }
    func preferences(instance: FilebeamInstance) async throws -> AccountPreferences { fatalError() }
    func updatePreferences(instance: FilebeamInstance, preferences: AccountPreferences) async throws { fatalError() }
    func receipt(transferID: TransferID, includeKey: Bool) async throws -> ShareReceipt { fatalError() }
    func inspectInvite(link: String) async throws -> InviteInspection { fatalError() }
    func acceptInvite(_ request: InviteAcceptanceRequest) async throws -> AccountSession { fatalError() }
    func report(_ request: ReportRequest) async throws -> ReportReceipt { fatalError() }
    func requestAccountDeletion(_ request: AccountDeletionRequest) async throws -> AccountDeletionResult { fatalError() }
}
