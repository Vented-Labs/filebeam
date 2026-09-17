import XCTest
@testable import FilebeamBackgroundPortable

final class BackgroundJournalPortableTests: XCTestCase {
    func testDescriptorRoundTripsWithoutSecrets() throws {
        let descriptor = try BackgroundTaskDescriptor(operationID: "operation-1", checkpointID: "checkpoint-1", direction: .download)
        XCTAssertEqual(BackgroundTaskDescriptor.parse(descriptor.taskDescription), descriptor)
        XCTAssertNil(BackgroundTaskDescriptor.parse("https://cookie@example.invalid"))
    }

    func testReconcileDropsVanishedTaskButKeepsStagedWork() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .upload)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: 12, expectedResponseBytes: 0, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 1)])
        try journal.reconcile([])
        XCTAssertNil(journal.entry(forTaskIdentifier: 12))
        XCTAssertEqual(journal.entry(operationID: "op")?.state, .scheduled)
    }

    func testRejectsUntrustedSpoolPath() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .download)
        let disk = "{\"entries\":[{\"descriptor\":{\"operationID\":\"op\",\"checkpointID\":\"checkpoint\",\"direction\":\"download\"},\"taskIdentifier\":1,\"expectedResponseBytes\":1,\"spoolName\":\"../../etc/passwd\",\"responseHeaders\":{},\"state\":\"scheduled\",\"networkDone\":0}]}"
        _ = descriptor
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data(disk.utf8).write(to: directory.appendingPathComponent("background-http-v1.json"))
        XCTAssertThrowsError(try BackgroundJournal(directory: directory))
    }

    func testQueuePlanLimitsEachCheckpointToSixtyFourOperations() throws {
        let entries = (0 ..< BackgroundJournal.maximumEntries).map { index in
            let descriptor = try! BackgroundTaskDescriptor(operationID: "old-\(index)", checkpointID: "other", direction: .upload)
            return BackgroundJournal.Entry(descriptor: descriptor, taskIdentifier: index, expectedResponseBytes: 0, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)
        }
        let work = (0 ... BackgroundJournal.maximumEntries).map { "new-\($0)" }
        XCTAssertEqual(BackgroundQueuePlan.operationIDs(workOperationIDs: work, entries: entries, checkpointID: "checkpoint", direction: .upload), [])
        XCTAssertEqual(BackgroundQueuePlan.operationIDs(workOperationIDs: work, entries: [], checkpointID: "checkpoint", direction: .upload).count, BackgroundJournal.maximumScheduledPerCheckpoint)
    }

    func testQueuePlanRetriesTasklessScheduledEntryWithoutDuplicatingOperation() throws {
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .download)
        let entry = BackgroundJournal.Entry(descriptor: descriptor, taskIdentifier: nil, expectedResponseBytes: 1, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)
        XCTAssertEqual(BackgroundQueuePlan.operationIDs(workOperationIDs: ["op", "new"], entries: [entry], checkpointID: "checkpoint", direction: .download), ["op", "new"])
    }

    func testQueuePlanDoesNotReplayStagedOperation() throws {
        let descriptor = try BackgroundTaskDescriptor(operationID: "staged", checkpointID: "checkpoint", direction: .upload)
        let entry = BackgroundJournal.Entry(descriptor: descriptor, taskIdentifier: nil, expectedResponseBytes: 0, spoolName: nil, responseStatus: 201, responseHeaders: [:], state: .staged, networkDone: 0)
        XCTAssertEqual(BackgroundQueuePlan.operationIDs(workOperationIDs: ["staged", "next", "next"], entries: [entry], checkpointID: "checkpoint", direction: .upload), ["next"])
    }

    func testReconcileRemovesFailedAcknowledgedOperationAndIsReady() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .upload)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: nil, expectedResponseBytes: 0, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .failed, networkDone: 0)])
        _ = try journal.reconcilePending(checkpointID: "checkpoint", direction: .upload, pendingOperationIDs: [])
        XCTAssertTrue(journal.entries(checkpointID: "checkpoint", direction: .upload).isEmpty)
    }

    func testReconcilePreservesStillPendingOperationForRetry() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .upload)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: nil, expectedResponseBytes: 0, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .failed, networkDone: 0)])
        _ = try journal.reconcilePending(checkpointID: "checkpoint", direction: .upload, pendingOperationIDs: ["op"])
        XCTAssertEqual(journal.entry(operationID: "op")?.state, .failed)
        XCTAssertEqual(journal.queuePlan(workOperationIDs: ["op"], checkpointID: "checkpoint", direction: .upload), ["op"])
    }

    func testReconcilePreservesJournalWhenKeysAreUnavailable() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .download)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: nil, expectedResponseBytes: 1, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .failed, networkDone: 0)])
        _ = try journal.reconcilePending(checkpointID: "checkpoint", direction: .download, pendingOperationIDs: nil)
        XCTAssertNotNil(journal.entry(operationID: "op"))
    }

    func testReconcileKeepsTasklessScheduledOperationForReattempt() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .download)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: nil, expectedResponseBytes: 1, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)])
        _ = try journal.reconcilePending(checkpointID: "checkpoint", direction: .download, pendingOperationIDs: ["op"])
        XCTAssertEqual(journal.queuePlan(workOperationIDs: ["op"], checkpointID: "checkpoint", direction: .download), ["op"])
    }

    func testUploadResponseSpoolSurvivesUntilAcknowledgedRemoval() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .upload)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: 1, expectedResponseBytes: 0, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)])
        _ = try journal.appendUploadResponse(Data("ack".utf8), taskIdentifier: 1, response: nil, maximumBytes: 32)
        let staged = try XCTUnwrap(journal.stageUpload(taskIdentifier: 1))
        let spool = directory.appendingPathComponent("background-ciphertext").appendingPathComponent(try XCTUnwrap(staged.spoolName))
        XCTAssertTrue(FileManager.default.fileExists(atPath: spool.path))
        try journal.remove(operationID: "op")
        XCTAssertFalse(FileManager.default.fileExists(atPath: spool.path))
    }
}
