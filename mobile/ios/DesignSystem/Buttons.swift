import SwiftUI

enum FbButtonKind { case primary, secondary, quiet, danger }

/// The app's button. Primary fills with the action gradient; Secondary is outlined;
/// Quiet is text-only; Danger is a quiet destructive action.
/// While `loading` the label gives way to a spinner and presses are ignored.
struct FbButton: View {
    @Environment(\.theme) private var theme
    let title: String
    var kind: FbButtonKind = .primary
    var icon: Icon?
    var trailingIcon: Icon?
    var enabled = true
    var loading = false
    var height: CGFloat = Space.control
    var fill = true
    let action: () -> Void

    init(_ title: String, kind: FbButtonKind = .primary, icon: Icon? = nil, trailingIcon: Icon? = nil, enabled: Bool = true, loading: Bool = false, height: CGFloat = Space.control, fill: Bool = true, action: @escaping () -> Void) {
        self.title = title; self.kind = kind; self.icon = icon; self.trailingIcon = trailingIcon; self.enabled = enabled; self.loading = loading; self.height = height; self.fill = fill; self.action = action
    }

    var body: some View {
        Button {
            Haptics.tap()
            action()
        } label: {
            ZStack {
                HStack(spacing: Space.sm) {
                    if let icon { FbIcon(icon, size: 20, tint: content) }
                    Text(title).fbText(.label, color: content).lineLimit(1).minimumScaleFactor(0.85)
                    if let trailingIcon { FbIcon(trailingIcon, size: 18, tint: content) }
                }
                .opacity(loading ? 0 : 1)
                .scaleEffect(loading ? 0.8 : 1)
                if loading { Spinner(color: content, size: 20) }
            }
            .motion(Motion.fade, value: loading)
            .frame(maxWidth: fill ? .infinity : nil, minHeight: height)
            .padding(.horizontal, Space.xl)
            .contentShape(Shapes.control)
        }
        .buttonStyle(FbButtonStyle(kind: kind, enabled: enabled, loading: loading))
        .disabled(!enabled || loading)
        .accessibilityHint(loading ? "Working" : "")
    }

    private var content: Color {
        switch (enabled, kind) {
        case (false, _): theme.disabledText
        case (_, .primary): theme.onAction
        case (_, .danger): theme.danger.ink
        case (_, .quiet): theme.accentText
        case (_, .secondary): theme.text
        }
    }
}

private struct FbButtonStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    let kind: FbButtonKind
    let enabled: Bool
    let loading: Bool

    func makeBody(configuration: Configuration) -> some View {
        let pressed = configuration.isPressed
        configuration.label
            .background(background(pressed: pressed), in: Shapes.control)
            .overlay(Shapes.control.strokeBorder(border(pressed: pressed), lineWidth: 1))
            .overlay(alignment: .top) {
                // A faint top highlight lifts the primary fill in dark themes.
                if kind == .primary && enabled {
                    Rectangle().fill(theme.highlight).frame(height: 1).padding(.horizontal, 18).padding(.top, 1)
                }
            }
            .clipShape(Shapes.control)
            .shadow(color: kind == .primary && enabled ? theme.action.opacity(0.35) : .clear, radius: 10, y: 4)
            .scaleEffect(pressed ? 0.97 : 1)
            .motion(Motion.spatialFast, value: pressed)
    }

    private func background(pressed: Bool) -> AnyShapeStyle {
        switch kind {
        case .primary where !enabled: AnyShapeStyle(theme.disabledSurface)
        case .primary: pressed ? AnyShapeStyle(theme.actionPressed) : AnyShapeStyle(theme.actionGradient)
        case .secondary: AnyShapeStyle(pressed ? theme.hover : theme.surface)
        case .quiet, .danger: AnyShapeStyle(pressed ? theme.hover : Color.clear)
        }
    }

    private func border(pressed: Bool) -> Color {
        switch kind {
        case _ where !enabled: theme.disabledBorder
        case .primary: theme.actionBorder
        case .secondary: pressed ? theme.controlBorderHover : theme.controlBorder
        default: .clear
        }
    }
}

/// A round, touch-sized icon button with an accessible label.
struct IconAction: View {
    @Environment(\.theme) private var theme
    let icon: Icon
    let label: String
    var tint: Color?
    var enabled = true
    var bordered = false
    var size: CGFloat = 44
    let action: () -> Void

    init(_ icon: Icon, _ label: String, tint: Color? = nil, enabled: Bool = true, bordered: Bool = false, size: CGFloat = 44, action: @escaping () -> Void) {
        self.icon = icon; self.label = label; self.tint = tint; self.enabled = enabled; self.bordered = bordered; self.size = size; self.action = action
    }

    var body: some View {
        Button(action: action) {
            FbIcon(icon, size: 22, tint: enabled ? (tint ?? theme.textMuted) : theme.disabledText)
                .frame(width: size, height: size)
                .background(bordered ? theme.surface : Color.clear, in: Circle())
                .overlay(Circle().strokeBorder(bordered ? theme.border : .clear, lineWidth: 1))
                .contentShape(Circle())
        }
        .buttonStyle(IconActionStyle())
        .disabled(!enabled)
        .accessibilityLabel(label)
    }
}

private struct IconActionStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? theme.hover : Color.clear, in: Circle())
            .scaleEffect(configuration.isPressed ? 0.9 : 1)
            .motion(Motion.spatialFast, value: configuration.isPressed)
    }
}

/// An indeterminate ring. Holds still when motion is reduced.
struct Spinner: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var color: Color?
    var size: CGFloat = 22
    var stroke: CGFloat = 2
    @State private var spinning = false

    var body: some View {
        let ink = color ?? theme.accentText
        ZStack {
            Circle().stroke(ink.opacity(0.22), lineWidth: stroke)
            Circle().trim(from: 0, to: 0.28).stroke(ink, style: StrokeStyle(lineWidth: stroke, lineCap: .round))
                .rotationEffect(.degrees(spinning ? 360 : 0))
        }
        .frame(width: size, height: size)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(.linear(duration: 0.8).repeatForever(autoreverses: false)) { spinning = true }
        }
        .accessibilityLabel("Loading")
    }
}
