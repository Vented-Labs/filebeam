import SwiftUI

/// The type scale. Styles map onto the system text styles so Dynamic Type scales every screen,
/// with weights and tracking that follow the web tokens.
enum FbTextStyle {
    /// Hero statements such as "Send your files privately".
    case display, headline, title, subtitle, body, bodyStrong, small, smallStrong, caption, label
    /// Small caps style section markers, e.g. "TURBO TRANSFER".
    case eyebrow, code, codeSmall
    /// Tabular figures so counters do not jitter as digits change.
    case numeric

    var font: Font {
        switch self {
        case .display: .system(.largeTitle, design: .default, weight: .bold)
        case .headline: .system(.title, design: .default, weight: .bold)
        case .title: .system(.title3, design: .default, weight: .semibold)
        case .subtitle: .system(.headline, design: .default, weight: .semibold)
        case .body: .system(.callout, design: .default, weight: .regular)
        case .bodyStrong: .system(.callout, design: .default, weight: .semibold)
        case .small: .system(.subheadline, design: .default, weight: .regular)
        case .smallStrong: .system(.subheadline, design: .default, weight: .semibold)
        case .caption: .system(.caption, design: .default, weight: .medium)
        case .label: .system(.subheadline, design: .default, weight: .semibold)
        case .eyebrow: .system(.caption2, design: .default, weight: .bold)
        case .code: .system(.subheadline, design: .monospaced, weight: .regular)
        case .codeSmall: .system(.caption, design: .monospaced, weight: .regular)
        case .numeric: .system(.subheadline, design: .default, weight: .medium).monospacedDigit()
        }
    }

    var tracking: CGFloat {
        switch self {
        case .display: -0.8
        case .headline: -0.5
        case .title: -0.3
        case .eyebrow: 1.4
        default: 0
        }
    }

    var isUppercased: Bool { self == .eyebrow }
}

struct FbTextModifier: ViewModifier {
    @Environment(\.theme) private var theme
    let style: FbTextStyle
    let color: Color?

    func body(content: Content) -> some View {
        content
            .font(style.font)
            .tracking(style.tracking)
            .foregroundStyle(color ?? theme.text)
            .textCase(style.isUppercased ? .uppercase : nil)
    }
}

extension View {
    /// Applies a Filebeam text style. Colour defaults to primary text.
    func fbText(_ style: FbTextStyle, color: Color? = nil) -> some View { modifier(FbTextModifier(style: style, color: color)) }
}
