import XCTest

final class PeerAcceptanceTests: XCTestCase {
    func testHTTPSFixtureDownloadsThroughExplicitOriginConfirmation() throws {
        guard let link = ProcessInfo.processInfo.environment["FILEBEAM_ACCEPTANCE_HTTPS_LINK_FIXTURE"], link.hasPrefix("https://") else {
            throw XCTSkip("A securely supplied HTTPS fixture is required.")
        }
        let app = XCUIApplication()
        app.launch()
        app.tabBars.buttons["Receive"].tap()
        let field = app.textFields["Transfer link or ID"]
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.tap()
        field.typeText(link)
        app.buttons["Download and verify"].tap()
        XCTAssertTrue(app.staticTexts["Transfer from another instance"].waitForExistence(timeout: 20) || app.staticTexts["Ready to receive"].exists)
        app.buttons["Download and verify"].lastMatch.tap()
        XCTAssertTrue(app.staticTexts["Verified on this device"].waitForExistence(timeout: 180), "The fixture did not complete native decrypt-and-verify")
    }

    func testReceiveDraftStillRetainsTypedInputAcrossTabs() {
        let app = XCUIApplication()
        app.launch()
        app.tabBars.buttons["Receive"].tap()
        let field = app.textFields["Transfer link or ID"]
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.tap(); field.typeText("draft-retention-check")
        app.tabBars.buttons["Settings"].tap(); app.tabBars.buttons["Receive"].tap()
        XCTAssertEqual(field.value as? String, "draft-retention-check")
    }
}
