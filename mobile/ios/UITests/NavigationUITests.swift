import XCTest

final class NavigationUITests: XCTestCase {
    func testFivePrimaryTabsAreReachable() {
        let app = XCUIApplication()
        app.launch()
        for tab in ["Send", "Receive", "Transfers", "Inbox", "Settings"] {
            XCTAssertTrue(app.tabBars.buttons[tab].waitForExistence(timeout: 5), "Missing \(tab) tab")
            app.tabBars.buttons[tab].tap()
        }
    }

    func testReceiveDraftSurvivesTabSwitch() {
        let app = XCUIApplication()
        app.launch()
        app.tabBars.buttons["Receive"].tap()
        let field = app.textFields["Transfer link or ID"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText("example-transfer-id")
        app.tabBars.buttons["Settings"].tap()
        app.tabBars.buttons["Receive"].tap()
        XCTAssertEqual(field.value as? String, "example-transfer-id")
    }
}
