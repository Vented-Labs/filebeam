import SwiftUI
import FilebeamDomain

private enum ContactsTab: Hashable { case friends, requests, blocked }

let receivingPolicies: [(String, String)] = [
    ("anyone", "Anyone, including anonymous senders"), ("authenticated", "Signed-in users"), ("friends", "Friends only"), ("nobody", "Nobody, unless explicitly allowed"),
]

/// "Who can send me files" and automatic downloads from friends. Shared with the Secure inbox screen.
struct ReceivingDefaultsPanel: View {
    @Environment(\.theme) private var theme
    let directory: ContactDirectory?
    let onChange: (String, Bool) -> Void

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.md) {
                Text("Who can send me files").fbText(.bodyStrong)
                if let directory {
                    ForEach(receivingPolicies, id: \.0) { key, label in
                        ChoiceRow(label, selected: directory.settings.receivingPolicy == key) { onChange(key, directory.settings.autoDownloadFriends) }
                    }
                    Hairline()
                    SwitchRow("Automatically download from friends", isOn: Binding(get: { directory.settings.autoDownloadFriends }, set: { onChange(directory.settings.receivingPolicy, $0) }), description: "Encrypted deliveries from friends are fetched in the background.", icon: .download)
                } else { CenteredSpinner() }
            }
            .padding(Space.lg)
        }
    }
}

