import SwiftUI

/// A bottom sheet in the Filebeam surface style with an optional title row.
/// Content scrolls when it outgrows the sheet and stays above the keyboard.
struct FbSheet<Content: View>: View {
    @Environment(\.theme) private var theme
    var title: String?
    var subtitle: String?
    var fitted = true
    @ViewBuilder let content: () -> Content
    @State private var height: CGFloat = 0

    init(title: String? = nil, subtitle: String? = nil, fitted: Bool = true, @ViewBuilder content: @escaping () -> Content) {
        self.title = title; self.subtitle = subtitle; self.fitted = fitted; self.content = content
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.lg) {
                if let title {
                    VStack(alignment: .leading, spacing: Space.xs) {
                        Text(title).fbText(.title).accessibilityAddTraits(.isHeader)
                        if let subtitle { Text(subtitle).fbText(.small, color: theme.textMuted) }
                    }
                }
                content()
            }
            .frame(maxWidth: contentMaxWidth)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Space.gutter)
            .padding(.top, Space.xl)
            .padding(.bottom, Space.xxl)
            .readHeight($height)
        }
        .scrollBounceBehavior(.basedOnSize)
        .scrollDismissesKeyboard(.interactively)
        .background(theme.surface)
        .presentationDetents(fitted && height > 0 ? [.height(min(height + 24, UIScreen.main.bounds.height * 0.88)), .large] : [.large])
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(Radius.sheet)
        .presentationBackground(theme.surface)
        .tint(theme.accentText)
    }
}

/// A centred confirmation dialog with a statement, optional body and up to two actions.
struct FbDialogSpec: Identifiable {
    let id = UUID()
    var title: String
    var body: String?
    var confirmLabel: String
    var dismissLabel: String? = "Cancel"
    var destructive = false
    var content: AnyView?
    var onConfirm: () -> Void
}

/// Presents a dialog above the whole app. Screens call `AppModel.dialog(...)`; the root shows it.
struct FbDialogHost: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let spec: FbDialogSpec?
    let dismiss: () -> Void

    var body: some View {
        ZStack {
            if let spec {
                theme.scrim.ignoresSafeArea().onTapGesture { dismiss() }.transition(.opacity)
                VStack(alignment: .leading, spacing: Space.lg) {
                    Text(spec.title).fbText(.title).accessibilityAddTraits(.isHeader)
                    if let body = spec.body { Text(body).fbText(.small, color: theme.textMuted).fixedSize(horizontal: false, vertical: true) }
                    if let content = spec.content { content }
                    HStack(spacing: Space.sm) {
                        Spacer(minLength: 0)
                        if let dismissLabel = spec.dismissLabel { FbButton(dismissLabel, kind: .quiet, height: 44, fill: false) { dismiss() } }
                        FbButton(spec.confirmLabel, kind: spec.destructive ? .danger : .primary, height: 44, fill: false) { spec.onConfirm() }
                    }
                }
                .padding(Space.xxl)
                .frame(maxWidth: 420)
                .background(theme.raised, in: Shapes.panel)
                .overlay(Shapes.panel.strokeBorder(theme.border, lineWidth: 1))
                .shadow(color: theme.shadow, radius: 24, y: 10)
                .padding(.horizontal, Space.xxl)
                .transition(.scale(scale: 0.94).combined(with: .opacity))
                .accessibilityAddTraits(.isModal)
            }
        }
        .animation(reduceMotion ? nil : Motion.spatial, value: spec?.id)
    }
}
