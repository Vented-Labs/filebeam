import XCTest

final class PeerAcceptanceTests: XCTestCase {
    @MainActor func testHTTPSFixtureDownloadsAfterListingItsFiles() throws {
        guard let link = ProcessInfo.processInfo.environment["FILEBEAM_ACCEPTANCE_HTTPS_LINK_FIXTURE"], link.hasPrefix("https://") else {
            throw XCTSkip("A securely supplied HTTPS fixture is required.")
        }
        let app = XCUIApplication()
        app.launchArguments = ["-tour.seen", "YES"]
        app.launch()
        app.buttons["Receive"].tap()
        let field = app.descendants(matching: .any).matching(identifier: "receive-input").firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.typeWhenFocused(link)
        app.buttons["Open transfer"].tap()
        let download = app.buttons["Download"]
        let downloadAll = app.buttons["Download all"]
        XCTAssertTrue(download.waitForExistence(timeout: 60) || downloadAll.waitForExistence(timeout: 5), "The file list did not appear")
        (downloadAll.exists ? downloadAll : download).tap()
        XCTAssertTrue(app.staticTexts["Download completed"].waitForExistence(timeout: 180), "The fixture did not complete native decrypt-and-verify")
    }

    @MainActor func testReceiveDraftStillRetainsTypedInputAcrossTabs() {
        let app = XCUIApplication()
        app.launchArguments = ["-tour.seen", "YES"]
        app.launch()
        app.buttons["Receive"].tap()
        let field = app.descendants(matching: .any).matching(identifier: "receive-input").firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.typeWhenFocused("draft-retention-check")
        app.buttons["Transfers"].tap(); app.buttons["Receive"].tap()
        XCTAssertTrue(field.waitForExistence(timeout: 5), app.visibleSummary)
        XCTAssertEqual(field.value as? String, "draft-retention-check")
    }
}
