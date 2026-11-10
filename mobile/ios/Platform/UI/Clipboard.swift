import UIKit
import UniformTypeIdentifiers

/// Copies text flagged as local-only and short-lived, so keys and links do not linger in
/// Universal Clipboard or the system's clipboard history.
@MainActor
enum Clipboard {
    static func copySensitive(_ text: String) {
        UIPasteboard.general.setItems(
            [[UTType.utf8PlainText.identifier: text]],
            options: [.localOnly: true, .expirationDate: Date().addingTimeInterval(300)]
        )
    }

    static func copy(_ text: String) { UIPasteboard.general.string = text }

    /// The clipboard's text, read only when the person asked to paste.
    static func paste() -> String? { UIPasteboard.general.string }
}
