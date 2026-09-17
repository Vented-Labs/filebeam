import XCTest
import FilebeamDomain

#if os(Linux)
@testable import NativePolicy
#else
@testable import Filebeam
#endif

final class NativePolicyTests: XCTestCase {
    private let instance = FilebeamInstance(origin: "https://example.test")!

    func testDecodesAuthenticatedPolicyAndPreservesLimitStates() throws {
        let policy = try NativePolicyDecoder.decode(fixture, instance: instance)

        XCTAssertFalse(policy.anonymousUploads)
        XCTAssertEqual(policy.registrationEnabled, true)
        XCTAssertEqual(policy.usernameRoutingEnabled, false)
        XCTAssertEqual(policy.retentionOptionsHours, [6, 24])
        XCTAssertEqual(policy.noteRetentionOptionsHours, [1, 72])
        XCTAssertEqual(policy.fileDefaultRetentionHours, 24)
        XCTAssertEqual(policy.noteDefaultRetentionHours, 72)
        XCTAssertEqual(policy.drivers["http"]?.maximumTransferBytes, .unlimited)
        XCTAssertEqual(policy.drivers["http"]?.maximumFileCount, .finite(0))
        XCTAssertEqual(policy.drivers["http"]?.maximumNoteBytes, .finite(4096))
        XCTAssertEqual(policy.drivers["webrtc"]?.maximumTransferBytes, .unknown)
    }

    func testRejectsUnsupportedDriversAndInvalidEngineBounds() {
        XCTAssertThrowsError(try NativePolicyDecoder.decode(fixture.replacingOccurrences(of: "\"http\", \"webrtc\"", with: "\"ftp\", \"webrtc\""), instance: instance))
        XCTAssertThrowsError(try NativePolicyDecoder.decode(fixture.replacingOccurrences(of: "\"chunkBytes\":1024", with: "\"chunkBytes\":25000000"), instance: instance))
        XCTAssertThrowsError(try NativePolicyDecoder.decode(fixture.replacingOccurrences(of: "\"maximum_file_count\":0", with: "\"maximum_file_count\":65536"), instance: instance))
    }

    private let fixture = """
    {"account":{"registrationEnabled":true,"usernameRoutingEnabled":false},"transfer":{"anonymousUploadsEnabled":false,"defaultDriver":"http","enabledDrivers":["http", "webrtc"],"chunkBytes":1024,"limits":{"http":{"maximum_transfer_bytes":null,"maximum_file_count":0,"maximum_note_bytes":4096},"webrtc":{"maximum_file_count":2,"maximum_note_bytes":null}},"retention":{"files":{"defaultHours":24,"options":[6,24]},"notes":{"defaultHours":72,"options":[1,72]}}}}
    """
}

#if os(Linux)
extension NativePolicyTests {
    static let allTests = [
        ("testDecodesAuthenticatedPolicyAndPreservesLimitStates", testDecodesAuthenticatedPolicyAndPreservesLimitStates),
        ("testRejectsUnsupportedDriversAndInvalidEngineBounds", testRejectsUnsupportedDriversAndInvalidEngineBounds),
    ]
}

@main
enum NativePolicyTestRunner {
    static func main() { XCTMain([testCase(NativePolicyTests.allTests)]) }
}
#endif
