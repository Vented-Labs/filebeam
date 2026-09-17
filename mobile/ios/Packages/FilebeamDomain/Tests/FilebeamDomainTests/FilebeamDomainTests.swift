import XCTest
@testable import FilebeamDomain

final class FilebeamDomainTests: XCTestCase {
    private let instance = FilebeamInstance(origin: "https://one.example")!
    private var policy: InstancePolicy {
        InstancePolicy(instance: instance, anonymousUploads: true, enabledTransports: [.http, .webRTC], defaultDriver: "http", chunkBytes: 64, retentionOptionsHours: [24], drivers: [
            "http": DriverPolicy(name: "http", maximumTransferBytes: .finite(130), maximumFileCount: .finite(2), maximumNoteBytes: .finite(10)),
            "unlimited": DriverPolicy(name: "unlimited", maximumTransferBytes: .unlimited, maximumFileCount: .unlimited, maximumNoteBytes: .unlimited)
        ])
    }

    func testFileValidationAccountsForCiphertextOverheadAndPreservesConflictingOptions() {
        let recipient = Recipient(userID: 1, username: "mason", keyBundleID: 2, publicKey: "key", fingerprint: "fp")
        let options = FileTransferOptions(transport: .http, turbo: true, passwordEnabled: true, recipient: recipient)
        let draft = FileDraft(sources: [FileSource(name: "a", location: "/tmp/a", sizeBytes: 100)], options: options)
        let results = SendPolicy.validate(draft, policy: policy, isAuthenticated: true)
        XCTAssertTrue(results.contains(.blocked("The encrypted selection exceeds this driver's byte limit.")))
        XCTAssertTrue(results.contains(.blocked("Recipient delivery cannot use Turbo Transfer.")))
        XCTAssertTrue(results.contains(.blocked("Recipient delivery cannot use a transfer password.")))
        XCTAssertEqual(draft.options, options)
    }

    func testUnknownAndUnlimitedLimitsAreDistinct() {
        let pending = FileDraft(sources: [FileSource(name: "a", location: "/tmp/a", state: .sizePending)])
        XCTAssertTrue(SendPolicy.validate(pending, policy: policy, isAuthenticated: true).contains(.unknown("One or more source sizes are pending or unavailable.")))
        let note = NoteDraft(text: String(repeating: "x", count: 11), options: .init(driver: "http"))
        XCTAssertTrue(SendPolicy.validate(note, policy: policy, isAuthenticated: true).contains(.blocked("The encrypted note exceeds this driver's byte limit.")))
    }

    func testWebRTCUsesItsOwnDriverAndArchiveCanBePrepared() {
        let policy = InstancePolicy(instance: instance, anonymousUploads: true, enabledTransports: [.http, .webRTC], defaultDriver: "http", chunkBytes: 64, retentionOptionsHours: [24], drivers: [
            "http": DriverPolicy(name: "http", maximumTransferBytes: .finite(1), maximumFileCount: .finite(1), maximumNoteBytes: .finite(1)),
            "webrtc": DriverPolicy(name: "webrtc", maximumTransferBytes: .unlimited, maximumFileCount: .finite(1), maximumNoteBytes: .unlimited)
        ])
        let draft = FileDraft(sources: [FileSource(name: "a", location: "/tmp/a", sizeBytes: 100)], options: .init(transport: .webRTC, archive: true))
        let validation = SendPolicy.validate(draft, policy: policy, isAuthenticated: true)
        XCTAssertFalse(validation.contains(.blocked("The encrypted selection exceeds this driver's byte limit.")))
        XCTAssertFalse(validation.contains(.unknown("Encrypted archive size is unknown until planning.")))
    }

    func testCiphertextCountsAnAeadTagForEmptyFilesAndRejectsProtocolBounds() {
        XCTAssertEqual(SendPolicy.ciphertextBytes(fileSizes: [0], chunkBytes: 64), 16)
        XCTAssertNil(SendPolicy.ciphertextBytes(fileSizes: [65_536], chunkBytes: 1))
        XCTAssertNil(SendPolicy.ciphertextBytes(fileSizes: [1], chunkBytes: 25_000_000))
    }

    func testNoteAndFileLifetimesAreIndependent() {
        let policy = InstancePolicy(instance: instance, anonymousUploads: true, enabledTransports: [.http], defaultDriver: "http", chunkBytes: 64, retentionOptionsHours: [24], drivers: ["http": DriverPolicy(name: "http", maximumTransferBytes: .unlimited, maximumFileCount: .unlimited, maximumNoteBytes: .unlimited)], noteRetentionOptionsHours: [48])
        XCTAssertTrue(SendPolicy.validate(FileDraft(sources: [FileSource(name: "a", location: "/tmp/a", sizeBytes: 1)], options: .init(retentionHours: 24)), policy: policy, isAuthenticated: true).isEmpty)
        XCTAssertTrue(SendPolicy.validate(NoteDraft(text: "x", options: .init(retentionHours: 24)), policy: policy, isAuthenticated: true).contains(.blocked("The selected lifetime is unavailable.")))
    }

