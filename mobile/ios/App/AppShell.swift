import SwiftUI
import FilebeamDomain

/// The root of the app: ambient backdrop, the navigation stack, the floating navigation bar,
/// toasts and dialogs. Every screen below reads the theme and the app model from the environment.
struct AppShell: View {
    @Bindable var model: AppModel
    @Environment(\.colorScheme) private var systemScheme
    @Environment(\.scenePhase) private var scenePhase
    @State private var draftSaveTask: Task<Void, Never>?

    private var theme: Theme {
        Theme.resolve(model.appearance.appearance, systemDark: systemScheme == .dark, instanceColor: model.branding.primaryColor)
    }

    var body: some View {
        ZStack {
            NavigationStack(path: $model.path) {
                HomeView(model: model)
                    .navigationDestination(for: Route.self) { route in RouteView(route: route) }
                    .toolbar(.hidden, for: .navigationBar)
            }
            FbDialogHost(spec: model.dialog) { model.dismissDialog() }
        }
        .overlay(alignment: .bottom) {
            ToastHost(message: model.toast, onDismissed: model.toastDismissed)
                .padding(.bottom, model.path.isEmpty ? navBarClearance : Space.xxl)
        }
        .environment(\.theme, theme)
        .environment(model)
        .tint(theme.accentText)
        .preferredColorScheme(model.appearance.appearance.mode.colorScheme)
        .sheet(item: $model.activePrompt) { PromptSheet(model: model, context: $0) }
        .sheet(item: Binding(get: { model.receipt }, set: { if $0 == nil { model.receipt = nil } })) { ShareReceiptSheet(receipt: $0) }
        .sheet(item: Binding(get: { model.downloadCommand.map(CommandSheetItem.init) }, set: { if $0 == nil { model.downloadCommand = nil } })) { DownloadCommandSheet(command: $0.command) }
        .task {
            await model.load()
            if !model.appearance.hasSeenTour { model.push(.tour) }
            applyDebugRoute()
            if let paths = model.paths, let group = paths.group { await model.processPendingSharedDrafts(root: group) }
        }
        .task { await model.heartbeat() }
        .onOpenURL { model.handle(url: $0) }
        .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { activity in
            if let url = activity.webpageURL { model.handle(url: url) }
        }
        .onChange(of: model.drafts) { _, drafts in
            draftSaveTask?.cancel()
            draftSaveTask = Task {
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled else { return }
                await model.persistDrafts(drafts)
            }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { InboxBackgroundReceiving.schedule() }
            if phase == .active, let paths = model.paths, let group = paths.group { Task { await model.processPendingSharedDrafts(root: group) } }
        }
    }
}

extension AppShell {
    /// Debug builds accept `-filebeam.screen <name>` so every screen can be captured without a backend.
    fileprivate func applyDebugRoute() {
        #if DEBUG
        guard let screen = UserDefaults.standard.string(forKey: "filebeam.screen") else { return }
        model.appearance.hasSeenTour = true
        model.path.removeAll()
        switch screen {
        case "send": model.select(.send)
        case "notes": model.select(.send); model.send.mode = .notes
        case "receive": model.select(.receive)
        case "transfers": model.select(.transfers)
        case "inbox": model.select(.inbox)
        case "auth": model.push(.auth(.login))
        case "register": model.push(.auth(.register))
        case "account": model.push(.account)
        case "settings": model.push(.settings)
        case "appearance": model.push(.appearance)
        case "about": model.push(.about)
        case "tour": model.push(.tour)
        case "changelog": model.push(.changelog)
        case "contacts": model.push(.contacts)
        case "devices": model.push(.devices)
        case "history": model.push(.history)
        case "storage": model.push(.storage)
        case "secure-inbox": model.push(.secureInbox)
        default: if screen.hasPrefix("recipient:") { model.push(.recipient(String(screen.dropFirst(10)))) }
        }
        switch UserDefaults.standard.string(forKey: "filebeam.fixture") {
        case "queue":
            model.send.addSources([
                FileSource(name: "holiday-photos.zip", location: "/dev/null", sizeBytes: 48_300_000),
                FileSource(name: "quarterly-report.pdf", location: "/dev/null", sizeBytes: 2_400_000),
                FileSource(name: "notes.md", location: "/dev/null", sizeBytes: 12_800),
            ])
        case "code":
            model.send.mode = .notes
            model.send.note.title = "Deploy checklist"
            model.send.note.language = .typescript
            model.send.note.text = """
            // Rotate the signing key before release
            import { createClient } from "./client";

            const limits: Record<string, number> = { http: 10, webrtc: 2 };
            export async function publish(tag: string): Promise<boolean> {
              const client = createClient({ retries: 3 });
              return client.release(tag, { prerelease: tag.includes("beta") });
            }
            """
        default: break
        }
        #endif
    }
}

extension ShareReceipt: @retroactive Identifiable {
    public var id: String { transferID.rawValue + link }
}

private struct CommandSheetItem: Identifiable {
    let command: String
    var id: String { command }
}

/// The root entry: the selected tab cross-fades through, and each tab keeps its own model state.
private struct HomeView: View {
    @Bindable var model: AppModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack(alignment: .bottom) {
            AmbientBackground(energy: model.transfers.contains { $0.lifecycle == .running } ? 0.6 : 0)
            ZStack {
                switch model.selectedTab {
                case .send: SendView().transition(tabTransition)
                case .receive: ReceiveView().transition(tabTransition)
                case .transfers: TransfersView().transition(tabTransition)
                case .inbox: InboxView().transition(tabTransition)
                }
            }
            .animation(reduceMotion ? nil : Motion.enterQuick, value: model.selectedTab)
            FloatingNavBar(selected: $model.selectedTab, inboxUnread: Int(model.inboxUnreadCount))
                .padding(.horizontal, Space.gutter)
                .padding(.bottom, 12)
        }
        .ignoresSafeArea(.keyboard, edges: .bottom)
    }

    private var tabTransition: AnyTransition {
        .asymmetric(insertion: .opacity.combined(with: .scale(scale: 0.98)), removal: .opacity)
    }
}

/// Pushed destinations. Each draws its own backdrop so the navigation stack never flashes a system colour.
private struct RouteView: View {
    let route: Route
    var body: some View {
        ZStack {
            AmbientBackground()
            switch route {
            case .account: AccountView()
            case let .auth(mode): AuthView(initialMode: mode)
            case .secureInbox: SecureInboxView()
            case .contacts: ContactsView()
            case .devices: DevicesView()
            case .history: HistoryView()
            case .appearance: AppearanceView()
            case .settings: SettingsView()
            case .storage: StorageView()
            case .about: AboutView()
            case .tour: TourView()
            case .changelog: ChangelogView()
            case let .release(tag): ReleaseView(tag: tag)
            case let .recipient(username): RecipientView(username: username)
            }
        }
        .toolbar(.hidden, for: .navigationBar)
    }
}