/// Friends, pending requests and blocked accounts, with per-friend receiving preferences.
struct ContactsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var directory: ContactDirectory?
    @State private var tab: ContactsTab = .friends
    @State private var error: String?
    @State private var username = ""
    @State private var addError: String?
    @State private var adding = false
    @State private var search = ""
    @State private var confirming: (String, String)?
    @State private var preferences: Contact?
    @State private var defaultsOpen = false

    private var friends: [Contact] { directory?.contacts.filter { $0.status == "accepted" } ?? [] }
    private var incoming: [Contact] { directory?.contacts.filter { $0.status == "incoming" } ?? [] }
    private var outgoing: [Contact] { directory?.contacts.filter { $0.status == "outgoing" } ?? [] }
    private var blocked: [BlockedContact] { directory?.blocked ?? [] }

    var body: some View {
        SubScreen("Friends") {
            Panel {
                VStack(spacing: Space.md) {
                    FbTextField($username, label: "Add a friend", placeholder: "@username", error: addError, icon: .user, submitLabel: .send, onSubmit: submit)
                        .onChange(of: username) { _, value in
                            username = String(value.lowercased().filter { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "@" }.prefix(25))
                            addError = nil
                        }
                    FbButton("Send friend request", icon: .plus, enabled: username.trimmingCharacters(in: CharacterSet(charactersIn: "@")).count >= 3, loading: adding) { submit() }
                }
                .padding(Space.lg)
            }
            if let error { Notice(error, tone: .danger) { FbButton("Retry", kind: .secondary, height: 40, fill: false) { Task { await load() } } } }
            SegmentedRail(segments: [
                Segment(value: ContactsTab.friends, label: "Friends \(friends.count)"),
                Segment(value: ContactsTab.requests, label: incoming.isEmpty ? "Requests" : "\(incoming.count) incoming"),
                Segment(value: ContactsTab.blocked, label: "Blocked \(blocked.count)"),
            ], selection: $tab)
            if directory == nil && error == nil { CenteredSpinner() }
            switch tab {
            case .friends:
                if friends.count > 8 { FbTextField($search, placeholder: "Find a friend", icon: .search) }
                let shown = friends.filter { search.isBlank || $0.username.localizedCaseInsensitiveContains(search.trimmingCharacters(in: CharacterSet(charactersIn: "@ "))) }
                if directory != nil && friends.isEmpty {
                    EmptyState(.users, "Add your first friend", body: "Friends can send files straight to each other's encrypted inbox.")
                } else if directory != nil && shown.isEmpty {
                    EmptyState(.search, "No matching friends", tone: .neutral)
                } else {
                    ForEach(shown) { friend in
                        ContactCard(username: friend.username, badge: badge(friend), confirming: confirming.flatMap { $0.0 == friend.username ? $0.1 : nil }, onCancel: { confirming = nil }, onConfirm: { action in
                            act(friend.username, action, action == "block" ? "Blocked @\(friend.username)" : "Removed @\(friend.username)")
                        }) {
                            FbButton("Send files", icon: .upload, height: 44) { model.pendingRecipient = friend.username; model.select(.send) }
                            IconAction(.settings, "Receiving preferences", bordered: true) { preferences = friend }
                            IconAction(.link, "Copy profile link", bordered: true) { Clipboard.copySensitive("\(model.instance.origin)/u/\(friend.username)"); model.toast("Copied", tone: .success) }
                            IconAction(.trash, "Remove friend", bordered: true) { confirming = (friend.username, "remove") }
                            IconAction(.slash, "Block account", tint: theme.danger.ink, bordered: true) { confirming = (friend.username, "block") }
                        }
                    }
                }
            case .requests:
                if directory != nil && incoming.isEmpty && outgoing.isEmpty { EmptyState(.bell, "Friend requests will appear here") }
                if !incoming.isEmpty { SectionLabel("Incoming") }
                ForEach(incoming) { request in
                    ContactCard(username: request.username, badge: nil, confirming: confirming.flatMap { $0.0 == request.username ? $0.1 : nil }, onCancel: { confirming = nil }, onConfirm: { _ in act(request.username, "block", "Blocked @\(request.username)") }) {
                        FbButton("Accept", height: 44) { act(request.username, "accept", "You and @\(request.username) are now friends") }
                        FbButton("Decline", kind: .secondary, height: 44, fill: false) { act(request.username, "decline", "Declined @\(request.username)") }
                        IconAction(.slash, "Block account", tint: theme.danger.ink, bordered: true) { confirming = (request.username, "block") }
                    }
                }
                if !outgoing.isEmpty { SectionLabel("Sent") }
                ForEach(outgoing) { request in
                    ContactCard(username: request.username, badge: ("Pending", .neutral), confirming: confirming.flatMap { $0.0 == request.username ? $0.1 : nil }, onCancel: { confirming = nil }, onConfirm: { _ in act(request.username, "block", "Blocked @\(request.username)") }) {
                        FbButton("Cancel request", kind: .secondary, height: 44) { act(request.username, "cancel", "Cancelled the request to @\(request.username)") }
                        IconAction(.slash, "Block account", tint: theme.danger.ink, bordered: true) { confirming = (request.username, "block") }
                    }
                }
            case .blocked:
                if directory != nil && blocked.isEmpty { EmptyState(.slash, "No blocked accounts", tone: .neutral) }
                ForEach(blocked) { contact in
                    ContactCard(username: contact.username, badge: ("Blocked", .danger), confirming: confirming.flatMap { $0.0 == contact.username ? $0.1 : nil }, onCancel: { confirming = nil }, onConfirm: { _ in act(contact.username, "unblock", "Unblocked @\(contact.username)") }) {
                        FbButton("Unblock", kind: .secondary, height: 44) { confirming = (contact.username, "unblock") }
                    }
                }
            }
            SectionLabel("Receiving defaults")
            VStack(spacing: Space.md) {
                let policy = receivingPolicies.first { $0.0 == directory?.settings.receivingPolicy }?.1 ?? "Receiving defaults"
                FbButton(policy + (directory?.settings.autoDownloadFriends == true ? " · auto-download on" : ""), kind: .secondary, trailingIcon: defaultsOpen ? .chevronUp : .chevronDown) { withMotion(Motion.spatial) { defaultsOpen.toggle() } }
                if defaultsOpen {
                    ReceivingDefaultsPanel(directory: directory) { policy, auto in
                        Task {
                            do { try await model.service.receivingDefaults(instance: model.instance, policy: policy, autoDownload: auto) } catch { self.error = readableMessage(error) }
                            await load()
                        }
                    }
                    .transition(.opacity.combined(with: .move(edge: .top)))
                }
            }
            if model.policy?.usernameRoutingEnabled == false { Notice("New username inbox setup is unavailable on this instance.", tone: .warning) }
        }
        .task { await load() }
        .onChange(of: model.session == nil) { _, signedOut in if signedOut { model.replace(.auth(.login)) } }
        .sheet(item: $preferences) { friend in PreferencesSheet(friend: directory?.contacts.first { $0.id == friend.id } ?? friend) { canSend, auto in act(friend.username, "preferences", "Preferences saved", canSend: canSend, autoDownload: auto) } }
    }

    private func badge(_ friend: Contact) -> (String, Tone)? {
        if !friend.effective.canSend { return ("Receiving off", .warning) }
        if friend.canSend != nil || friend.autoDownload != nil { return ("Custom preferences", .accent) }
        return nil
    }

    private func load() async {
        do { directory = try await model.service.contacts(instance: model.instance); error = nil } catch { self.error = readableMessage(error) }
    }

    /// Runs one contact action, announces the outcome, and refreshes on failure.
    private func act(_ name: String, _ action: String, _ done: String, canSend: Bool? = nil, autoDownload: Bool? = nil) {
        Task {
            do {
                directory = try await model.service.contactAction(instance: model.instance, username: name, action: action, canSend: canSend, autoDownload: autoDownload)
                error = nil
                model.toast(done, tone: .success)
                UIAccessibility.post(notification: .announcement, argument: done)
            } catch {
                self.error = readableMessage(error)
                await load()
            }
            confirming = nil
        }
    }

    private func submit() {
        let name = username.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "@", with: "").lowercased()
        let rule = /^[a-z0-9_]{3,24}$/
        addError = name.wholeMatch(of: rule) == nil ? "3 to 24 lowercase letters, numbers or underscores."
            : friends.contains { $0.username == name } ? "You are already friends with this account."
            : incoming.contains { $0.username == name } ? "This account has already requested to be your friend."
            : outgoing.contains { $0.username == name } ? "A request to this account is already pending." : nil
        guard addError == nil else { return }
        adding = true
        Task {
            do {
                directory = try await model.service.contactAction(instance: model.instance, username: name, action: "request", canSend: nil, autoDownload: nil)
                username = ""
                model.toast("Request sent to @\(name)", tone: .success)
            } catch { addError = readableMessage(error) }
            adding = false
        }
    }
}

