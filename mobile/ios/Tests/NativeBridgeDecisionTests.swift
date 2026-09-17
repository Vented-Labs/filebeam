import XCTest
import FilebeamCore

final class NativeBridgeDecisionTests: XCTestCase {
    private let key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
    private let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV"

    func testSharePresentationKeepsPortAndCanToggleKeyPlacement() throws {
        let origin = "https://filebeam.test:8443"
        let separate = try presentShareLink(instance: origin, shareUrl: "/\(id)", shareKey: "v1.\(key)", includeKey: false)
        XCTAssertEqual(separate.link, "\(origin)/\(id)")
        XCTAssertEqual(separate.separateKey, "v1.\(key)")

        let included = try presentShareLink(instance: origin, shareUrl: separate.link, shareKey: separate.separateKey, includeKey: true)
        XCTAssertEqual(included.link, "\(origin)/\(id)#k=v1.\(key)")
    }

    func testSplitAndPresentRoundTripsCanonicalKey() throws {
        let source = "https://filebeam.test:8443/\(id)#k=v1.\(key)"
        let split = try splitShareLink(instance: "https://filebeam.test:8443", link: source)
        let restored = try presentShareLink(instance: "https://filebeam.test:8443", shareUrl: split.link, shareKey: split.separateKey, includeKey: true)
        XCTAssertEqual(restored.link, source)
    }
}
