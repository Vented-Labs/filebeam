import XCTest

extension XCUIElement {
    /// Types once the element owns the keyboard; a tap during a tab transition can miss focus.
    @MainActor func typeWhenFocused(_ text: String) {
        for _ in 0..<3 {
            tap()
            let focused = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hasKeyboardFocus == true"), object: self)
            if XCTWaiter().wait(for: [focused], timeout: 2) == .completed { break }
        }
        typeText(text)
    }
}

extension XCUIApplication {
    /// The visible buttons and headings, for assertion messages that explain which screen was showing.
    @MainActor var visibleSummary: String {
        let buttons = buttons.allElementsBoundByIndex.prefix(12).map(\.label).filter { !$0.isEmpty }
        let texts = staticTexts.allElementsBoundByIndex.prefix(6).map(\.label).filter { !$0.isEmpty }
        return "buttons: \(buttons.joined(separator: " | ")); texts: \(texts.joined(separator: " | "))"
    }
}