/// One person with their actions. A destructive action swaps the action row for an inline confirmation.
private struct ContactCard<Actions: View>: View {
    @Environment(\.theme) private var theme
    let username: String
    let badge: (String, Tone)?
    let confirming: String?
    let onCancel: () -> Void
    let onConfirm: (String) -> Void
    @ViewBuilder let actions: () -> Actions

    var body: some View {
        Panel(radius: Radius.card) {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.md) {
                    Avatar(name: username)
                    Text("@\(username)").fbText(.bodyStrong).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                    if let badge { StatusPill(badge.0, tone: badge.1) }
                }
                if let action = confirming {
                    HStack(spacing: Space.sm) {
                        Text(action == "block" ? "Block @\(username)?" : action == "unblock" ? "Unblock @\(username)?" : "Remove @\(username)?").fbText(.smallStrong, color: theme.danger.ink).frame(maxWidth: .infinity, alignment: .leading)
                        FbButton("Cancel", kind: .quiet, height: 40, fill: false, action: onCancel)
                        FbButton("Confirm", kind: .danger, height: 40, fill: false) { onConfirm(action) }
                    }
                    .transition(.opacity)
                } else {
                    HStack(spacing: Space.sm) { actions() }.transition(.opacity)
                }
            }
            .padding(Space.md)
            .motion(Motion.fade, value: confirming)
        }
    }
}

private struct PreferencesSheet: View {
    @Environment(\.theme) private var theme
    let friend: Contact
    let onChange: (Bool?, Bool?) -> Void

    var body: some View {
        FbSheet(title: "Preferences for @\(friend.username)") {
            Text("Can send me files").fbText(.bodyStrong)
            ForEach([(nil, "Use account default"), (true, "Allow"), (false, "Don't allow")], id: \.1) { value, label in
                ChoiceRow(label, selected: friend.canSend == value) { onChange(value, friend.autoDownload) }
            }
            Text("Currently: \(friend.effective.canSend ? "Allow" : "Don't allow")").fbText(.caption, color: theme.textMuted)
            Text("Automatic download").fbText(.bodyStrong)
            ForEach([(nil, "Use account default"), (true, "On"), (false, "Off")], id: \.1) { value, label in
                ChoiceRow(label, selected: friend.autoDownload == value) { onChange(friend.canSend, value) }
            }
            Text("Currently: \(friend.effective.autoDownload ? "On" : "Off")").fbText(.caption, color: theme.textMuted)
            FbButton("Reset to defaults", kind: .quiet, enabled: friend.canSend != nil || friend.autoDownload != nil) { onChange(nil, nil) }
        }
    }
}
