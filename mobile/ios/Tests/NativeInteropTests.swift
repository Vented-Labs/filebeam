import XCTest
import FilebeamCore

final class NativeInteropTests: XCTestCase {
    func testAppleABIRuntimeConstructsAndRejectsHTTPByDefault() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = ClientConfig(stateDirectory: root.path, memoryBudgetMib: 8, maxConcurrency: 1, relayOnly: false, allowHttp: false)
        let runtime = try NativeRuntime(config: config)
        _ = try TransferClient.newWithRuntime(config: config, runtime: runtime)
    }

    func testNativeIntegrationAgainstConfiguredHTTPSInstance() throws {
        guard let instance = ProcessInfo.processInfo.environment["FILEBEAM_ACCEPTANCE_INSTANCE"], instance.hasPrefix("https://") else {
            throw XCTSkip("Set FILEBEAM_ACCEPTANCE_INSTANCE to run the opt-in HTTPS FFI integration check.")
        }
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = ClientConfig(stateDirectory: root.path, memoryBudgetMib: 8, maxConcurrency: 1, relayOnly: false, allowHttp: false)
        let runtime = try NativeRuntime(config: config)
        let client = try TransferClient.newWithRuntime(config: config, runtime: runtime)
        XCTAssertFalse(try client.discover(instance: instance).enabledTransports.isEmpty)
    }
}