    func testUnknownByteLimitIsNotTreatedAsUnlimited() {
        let policy = InstancePolicy(instance: instance, anonymousUploads: true, enabledTransports: [.http], defaultDriver: "http", chunkBytes: 64, retentionOptionsHours: [], drivers: ["http": DriverPolicy(name: "http", maximumTransferBytes: .unknown, maximumFileCount: .unlimited, maximumNoteBytes: .unknown)])
        let draft = FileDraft(sources: [FileSource(name: "a", location: "/tmp/a", sizeBytes: 1)])
        XCTAssertTrue(SendPolicy.validate(draft, policy: policy, isAuthenticated: true).contains(.unknown("The driver's byte limit is unavailable.")))
    }

    func testTransferRoutePreservesFullFragmentAndUsesLinkInstance() {
        let input = "https://other.example/t/transfer-7#decryption-key"
        guard case let .transfer(id, routedInstance, original) = InputRouter.route(input, selectedInstance: instance) else { return XCTFail("Expected transfer") }
        XCTAssertEqual(id, "transfer-7")
        XCTAssertEqual(routedInstance.origin, "https://other.example")
        XCTAssertEqual(original, input)
        guard case let .transfer(_, bareInstance, _) = InputRouter.route("bare-id", selectedInstance: instance) else { return XCTFail("Expected bare transfer") }
        XCTAssertEqual(bareInstance, instance)
    }

    func testInstanceRejectsUntrustedURLComponentsIncludingWhenDecoded() throws {
        XCTAssertNil(FilebeamInstance(origin: "https://one.example/path"))
        XCTAssertNil(FilebeamInstance(origin: "https://user@one.example"))
        XCTAssertNil(FilebeamInstance(origin: "http://one.example"))
        XCTAssertThrowsError(try JSONDecoder().decode(FilebeamInstance.self, from: Data("\"https://one.example/#key\"".utf8)))
    }

    func testOneTimeConsentIsAnIntentStateAndExternalRequestsQueue() {
        XCTAssertEqual(ReceiveIntentState.needsOneTimeConsent, .needsOneTimeConsent)
        var coordinator = ExternalRequestCoordinator()
        let receive = ExternalRequest.receive(input: "id", selectedInstance: instance)
        let reset = ExternalRequest.resetPassword(url: "https://one.example/reset")
        coordinator.enqueue(receive)
        coordinator.enqueue(reset)
        XCTAssertEqual(coordinator.active, receive)
        XCTAssertEqual(coordinator.queued, [reset])
        coordinator.completeActive()
        XCTAssertEqual(coordinator.active, reset)
    }

    func testOptionsCopyCanBeCancelledWithoutChangingIndependentDrafts() {
        let original = ComposerDrafts(files: FileDraft(options: .init(turbo: false)), note: NoteDraft(text: "keep", options: .init(burnOnRead: true)))
        var files = FileOptionsTransaction(options: original.files.options)
        var note = NoteOptionsTransaction(options: original.note.options)
        files.draft.turbo = true
        note.draft.burnOnRead = false
        XCTAssertFalse(files.cancel().turbo)
        XCTAssertTrue(note.cancel().burnOnRead)
        XCTAssertTrue(files.commit().turbo)
        XCTAssertEqual(original.note.text, "keep")
    }

    func testVerifiedAndExportedActionsRemainHonest() {
        let verified = TransferRecord(id: "v", direction: .download, kind: .files, transport: .http, phase: .verifiedAwaitingExport, verifiedPrivately: true, exportStatus: .notExported, expiresAt: nil, serverAvailability: .available, actions: .init(retryExport: true))
        XCTAssertTrue(verified.verifiedPrivately)
        XCTAssertEqual(verified.exportStatus, .notExported)
        XCTAssertTrue(verified.actions.retryExport)
        let exported = TransferRecord(id: "e", direction: .download, kind: .files, transport: .http, phase: .complete, verifiedPrivately: true, exportStatus: .exported, expiresAt: nil, serverAvailability: .available, actions: .init())
        XCTAssertEqual(exported.exportStatus, .exported)
        XCTAssertFalse(exported.actions.retryExport)
    }

    func testFileSelectionPromptRetainsOnlyAuthenticatedItemIdentifiers() throws {
        let items = [TransferSelectionItem(id: "manifest-a", name: "a.txt", size: 1), TransferSelectionItem(id: "manifest-b", name: "b.txt", size: 2)]
        let prompt = TransferPrompt(id: 0x8000_0000_0000_0001, kind: .filesSelection, message: "Choose files", selectionItems: items)
        let restored = try JSONDecoder().decode(TransferPrompt.self, from: JSONEncoder().encode(prompt))
        XCTAssertEqual(restored.kind, .filesSelection)
        XCTAssertEqual(restored.selectionItems.map(\.id), ["manifest-a", "manifest-b"])
    }
}
