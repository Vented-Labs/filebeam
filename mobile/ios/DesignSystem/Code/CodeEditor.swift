import SwiftUI
import UIKit
import FilebeamTheme

/// A code surface with a line gutter and syntax colouring.
/// With a binding it is an editor; with plain text, a selectable read-only viewer.
/// `wrap` switches between soft wrapping and horizontal scrolling.
struct CodeEditor: View {
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var dynamicType
    var text: Binding<String>?
    let value: String
    let language: String
    var wrap = true
    var placeholder: String?
    var minHeight: CGFloat = 180
    var maxHeight: CGFloat = 420
    @State private var contentHeight: CGFloat = 0

    init(text: Binding<String>, language: String, wrap: Bool = true, placeholder: String? = nil, minHeight: CGFloat = 180, maxHeight: CGFloat = 420) {
        self.text = text; value = text.wrappedValue; self.language = language; self.wrap = wrap; self.placeholder = placeholder; self.minHeight = minHeight; self.maxHeight = maxHeight
    }

    init(_ value: String, language: String, wrap: Bool = true, minHeight: CGFloat = 96, maxHeight: CGFloat = 420) {
        text = nil; self.value = value; self.language = language; self.wrap = wrap; self.minHeight = minHeight; self.maxHeight = maxHeight
    }

    var body: some View {
        CodeTextView(text: text, value: value, language: language, wrap: wrap, placeholder: placeholder, colors: theme.editor, contentHeight: $contentHeight)
            .frame(height: min(max(contentHeight + 24, minHeight), maxHeight))
            .background(theme.editor.background, in: Shapes.control)
            .overlay(Shapes.control.strokeBorder(theme.editor.border, lineWidth: 1))
            .clipShape(Shapes.control)
            .privacySensitive()
    }
}

private struct CodeTextView: UIViewRepresentable {
    var text: Binding<String>?
    let value: String
    let language: String
    let wrap: Bool
    let placeholder: String?
    let colors: EditorColors
    @Binding var contentHeight: CGFloat

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> CodeContainerView {
        let view = CodeContainerView()
        view.textView.delegate = context.coordinator
        view.textView.isEditable = text != nil
        context.coordinator.container = view
        return view
    }

    func updateUIView(_ view: CodeContainerView, context: Context) {
        context.coordinator.parent = self
        view.apply(colors: colors, wrap: wrap, placeholder: placeholder, editable: text != nil)
        if view.textView.text != value {
            let selection = view.textView.selectedRange
            view.textView.text = value
            if text != nil {
                let location = min(selection.location, (value as NSString).length)
                view.textView.selectedRange = NSRange(location: location, length: 0)
            }
        }
        context.coordinator.highlight(language: language)
        view.setNeedsLayout()
        context.coordinator.reportHeight()
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: CodeTextView
        weak var container: CodeContainerView?
        private var pending: Task<Void, Never>?
        private var lastHighlighted: (String, String)?
        private var tokens: [Token] = []

        init(_ parent: CodeTextView) { self.parent = parent }

        func textViewDidChange(_ textView: UITextView) {
            parent.text?.wrappedValue = textView.text
            highlight(language: parent.language)
            container?.setNeedsLayout()
            reportHeight()
        }

        func textViewDidChangeSelection(_ textView: UITextView) { container?.gutter.setNeedsDisplay() }
        func scrollViewDidScroll(_ scrollView: UIScrollView) { container?.gutter.setNeedsDisplay() }

        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            // A hardware Tab indents instead of moving focus out of the editor.
            if text == "\t" {
                textView.insertText("    ")
                return false
            }
            return true
        }

        func reportHeight() {
            guard let container else { return }
            let height = container.textView.sizeThatFits(CGSize(width: max(container.textView.bounds.width, 1), height: .greatestFiniteMagnitude)).height
            if abs(height - parent.contentHeight) > 0.5 {
                Task { @MainActor in self.parent.contentHeight = height }
            }
        }

        /// Computes tokens off the main thread for large documents, debounced so typing stays fluid.
        func highlight(language: String) {
            guard let container else { return }
            let text = container.textView.text ?? ""
            // Every SwiftUI update re-applies the base font and colour, so cached tokens are re-applied too.
            if lastHighlighted?.0 == text && lastHighlighted?.1 == language { apply(tokens: tokens); return }
            lastHighlighted = (text, language)
            pending?.cancel()
            if text.count <= 20_000 {
                apply(tokens: Highlighter.highlight(text, language: language))
            } else {
                apply(tokens: [])
                pending = Task.detached(priority: .userInitiated) { [weak self] in
                    try? await Task.sleep(for: .milliseconds(120))
                    let tokens = Highlighter.highlight(text, language: language)
                    guard !Task.isCancelled else { return }
                    await MainActor.run { self?.apply(tokens: tokens, for: text) }
                }
            }
        }

        private func apply(tokens: [Token], for source: String? = nil) {
            guard let container else { return }
            let storage = container.textView.textStorage
            if let source, container.textView.text != source { return }
            self.tokens = tokens
            let full = NSRange(location: 0, length: storage.length)
            let selection = container.textView.selectedRange
            storage.beginEditing()
            storage.setAttributes(container.baseAttributes, range: full)
            for token in tokens where NSMaxRange(token.range) <= storage.length {
                storage.addAttribute(.foregroundColor, value: UIColor(parent.colors.color(for: token.kind)), range: token.range)
                if token.kind == .comment { storage.addAttribute(.font, value: container.italicFont, range: token.range) }
                if token.kind == .heading { storage.addAttribute(.font, value: container.boldFont, range: token.range) }
            }
            storage.endEditing()
            if container.textView.selectedRange != selection { container.textView.selectedRange = selection }
            container.textView.typingAttributes = container.baseAttributes
            container.gutter.setNeedsDisplay()
        }
    }
}

