import SwiftUI

/// The standard raised container: surface fill, hairline border, soft shadow.
struct Panel<Content: View>: View {
    @Environment(\.theme) private var theme
    var radius: CGFloat = Radius.panel
    var color: Color?
    var border: Color?
    var elevation: CGFloat = 0
    @ViewBuilder let content: () -> Content

    init(radius: CGFloat = Radius.panel, color: Color? = nil, border: Color? = nil, elevation: CGFloat = 0, @ViewBuilder content: @escaping () -> Content) {
        self.radius = radius; self.color = color; self.border = border; self.elevation = elevation; self.content = content
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        VStack(alignment: .leading, spacing: 0) { content() }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(color ?? theme.surface, in: shape)
            .overlay(shape.strokeBorder(border ?? theme.border, lineWidth: 1))
            .shadow(color: elevation > 0 ? theme.shadow : .clear, radius: elevation, y: elevation / 2)
    }
}

/// A one-pixel separator in the palette's soft line colour.
struct Hairline: View {
    @Environment(\.theme) private var theme
    var color: Color?
    var inset: CGFloat = 0
    var body: some View {
        Rectangle().fill(color ?? theme.border).frame(height: 1).padding(.horizontal, inset)
    }
}

/// A small uppercase label that introduces a group of rows.
struct SectionLabel: View {
    @Environment(\.theme) private var theme
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text).fbText(.eyebrow, color: theme.textMuted).padding(.top, Space.sm).accessibilityAddTraits(.isHeader)
    }
}

/// A screen heading with an optional supporting line.
struct ScreenHeader: View {
    @Environment(\.theme) private var theme
    let title: String
    var subtitle: String?
    init(_ title: String, subtitle: String? = nil) { self.title = title; self.subtitle = subtitle }
    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(title).fbText(.headline).accessibilityAddTraits(.isHeader)
            if let subtitle { Text(subtitle).fbText(.body, color: theme.textMuted) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Fades and lifts its content in once, `index` steps after its siblings.
struct Staggered: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let index: Int
    @State private var shown = false

    func body(content: Content) -> some View {
        content
            .opacity(shown || reduceMotion ? 1 : 0)
            .offset(y: shown || reduceMotion ? 0 : 16)
            .scaleEffect(shown || reduceMotion ? 1 : 0.985)
            .task {
                guard !shown else { return }
                try? await Task.sleep(for: .milliseconds(Int(Double(index) * Motion.stagger * 1000)))
                withAnimation(reduceMotion ? nil : Motion.enter) { shown = true }
            }
    }
}

extension View {
    func staggered(_ index: Int) -> some View { modifier(Staggered(index: index)) }
}

/// Shrinks slightly while pressed and springs back on release; the standard feel for tappable surfaces.
struct PressScale: ButtonStyle {
    var scale: CGFloat = 0.97
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? scale : 1)
            .motion(Motion.spatialFast, value: configuration.isPressed)
    }
}

extension ButtonStyle where Self == PressScale {
    static var pressScale: PressScale { PressScale() }
    static func pressScale(_ scale: CGFloat) -> PressScale { PressScale(scale: scale) }
}

/// An initial on the brand gradient, used wherever a person is shown.
struct Avatar: View {
    @Environment(\.theme) private var theme
    let name: String
    var size: CGFloat = 40
    var body: some View {
        Text(name.prefix(1).uppercased())
            .fbText(size > 48 ? .title : .label, color: .white)
            .frame(width: size, height: size)
            .background(theme.avatarGradient, in: Circle())
            .accessibilityHidden(true)
    }
}

/// Measures a view's height into a binding, for content-sized sheets.
struct HeightReader: ViewModifier {
    @Binding var height: CGFloat
    func body(content: Content) -> some View {
        content.background(GeometryReader { proxy in
            Color.clear.preference(key: HeightKey.self, value: proxy.size.height)
        })
        .onPreferenceChange(HeightKey.self) { value in
            Task { @MainActor in height = value }
        }
    }
}

struct HeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

extension View {
    func readHeight(_ height: Binding<CGFloat>) -> some View { modifier(HeightReader(height: height)) }
}
