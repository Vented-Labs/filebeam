import Foundation
import XCTest
@testable import Filebeam

final class BackgroundJournalTests: XCTestCase {
    func testJournalPersistsVersionedOpaqueTaskDescriptor() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "opaque-operation", checkpointID: "checkpoint", direction: .download)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: 1, expectedResponseBytes: 32, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)])
        XCTAssertEqual(BackgroundTaskDescriptor.parse(descriptor.taskDescription), descriptor)
        XCTAssertFalse(String(decoding: try Data(contentsOf: directory.appendingPathComponent("background-http-v1.json")), as: UTF8.self).contains("cookie"))
    }

    func testPauseIsDurableBeforeCancellation() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let journal = try BackgroundJournal(directory: directory)
        let descriptor = try BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .upload)
        try journal.addBatch([.init(descriptor: descriptor, taskIdentifier: 1, expectedResponseBytes: 0, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)])
        XCTAssertEqual(try journal.markPaused(checkpointID: "checkpoint"), .upload)
        XCTAssertEqual(journal.entry(operationID: "op")?.state, .paused)
    }

    func testURLSessionTaskDescriptionSurvivesTaskEnumeration() async {
        let configuration = URLSessionConfiguration.background(withIdentifier: "io.filebeam.tests.background.\(UUID().uuidString)")
        let session = URLSession(configuration: configuration)
        let task = session.downloadTask(with: URL(string: "https://example.invalid/ciphertext")!)
        let descriptor = try! BackgroundTaskDescriptor(operationID: "op", checkpointID: "checkpoint", direction: .download)
        task.taskDescription = descriptor.taskDescription
        let tasks = await session.allTasks
        XCTAssertEqual(BackgroundTaskDescriptor.parse(tasks.first?.taskDescription), descriptor)
        task.cancel(); session.invalidateAndCancel()
    }
}
