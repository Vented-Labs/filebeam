import XCTest

final class AppearanceCaptureUITests: XCTestCase {
    func testPrimaryFlowsCaptureLightDarkAndLargeText() {
        capture(name: "light", arguments: [])
        capture(name: "dark", arguments: ["-AppleInterfaceStyle", "Dark"])
        capture(name: "large-text", arguments: ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL"])
    }

    private func capture(name: String, arguments: [String]) {
        let app = XCUIApplication()
        app.launchArguments = arguments
        app.launch()
        XCTAssertTrue(app.tabBars.buttons["Transfers"].waitForExistence(timeout: 5))
        app.tabBars.buttons["Transfers"].tap()
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "transfers-\(name)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
