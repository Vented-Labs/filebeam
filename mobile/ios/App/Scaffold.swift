import SwiftUI

/// Height reserved at the bottom of tab screens so content clears the floating bar.
let navBarClearance: CGFloat = 96

/// The brand row at the top of tab screens: mark, wordmark and the account entry point.
struct BrandBar<Actions: View>: View {
    @Environment(\.theme) private var theme
    @Environment(AppModel.self) private var model
    @ViewBuilder var actions: () -> Actions

    init(@ViewBuilder actions: @escaping () -> Actions = { EmptyView() }) { self.actions = actions }

    var body: some View {
        HStack(spacing: 0) {
            FilebeamMark(height: 26)
            Text("Filebeam").fbText(.title).fontWeight(.heavy).padding(.leading, Space.sm)
            Spacer(minLength: Space.sm)
            actions()
            IconAction(.settings, "Settings") { model.push(.settings) }
            Button {
                Haptics.tap()
                model.push(model.session != nil ? .account : .auth(.login))
            } label: {
                Group {
                    if let name = model.session?.displayName.prefix(1) {
                        Text(name.uppercased()).fbText(.label, color: .white).frame(width: 40, height: 40).background(theme.avatarGradient, in: Circle())
                    } else {
                        FbIcon(.user, size: 20, tint: theme.textMuted).frame(width: 40, height: 40).background(theme.surface, in: Circle())
                    }
                }
                .overlay(Circle().strokeBorder(model.session != nil ? theme.actionBorder : theme.controlBorder, lineWidth: 1))
                .padding(.leading, Space.sm)
            }
            .buttonStyle(.pressScale(0.92))
            .accessibilityLabel(model.session.map { "Account, signed in as \($0.displayName)" } ?? "Sign in or create an account")
        }
        .frame(height: 56)
        .padding(.horizontal, Space.gutter)
    }
}

/// A scrolling tab screen. The bottom is padded so the last item clears the floating navigation bar.
struct TabScreen<Actions: View, Content: View>: View {
    @ViewBuilder var actions: () -> Actions
    @ViewBuilder var content: () -> Content

    init(@ViewBuilder actions: @escaping () -> Actions = { EmptyView() }, @ViewBuilder content: @escaping () -> Content) {
        self.actions = actions; self.content = content
    }

    var body: some View {
        VStack(spacing: 0) {
            BrandBar(actions: actions).frame(maxWidth: contentMaxWidth)
            ScrollView {
                VStack(alignment: .leading, spacing: Space.lg) { content() }
                    .frame(maxWidth: contentMaxWidth)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, Space.gutter)
                    .padding(.top, Space.sm)
                    .padding(.bottom, navBarClearance + Space.xxl)
            }
            .scrollDismissesKeyboard(.interactively)
            .scrollIndicators(.hidden)
        }
    }
}

/// A pushed screen with a back affordance and a title that gains a divider once content scrolls under it.
struct SubScreen<Actions: View, Content: View, Bottom: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let title: String
    @ViewBuilder var actions: () -> Actions
    @ViewBuilder var bottom: () -> Bottom
    @ViewBuilder var content: () -> Content
    @State private var scrolled = false

    init(_ title: String, @ViewBuilder actions: @escaping () -> Actions = { EmptyView() }, @ViewBuilder bottom: @escaping () -> Bottom = { EmptyView() }, @ViewBuilder content: @escaping () -> Content) {
        self.title = title; self.actions = actions; self.bottom = bottom; self.content = content
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: Space.xs) {
                IconAction(.chevronLeft, "Back", tint: theme.text) { dismiss() }
                Text(title).fbText(.title).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading).accessibilityAddTraits(.isHeader)
                actions()
            }
            .frame(maxWidth: contentMaxWidth)
            .frame(height: 56)
            .padding(.horizontal, Space.sm)
            Rectangle().fill(theme.border).frame(height: 1).opacity(scrolled ? 1 : 0).motion(Motion.fade, value: scrolled)
            ScrollView {
                VStack(alignment: .leading, spacing: Space.lg) { content() }
                    .frame(maxWidth: contentMaxWidth)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, Space.gutter)
                    .padding(.top, Space.lg)
                    .padding(.bottom, Space.xxxl)
                    .background(GeometryReader { proxy in
                        Color.clear.preference(key: ScrollOffsetKey.self, value: proxy.frame(in: .named("subscreen")).minY)
                    })
            }
            .coordinateSpace(name: "subscreen")
            .onPreferenceChange(ScrollOffsetKey.self) { value in
                Task { @MainActor in scrolled = value < -4 }
            }
            .scrollDismissesKeyboard(.interactively)
            .scrollIndicators(.hidden)
            bottom().frame(maxWidth: contentMaxWidth).padding(.horizontal, Space.gutter)
        }
        .toolbar(.hidden, for: .navigationBar)
        .enableSwipeBack()
    }
}

