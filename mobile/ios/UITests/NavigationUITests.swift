import XCTest

final class NavigationUITests: XCTestCase {
    @MainActor func testFourPrimaryDestinationsAreReachable() {
        let app = XCUIApplication()
        app.launchArguments = ["-tour.seen", "YES"]
        app.launch()
        for tab in ["Send", "Receive", "Transfers", "Inbox"] {
            let button = app.buttons[tab]
            XCTAssertTrue(button.waitForExistence(timeout: 5), "Missing \(tab) destination")
            button.tap()
        }
    }

    @MainActor func testReceiveDraftSurvivesTabSwitch() {
        let app = XCUIApplication()
        app.launchArguments = ["-tour.seen", "YES"]
        app.launch()
        app.buttons["Receive"].tap()
        let field = app.descendants(matching: .any).matching(identifier: "receive-input").firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeWhenFocused("example-transfer-id")
        app.buttons["Transfers"].tap()
        app.buttons["Receive"].tap()
        XCTAssertTrue(field.waitForExistence(timeout: 5), app.visibleSummary)
        XCTAssertEqual(field.value as? String, "example-transfer-id")
    }
}
