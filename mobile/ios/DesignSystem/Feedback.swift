import SwiftUI

enum Tone { case neutral, accent, success, warning, danger, info }

extension Theme {
    func colors(for tone: Tone) -> StatusColors {
        switch tone {
        case .neutral: StatusColors(ink: textMuted, surface: hover, border: border)
        case .accent: StatusColors(ink: accentText, surface: accentSurface, border: choiceBorder)
        case .success: success
        case .warning: warning
        case .danger: danger
        case .info: info
        }
    }
}

/// A small rounded status label.
struct StatusPill: View {
    @Environment(\.theme) private var theme
    let text: String
    var tone: Tone = .neutral
    var icon: Icon?

    init(_ text: String, tone: Tone = .neutral, icon: Icon? = nil) { self.text = text; self.tone = tone; self.icon = icon }

    var body: some View {
        let colors = theme.colors(for: tone)
        HStack(spacing: Space.xs) {
            if let icon { FbIcon(icon, size: 14, tint: colors.ink) }
            Text(text).fbText(.caption, color: colors.ink).lineLimit(1)
        }
        .padding(.horizontal, Space.sm)
        .padding(.vertical, 3)
        .background(colors.surface, in: Capsule())
        .overlay(Capsule().strokeBorder(colors.border.opacity(0.5), lineWidth: 1))
    }
}

/// An inline notice. Danger and warning notices are announced to assistive technology.
struct Notice<Action: View>: View {
    @Environment(\.theme) private var theme
    let text: String
    var tone: Tone = .info
    var icon: Icon?
    @ViewBuilder var action: () -> Action

    init(_ text: String, tone: Tone = .info, icon: Icon? = nil, @ViewBuilder action: @escaping () -> Action) {
        self.text = text; self.tone = tone; self.icon = icon; self.action = action
    }

    var body: some View {
        let colors = theme.colors(for: tone)
        // Failures from the transfer core are rewritten into plain sentences before they are shown.
        let shown = tone == .danger ? readableMessage(text) : text
        let fallback: Icon = switch tone { case .danger, .warning: .alert; case .success: .check; default: .info }
        HStack(spacing: Space.md) {
            FbIcon(icon ?? fallback, size: 20, tint: colors.ink)
            Text(shown).fbText(.small, color: colors.ink).fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity, alignment: .leading)
            action()
        }
        .padding(Space.md)
        .background(colors.surface, in: Shapes.control)
        .overlay(Shapes.control.strokeBorder(colors.border.opacity(0.55), lineWidth: 1))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(tone == .danger || tone == .warning ? .updatesFrequently : [])
    }
}

extension Notice where Action == EmptyView {
    init(_ text: String, tone: Tone = .info, icon: Icon? = nil) { self.init(text, tone: tone, icon: icon) { EmptyView() } }
}

/// A centred illustration, statement and optional action for empty and error states.
struct EmptyState<Action: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let icon: Icon
    let title: String
    var body_: String?
    var tone: Tone = .accent
    @ViewBuilder var action: () -> Action
    @State private var shown = false

    init(_ icon: Icon, _ title: String, body: String? = nil, tone: Tone = .accent, @ViewBuilder action: @escaping () -> Action) {
        self.icon = icon; self.title = title; self.body_ = body; self.tone = tone; self.action = action
    }

    var body: some View {
        let colors = theme.colors(for: tone)
        VStack(spacing: Space.md) {
            FbIcon(icon, size: 30, tint: colors.ink)
                .frame(width: 72, height: 72)
                .background(LinearGradient(colors: [colors.surface, theme.surface], startPoint: .topLeading, endPoint: .bottomTrailing), in: Shapes.panel)
                .overlay(Shapes.panel.strokeBorder(colors.border.opacity(0.5), lineWidth: 1))
            Text(title).fbText(.title).multilineTextAlignment(.center)
            if let body_ { Text(body_).fbText(.small, color: theme.textMuted).multilineTextAlignment(.center).frame(maxWidth: 320) }
            action().padding(.top, Space.sm)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, Space.xxl)
        .padding(.vertical, Space.xxxl)
        .opacity(shown || reduceMotion ? 1 : 0)
        .offset(y: shown || reduceMotion ? 0 : 16)
        .onAppear { withAnimation(reduceMotion ? nil : Motion.spatialSlow) { shown = true } }
    }
}

extension EmptyState where Action == EmptyView {
    init(_ icon: Icon, _ title: String, body: String? = nil, tone: Tone = .accent) { self.init(icon, title, body: body, tone: tone) { EmptyView() } }
}

struct ToastMessage: Identifiable, Equatable {
    let id = UUID()
    let text: String
    var tone: Tone = .neutral
}

/// Shows the latest message as a floating pill that rises in, lingers, and drops away.
struct ToastHost: View {
    @Environment(\.theme) private var theme
    let message: ToastMessage?
    let onDismissed: (ToastMessage) -> Void
    @State private var current: ToastMessage?
    @State private var visible = false

    var body: some View {
        ZStack {
            if visible, let shown = current {
                let colors = theme.colors(for: shown.tone)
                HStack(spacing: Space.md) {
                    if shown.tone != .neutral {
                        let icon: Icon = switch shown.tone { case .success: .check; case .danger, .warning: .alert; default: .info }
                        FbIcon(icon, size: 20, tint: colors.ink)
                    }
                    Text(shown.text).fbText(.smallStrong).lineLimit(3)
                }
                .padding(.horizontal, Space.lg)
                .padding(.vertical, Space.md)
                .background(theme.raised, in: Shapes.control)
                .overlay(Shapes.control.strokeBorder(theme.border, lineWidth: 1))
                .shadow(color: theme.shadow, radius: 16, y: 6)
                .padding(.horizontal, Space.gutter)
                .transition(.asymmetric(insertion: .move(edge: .bottom).combined(with: .opacity).combined(with: .scale(scale: 0.94)), removal: .move(edge: .bottom).combined(with: .opacity)))
                .accessibilityAddTraits(.updatesFrequently)
            }
        }
        .motion(Motion.spatial, value: visible)
        .task(id: message?.id) {
            guard let message else { return }
            current = message
            withMotion(Motion.spatial) { visible = true }
            try? await Task.sleep(for: .milliseconds(2600 + message.text.count * 28))
            guard !Task.isCancelled else { return }
            withMotion(Motion.exit) { visible = false }
            try? await Task.sleep(for: .milliseconds(200))
            onDismissed(message)
        }
    }
}
