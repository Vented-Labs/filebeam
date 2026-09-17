import XCTest
@testable import Filebeam
import FilebeamDomain

final class StorageKeychainStoreTests: XCTestCase {
    func testDocumentImportStreamsSnapshotWithoutRetainingProviderURL() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let source = root.appendingPathComponent("input.bin")
        try Data(repeating: 0x5A, count: 256 * 1024).write(to: source)
        let imported = await DocumentStore(root: root.appendingPathComponent("snapshots")).importSources([source])
        XCTAssertEqual(imported.count, 1)
        XCTAssertEqual(imported[0].sizeBytes, 256 * 1024)
        XCTAssertNotEqual(imported[0].location, source.path)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: imported[0].location)).count, 256 * 1024)
    }

    func testDocumentImportUsesCollisionSafeAppOwnedNames() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let first = root.appendingPathComponent("first/report.txt"); let second = root.appendingPathComponent("second/report.txt")
        try FileManager.default.createDirectory(at: first.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: second.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("one".utf8).write(to: first); try Data("two".utf8).write(to: second)
        let sources = await DocumentStore(root: root.appendingPathComponent("snapshots")).importSources([first, second])
        XCTAssertEqual(Set(sources.map(\.location)).count, 2)
        XCTAssertTrue(sources.allSatisfy { $0.state == .ready })
    }

    func testDocumentImportPreservesUnicodeDotfilesAndZeroByteFiles() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let folder = root.appendingPathComponent("资料", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data().write(to: folder.appendingPathComponent(".env"))
        try Data("contents".utf8).write(to: folder.appendingPathComponent("résumé.txt"))
        let imported = await DocumentStore(root: root.appendingPathComponent("snapshots")).importSources([folder])
        XCTAssertEqual(Set(imported.map(\.name)), [".env", "résumé.txt"])
        XCTAssertEqual(imported.first(where: { $0.name == ".env" })?.sizeBytes, 0)
    }

    func testDraftsAreEncryptedAndRoundTrip() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = DraftStore(directory: directory, keychain: IOSKeychainStore(namespace: "tests.\(UUID().uuidString)", stateDirectory: directory))
        let drafts = ComposerDrafts(note: NoteDraft(text: "sensitive note"))
        try await store.save(drafts)
        XCTAssertEqual(try await store.load(), drafts)
        let ciphertext = try Data(contentsOf: directory.appendingPathComponent("composer-drafts-v1.bin"))
        XCTAssertFalse(String(decoding: ciphertext, as: UTF8.self).contains("sensitive note"))
    }

    func testKeychainIsDeviceOnlyAndCanonicalizesSessionOrigin() throws {
        let keychain = IOSKeychainStore(namespace: "tests.\(UUID().uuidString)", stateDirectory: FileManager.default.temporaryDirectory)
        XCTAssertEqual(try keychain.checkpointKey(scope: "checkpoint-catalog-v1").count, 32)
        try keychain.saveSession(origin: "https://FILEBEAM.IO/", cookie: "cookie")
        XCTAssertEqual(try keychain.loadSession(origin: "https://filebeam.io/"), "cookie")
    }

    func testCheckpointKeyAllowsEmptyJobDirectoriesButRejectsExistingCheckpointWithoutKey() throws {
        let state = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: state) }
        let emptyJob = state.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: emptyJob, withIntermediateDirectories: true)
        XCTAssertEqual(try IOSKeychainStore(namespace: "tests.\(UUID().uuidString)", stateDirectory: state).checkpointKey(scope: "checkpoint-catalog-v1").count, 32)
        let checkpointJob = state.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: checkpointJob, withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: checkpointJob.appendingPathComponent("checkpoint.json"))
        XCTAssertThrowsError(try IOSKeychainStore(namespace: "tests.\(UUID().uuidString)", stateDirectory: state).checkpointKey(scope: "checkpoint-catalog-v1"))
    }

    func testCheckpointKeyCreationIsStableUnderConcurrentCalls() async throws {
        let keychain = IOSKeychainStore(namespace: "tests.\(UUID().uuidString)", stateDirectory: FileManager.default.temporaryDirectory)
        let keys = await withTaskGroup(of: Data?.self, returning: [Data].self) { group in
            for _ in 0..<100 { group.addTask { try? keychain.checkpointKey(scope: "checkpoint-catalog-v1") } }
            return await group.reduce(into: []) { if let key = $1 { $0.append(key) } }
        }
        XCTAssertEqual(keys.count, 100)
        XCTAssertEqual(Set(keys).count, 1)
    }

    func testSessionRejectsCredentialsAndNonOriginPaths() throws {
        let keychain = IOSKeychainStore(namespace: "tests.\(UUID().uuidString)", stateDirectory: FileManager.default.temporaryDirectory)
        XCTAssertThrowsError(try keychain.saveSession(origin: "https://user:pass@filebeam.io", cookie: "cookie"))
        XCTAssertThrowsError(try keychain.saveSession(origin: "https://filebeam.io/account", cookie: "cookie"))
    }
}
