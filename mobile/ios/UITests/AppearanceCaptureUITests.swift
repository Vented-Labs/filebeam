import XCTest

final class AppearanceCaptureUITests: XCTestCase {
    @MainActor func testPrimaryFlowsCaptureLightDarkAndLargeText() {
        capture(name: "light", arguments: [])
        capture(name: "dark", arguments: ["-AppleInterfaceStyle", "Dark"])
        capture(name: "large-text", arguments: ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL"])
    }

    @MainActor private func capture(name: String, arguments: [String]) {
        let app = XCUIApplication()
        app.launchArguments = ["-tour.seen", "YES"] + arguments
        app.launch()
        XCTAssertTrue(app.buttons["Transfers"].waitForExistence(timeout: 5))
        app.buttons["Transfers"].tap()
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "transfers-\(name)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
