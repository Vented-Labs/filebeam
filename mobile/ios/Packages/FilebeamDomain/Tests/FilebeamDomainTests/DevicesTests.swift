import XCTest
@testable import FilebeamDomain

final class DevicesTests: XCTestCase {
    func testDirectoryKeepsMutableNamesSeparateFromIdentity() throws {
        let instance = try XCTUnwrap(FilebeamInstance(origin: "https://example.test"))
        let device = DeviceSnapshot(id: "device-1", name: "Office", platform: "macOS", trust: .verified, availability: .ready, canReceive: true)
        let directory = DeviceDirectory(instance: instance, generation: 2, revision: 5, currentDeviceID: "device-1", devices: [device], stale: false)
        XCTAssertEqual(directory.devices.first?.id, "device-1")
        XCTAssertEqual(directory.devices.first?.name, "Office")
    }
}