private struct ScrollOffsetKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

/// Hiding the navigation bar disables the interactive pop gesture; this restores it.
private struct SwipeBackEnabler: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> UIViewController { SwipeBackController() }
    func updateUIViewController(_ controller: UIViewController, context: Context) {}

    final class SwipeBackController: UIViewController, UIGestureRecognizerDelegate {
        override func didMove(toParent parent: UIViewController?) {
            super.didMove(toParent: parent)
            Task { @MainActor in
                guard let navigation = self.navigationController ?? self.parent?.navigationController else { return }
                navigation.interactivePopGestureRecognizer?.isEnabled = true
                navigation.interactivePopGestureRecognizer?.delegate = self
            }
        }
        func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
            (navigationController ?? parent?.navigationController).map { $0.viewControllers.count > 1 } ?? false
        }
    }
}

extension View {
    func enableSwipeBack() -> some View { background(SwipeBackEnabler().frame(width: 0, height: 0)) }
}

/// The floating navigation bar. A selection pill slides between destinations and the
/// chosen icon gives a small bounce; the inbox shows an unread badge that pops in.
struct FloatingNavBar: View {
    @Environment(\.theme) private var theme
    @Binding var selected: Tab
    let inboxUnread: Int
    @Namespace private var namespace

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Tab.allCases) { tab in
                let isSelected = tab == selected
                Button {
                    guard !isSelected else { return }
                    // Editing ends before the switch so the keyboard, and the bar above it, settle at once.
                    hideKeyboard()
                    Haptics.tick()
                    withMotion(Motion.spatial) { selected = tab }
                } label: {
                    VStack(spacing: 2) {
                        FbIcon(tab.icon, size: 22, tint: isSelected ? theme.accentText : theme.textMuted, weight: isSelected ? .semibold : .medium)
                            .scaleEffect(isSelected ? 1.08 : 1)
                            .overlay(alignment: .topTrailing) {
                                if tab == .inbox && inboxUnread > 0 {
                                    Text(inboxUnread > 9 ? "9+" : "\(inboxUnread)")
                                        .font(.system(size: 10, weight: .bold))
                                        .foregroundStyle(theme.onAction)
                                        .frame(width: 18, height: 18)
                                        .background(theme.action, in: Circle())
                                        .overlay(Circle().strokeBorder(theme.raised, lineWidth: 1.5))
                                        .offset(x: 8, y: -6)
                                        .transition(.scale.combined(with: .opacity))
                                }
                            }
                        Text(tab.title).fbText(.caption, color: isSelected ? theme.accentText : theme.textMuted).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background {
                        if isSelected {
                            Capsule().fill(theme.selectedSurface)
                                .overlay(Capsule().strokeBorder(theme.choiceBorder.opacity(0.6), lineWidth: 1))
                                .matchedGeometryEffect(id: "indicator", in: namespace)
                        }
                    }
                    .contentShape(Capsule())
                }
                .buttonStyle(.pressScale(0.94))
                .accessibilityLabel(tab == .inbox && inboxUnread > 0 ? "Inbox, \(inboxUnread) unread \(inboxUnread == 1 ? "delivery" : "deliveries")" : tab.title)
                .accessibilityAddTraits(isSelected ? [.isSelected] : [])
            }
        }
        .padding(6)
        .frame(maxWidth: 440)
        .frame(height: 68)
        .dynamicTypeSize(...DynamicTypeSize.xxxLarge)
        .background(theme.raised, in: Capsule())
        .overlay(Capsule().strokeBorder(theme.border, lineWidth: 1))
        .shadow(color: theme.shadow, radius: 20, y: 8)
        .motion(Motion.bouncy, value: inboxUnread > 0)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Navigation")
    }
}
