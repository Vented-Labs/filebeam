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
        XCTAssertEqual(policy.branding.name, "Filebeam")
        XCTAssertNil(policy.branding.primaryColor)
    }

    func testDecodesBrandingAndInstanceInformation() throws {
        let policy = try NativePolicyDecoder.decode(branded, instance: instance)
        XCTAssertEqual(policy.branding.name, "Acme Beam")
        XCTAssertEqual(policy.branding.primaryColor, 0x3d78d8)
        XCTAssertEqual(policy.branding.version, "1.4.0")
        XCTAssertTrue(policy.branding.changelogEnabled)
        XCTAssertTrue(policy.branding.deviceRendezvousEnabled)
        XCTAssertEqual(policy.branding.copyright, "2026 Acme")
        XCTAssertEqual(policy.branding.githubURL, "https://github.com/acme/beam")
        XCTAssertEqual(policy.branding.communityLinks.map(\.name), ["Discord", "Forum"])
        XCTAssertEqual(policy.fileRetentionChoices(), [6, 24])
    }

    func testRejectsUnsupportedDriversAndInvalidEngineBounds() {
        XCTAssertThrowsError(try NativePolicyDecoder.decode(fixture.replacingOccurrences(of: "\"http\", \"webrtc\"", with: "\"ftp\", \"webrtc\""), instance: instance))
        XCTAssertThrowsError(try NativePolicyDecoder.decode(fixture.replacingOccurrences(of: "\"chunkBytes\":1024", with: "\"chunkBytes\":25000000"), instance: instance))
        XCTAssertThrowsError(try NativePolicyDecoder.decode(fixture.replacingOccurrences(of: "\"maximum_file_count\":0", with: "\"maximum_file_count\":65536"), instance: instance))
    }

    private let fixture = """
    {"account":{"registrationEnabled":true,"usernameRoutingEnabled":false},"transfer":{"anonymousUploadsEnabled":false,"defaultDriver":"http","enabledDrivers":["http", "webrtc"],"chunkBytes":1024,"limits":{"http":{"maximum_transfer_bytes":null,"maximum_file_count":0,"maximum_note_bytes":4096},"webrtc":{"maximum_file_count":2,"maximum_note_bytes":null}},"retention":{"files":{"defaultHours":24,"options":[6,24]},"notes":{"defaultHours":72,"options":[1,72]}}}}
    """

    private let branded = """
    {"instance":{"name":"Acme Beam","url":"https://example.test","version":"1.4.0","changelogEnabled":true},"branding":{"primaryColor":"#3d78d8","copyright":"2026 Acme","githubUrl":"https://github.com/acme/beam","communityLinks":[{"type":"discord","url":"https://discord.gg/x"},{"name":"forum","url":"https://forum.example"}]},"account":{"registrationEnabled":true,"usernameRoutingEnabled":true,"deviceRendezvousEnabled":true},"transfer":{"anonymousUploadsEnabled":true,"defaultDriver":"http","enabledDrivers":["http", "webrtc"],"chunkBytes":1024,"limits":{"http":{"maximum_transfer_bytes":null,"maximum_file_count":0,"maximum_note_bytes":4096},"webrtc":{"maximum_file_count":2,"maximum_note_bytes":null}},"retention":{"files":{"defaultHours":24,"options":[6,24]},"notes":{"defaultHours":72,"options":[1,72]}}}}
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
