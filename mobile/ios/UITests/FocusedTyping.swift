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
