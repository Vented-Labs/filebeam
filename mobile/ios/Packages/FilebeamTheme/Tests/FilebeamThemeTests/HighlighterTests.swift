import Foundation
import XCTest
@testable import FilebeamTheme

final class HighlighterTests: XCTestCase {
    private func kinds(_ text: String, _ language: String) -> [(String, TokenKind)] {
        Highlighter.highlight(text, language: language).map { ((text as NSString).substring(with: $0.range), $0.kind) }
    }

    func testJavaScriptTokensFollowTheWebEditor() {
        let tokens = kinds("const total = items.length + 1; // sum\nfetch(url)", "javascript")
        XCTAssertEqual(tokens.first?.0, "const")
        XCTAssertEqual(tokens.first?.1, .keyword)
        XCTAssertTrue(tokens.contains { $0 == ("length", .property) })
        XCTAssertTrue(tokens.contains { $0 == ("// sum", .comment) })
        XCTAssertTrue(tokens.contains { $0 == ("fetch", .function) })
        XCTAssertTrue(tokens.contains { $0 == ("1", .number) })
    }

    func testJsonPropertiesAreDistinctFromStrings() {
        let tokens = kinds(#"{"name": "beam", "ok": true, "n": 2}"#, "json")
        XCTAssertTrue(tokens.contains { $0 == ("\"name\"", .property) })
        XCTAssertTrue(tokens.contains { $0 == ("\"beam\"", .string) })
        XCTAssertTrue(tokens.contains { $0 == ("true", .constant) })
    }

    func testMarkdownHeadingsAndLinks() {
        let tokens = kinds("# Title\nSee [docs](https://filebeam.io) and **bold**", "markdown")
        XCTAssertTrue(tokens.contains { $0 == ("# Title", .heading) })
        XCTAssertTrue(tokens.contains { $0 == ("[docs](https://filebeam.io)", .link) })
        XCTAssertTrue(tokens.contains { $0 == ("**bold**", .keyword) })
    }

    func testPlainAndOversizedInputProduceNoTokens() {
        XCTAssertTrue(Highlighter.highlight("let x = 1", language: "plain").isEmpty)
        let huge = String(repeating: "a", count: Highlighter.maximumHighlightCharacters + 1)
        XCTAssertTrue(Highlighter.highlight(huge, language: "javascript").isEmpty)
    }

    func testTokensCoverUtf16RangesOfNonAsciiText() {
        let text = "const café = \"naïve 🚀\"; // ok"
        let tokens = Highlighter.highlight(text, language: "javascript")
        let source = text as NSString
        for token in tokens { XCTAssertLessThanOrEqual(token.range.location + token.range.length, source.length) }
        XCTAssertTrue(kinds(text, "javascript").contains { $0 == ("\"naïve 🚀\"", .string) })
    }
}
