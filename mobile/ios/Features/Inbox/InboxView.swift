import SwiftUI
import FilebeamDomain

/// Files other people sent to this account. Everything stays encrypted until it is unlocked here.
struct InboxView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirming: TransferID?
    @State private var opening: InboxItem?
    @State private var unlocking: StagedInboxItem?
    @State private var verified: [URL] = []
    @State private var exporting: URL?

    var body: some View {
        let session = model.session
        TabScreen(actions: { if session != nil { IconAction(.refresh, "Refresh") { Task { await refresh() } } } }) {
            ScreenHeader("Inbox", subtitle: "Received files stay encrypted until you unlock them. Filenames are never included in notifications.").staggered(0)
            if let session {
                if !session.inboxEnabled {
                    Notice("Your secure inbox is not accepting transfers.", tone: .warning) {
                        FbButton("Set up", kind: .secondary, height: 40, fill: false) { model.push(.secureInbox) }
                    }
                }
                if let error = model.inboxError {
                    Notice(error, tone: .danger) { FbButton("Retry", kind: .secondary, height: 40, fill: false) { Task { await refresh() } } }
                }
                if let items = model.inboxItems {
                    if items.isEmpty {
                        EmptyState(.inbox, "No incoming files yet", body: "Files sent to your username appear here.")
                    } else {
                        ForEach(items) { item in
                            InboxRow(item: item, confirming: confirming == item.id, onOpen: { opening = item }, onAskDelete: { confirming = item.id }, onCancelDelete: { confirming = nil }) {
                                Task {
                                    do { try await model.deleteInboxItem(item); model.toast("Delivery deleted", tone: .success) } catch { model.fail(error) }
                                    confirming = nil
                                }
                            }
                            .transition(.opacity.combined(with: .move(edge: .top)))
                        }
                    }
                } else if model.inboxError == nil { CenteredSpinner() }

                SectionLabel("On this device")
                Panel {
                    SwitchRow("Receive automatically on this device", isOn: Binding(get: { model.automaticReceiving }, set: { enabled in Task { do { try await model.setAutomaticReceiving(enabled) } catch { model.fail(error) } } }), description: "Encrypted deliveries are fetched in the background and stay locked until you open them.", icon: .download)
                        .padding(.horizontal, Space.lg)
                    ForEach(model.stagedInbox) { entry in
                        Hairline()
                        HStack(spacing: Space.md) {
                            FbIcon(.lock, size: 22)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(formatBytes(entry.bytes)).fbText(.bodyStrong)
                                Text(entry.state == "staged-locked" ? "Encrypted and ready to unlock" : "Waiting to be fetched").fbText(.small, color: theme.textMuted)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            if entry.state == "staged-locked" { FbButton("Unlock", kind: .secondary, height: 40, fill: false) { unlocking = entry } }
                            IconAction(.trash, "Remove local ciphertext") { Task { do { try await model.dismissStagedInbox(entry) } catch { model.fail(error) } } }
                        }
                        .padding(Space.lg)
                    }
                    ForEach(verified, id: \.self) { file in
                        Hairline()
                        HStack(spacing: Space.md) {
                            FbIcon(fileIcon(for: file.lastPathComponent), size: 22, tint: theme.accentText)
                            Text(file.lastPathComponent).fbText(.bodyStrong).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                            FbButton("Save", kind: .secondary, height: 40, fill: false) { exporting = file }
                        }
                        .padding(Space.lg)
                    }
                }
                Panel {
                    LinkRow(.users, "Manage friend permissions") { model.push(.contacts) }
                    Hairline()
                    LinkRow(.shield, "Receiving settings") { model.push(.secureInbox) }
                }
            } else {
                EmptyState(.inbox, "Sign in to see your inbox", body: "Friends and people with your profile link can send files straight to your account.") {
                    FbButton("Sign in", icon: .login, fill: false) { model.push(.auth(.login)) }
                }
            }
        }
        .motion(Motion.spatial, value: model.inboxItems?.map(\.id))
        .task(id: session?.id) { await refresh() }
        .refreshable { await refresh() }
        .sheet(item: $opening) { item in
            UnlockSheet(title: "Unlock received files", body: "Your receiving key opens this delivery on this device. The secret is not sent anywhere.") { password in
                try await model.receiveInbox(item: item, password: password)
            }
        }
        .sheet(item: $unlocking) { entry in
            UnlockSheet(title: "Unlock received files", body: "Unlock the files already fetched to this device and verify them.") { password in
                verified = try await model.verifyStagedInbox(entry, password: password)
            }
        }
        .sheet(item: Binding(get: { exporting.map(ExportFile.init) }, set: { if $0 == nil { exporting = nil } })) { item in
            DocumentExportSheet(urls: [item.url]) { result in
                exporting = nil
                if case .success = result { model.toast("Saved", tone: .success) } else if case let .failure(error) = result { model.fail(error) }
            }
            .ignoresSafeArea()
        }
    }

    private func refresh() async {
        guard model.session != nil else { return }
        await model.refreshInbox()
        await model.refreshAutomaticInbox()
        await model.markInboxSeen()
    }
}

private struct ExportFile: Identifiable {
    let url: URL
    init(_ url: URL) { self.url = url }
    var id: String { url.path }
}

private struct InboxRow: View {
    @Environment(\.theme) private var theme
    let item: InboxItem
    let confirming: Bool
    let onOpen: () -> Void
    let onAskDelete: () -> Void
    let onCancelDelete: () -> Void
    let onDelete: () -> Void

    var body: some View {
        let uploading = item.completedAt == nil
        Panel(radius: Radius.card) {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.md) {
                    FbIcon(.lock, size: 24, tint: theme.accentText).frame(width: 44, height: 44)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(uploading ? "Turbo upload in progress" : item.itemCount == 1 ? "1 encrypted file" : "\(item.itemCount) encrypted files").fbText(.bodyStrong).lineLimit(1)
                        Text([formatBytes(item.ciphertextBytes), item.completedAt.map { "received \(formatRelative($0))" }].compactMap { $0 }.joined(separator: " · ")).fbText(.small, color: theme.textMuted).lineLimit(1)
                        if let expires = item.expiresAt { Text("Expires \(formatAbsolute(expires))").fbText(.caption, color: theme.textSubtle).lineLimit(1) }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                if confirming {
                    HStack(spacing: Space.md) {
                        Text("Permanently delete?").fbText(.smallStrong, color: theme.danger.ink).frame(maxWidth: .infinity, alignment: .leading)
                        FbButton("Cancel", kind: .quiet, height: 40, fill: false, action: onCancelDelete)
                        FbButton("Delete files", kind: .danger, height: 40, fill: false, action: onDelete)
                    }
                    .transition(.opacity)
                } else {
                    HStack(spacing: Space.md) {
                        FbButton("Unlock and download", icon: .unlock, height: 44, action: onOpen)
                        FbButton("Delete", kind: .danger, height: 44, fill: false, action: onAskDelete)
                    }
                    .transition(.opacity)
                }
            }
            .padding(Space.md)
            .motion(Motion.fade, value: confirming)
        }
    }
}