/// UIKit host: a gutter that draws logical line numbers beside a text view.
final class CodeContainerView: UIView {
    let textView = UITextView()
    let gutter = GutterView()
    private let placeholderLabel = UILabel()
    private(set) var baseAttributes: [NSAttributedString.Key: Any] = [:]
    private(set) var italicFont = UIFont.monospacedSystemFont(ofSize: 15, weight: .regular)
    private(set) var boldFont = UIFont.monospacedSystemFont(ofSize: 15, weight: .bold)
    private static let gutterWidth: CGFloat = 44

    override init(frame: CGRect) {
        super.init(frame: frame)
        textView.backgroundColor = .clear
        textView.autocapitalizationType = .none
        textView.autocorrectionType = .no
        textView.smartQuotesType = .no
        textView.smartDashesType = .no
        textView.spellCheckingType = .no
        textView.keyboardType = .asciiCapable
        textView.textContainerInset = UIEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        textView.alwaysBounceVertical = false
        gutter.textView = textView
        addSubview(gutter)
        addSubview(textView)
        placeholderLabel.numberOfLines = 0
        textView.addSubview(placeholderLabel)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func apply(colors: EditorColors, wrap: Bool, placeholder: String?, editable: Bool) {
        let size = UIFontMetrics(forTextStyle: .subheadline).scaledValue(for: 14)
        let font = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        italicFont = UIFont(descriptor: font.fontDescriptor.withSymbolicTraits(.traitItalic) ?? font.fontDescriptor, size: size)
        boldFont = UIFont.monospacedSystemFont(ofSize: size, weight: .bold)
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineHeightMultiple = 1.3
        baseAttributes = [.font: font, .foregroundColor: UIColor(colors.text), .paragraphStyle: paragraph]
        textView.typingAttributes = baseAttributes
        if textView.font != font { textView.font = font }
        if textView.textColor != UIColor(colors.text) { textView.textColor = UIColor(colors.text) }
        textView.tintColor = UIColor(colors.caret)
        textView.isEditable = editable
        textView.isSelectable = true
        textView.textContainer.widthTracksTextView = wrap
        textView.textContainer.lineBreakMode = wrap ? .byWordWrapping : .byClipping
        if !wrap { textView.textContainer.size = CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude) }
        gutter.colors = colors
        gutter.font = UIFont.monospacedSystemFont(ofSize: size * 0.86, weight: .regular)
        gutter.backgroundColor = UIColor(colors.gutterBackground)
        placeholderLabel.text = placeholder
        placeholderLabel.font = font
        placeholderLabel.textColor = UIColor(colors.comment)
        placeholderLabel.isHidden = placeholder == nil || !(textView.text ?? "").isEmpty
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        gutter.frame = CGRect(x: 0, y: 0, width: Self.gutterWidth, height: bounds.height)
        textView.frame = CGRect(x: Self.gutterWidth, y: 0, width: bounds.width - Self.gutterWidth, height: bounds.height)
        placeholderLabel.frame = CGRect(x: 17, y: 12, width: textView.bounds.width - 34, height: 0)
        placeholderLabel.sizeToFit()
        placeholderLabel.isHidden = placeholderLabel.text == nil || !(textView.text ?? "").isEmpty
        gutter.setNeedsDisplay()
    }
}

/// Draws line numbers at the top of each logical line, so wrapped lines keep their number.
final class GutterView: UIView {
    weak var textView: UITextView?
    var colors: EditorColors?
    var font = UIFont.monospacedSystemFont(ofSize: 12, weight: .regular)

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        isUserInteractionEnabled = false
        contentMode = .redraw
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func draw(_ rect: CGRect) {
        guard let textView, let colors, let context = UIGraphicsGetCurrentContext() else { return }
        context.setFillColor(UIColor(colors.gutterBackground).cgColor)
        context.fill(bounds)
        context.setFillColor(UIColor(colors.border).cgColor)
        context.fill(CGRect(x: bounds.width - 1, y: 0, width: 1, height: bounds.height))
        let layout = textView.layoutManager
        let storage = textView.textStorage
        let text = storage.string as NSString
        let active = textView.isEditable ? text.substring(to: min(textView.selectedRange.location, text.length)).components(separatedBy: "\n").count - 1 : -1
        let inset = textView.textContainerInset.top - textView.contentOffset.y
        var logical = 0
        var offset = 0
        let right = bounds.width - 12
        while true {
            let glyph = layout.glyphIndexForCharacter(at: min(offset, max(text.length, 0)))
            let fragment: CGRect
            if text.length == 0 || offset >= text.length {
                fragment = CGRect(x: 0, y: layout.usedRect(for: textView.textContainer).maxY - (offset >= text.length && offset > 0 ? font.lineHeight * 1.3 : 0), width: 0, height: font.lineHeight * 1.3)
            } else {
                fragment = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            }
            let y = fragment.minY + inset
            if y > bounds.height { break }
            if y + fragment.height >= 0 {
                let isActive = logical == active
                let number = "\(logical + 1)" as NSString
                let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: UIColor(isActive ? colors.gutterActiveText : colors.gutterText)]
                let size = number.size(withAttributes: attributes)
                number.draw(at: CGPoint(x: right - size.width, y: y + (fragment.height - size.height) / 2), withAttributes: attributes)
            }
            let next = text.range(of: "\n", options: [], range: NSRange(location: offset, length: text.length - offset))
            if next.location == NSNotFound { break }
            offset = next.location + 1
            logical += 1
            if offset > text.length { break }
        }
    }
}
