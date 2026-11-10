import SwiftUI
import FilebeamDomain

/// The account hub: identity, the profile link, and the way into everything account-related.
struct AccountView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    var body: some View {
        SubScreen("Account") {
            if let session = model.session {
                Panel {
                    VStack(alignment: .leading, spacing: Space.lg) {
                        HStack(spacing: Space.lg) {
                            Avatar(name: session.displayName, size: 56)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(session.name.isBlank ? session.displayName : session.name).fbText(.title).lineLimit(1)
                                Text("@\(session.displayName)").fbText(.small, color: theme.textMuted).lineLimit(1)
                                if !session.email.isBlank { Text(session.email).fbText(.small, color: theme.textMuted).lineLimit(1) }
                            }
                        }
                        if session.emailVerifiedAt != nil {
                            StatusPill("Email verified", tone: .success, icon: .check)
                        } else {
                            Notice("Verify your email to receive files by username.", tone: .warning) {
                                FbButton("Verify email", kind: .secondary, height: 40, fill: false) { model.push(.auth(.verify)) }
                            }
                        }
                        if let url = session.profileURL, model.policy?.usernameRoutingEnabled != false {
                            VStack(alignment: .leading, spacing: Space.sm) {
                                Text("Profile link").fbText(.smallStrong)
                                CopyField(icon: .link, value: url, label: "Copy") { Clipboard.copySensitive(url); model.toast("Copied", tone: .success) }
                                Text("Anyone with this link can send files to your encrypted inbox, within your receiving settings.").fbText(.caption, color: theme.textMuted)
                            }
                        }
                    }
                    .padding(Space.lg)
                }
                .staggered(0)
                SectionLabel("Sharing")
                Panel {
                    LinkRow(.shield, "Secure inbox", detail: session.inboxEnabled ? "On" : "Off") { model.push(.secureInbox) }
                    Hairline()
                    LinkRow(.users, "Friends") { model.push(.contacts) }
                    Hairline()
                    LinkRow(.clock, "Transfer history") { model.push(.history) }
                    Hairline()
                    LinkRow(.mobile, "My devices") { model.push(.devices) }
                }
                .staggered(1)
                SectionLabel("App")
                Panel {
                    LinkRow(.brush, "Appearance") { model.push(.appearance) }
                    Hairline()
                    LinkRow(.settings, "Settings") { model.push(.settings) }
                    Hairline()
                    LinkRow(.info, "About") { model.push(.about) }
                }
                .staggered(2)
                VStack(spacing: Space.sm) {
                    FbButton("Sign out", kind: .secondary, icon: .logout) { Task { await model.logout(); model.pop() } }
                    FbButton("Delete account", kind: .danger) { confirmDeletion() }
                }
                .staggered(3)
            }
        }
        .onChange(of: model.session == nil, initial: true) { _, signedOut in if signedOut { model.replace(.auth(.login)) } }
    }

    private func confirmDeletion() {
        model.present(FbDialogSpec(title: "Delete this account?", body: "Your account, inbox, friends and devices are removed permanently. Transfers you already shared expire on their own schedule.", confirmLabel: "Delete account", destructive: true, content: AnyView(DeletionPassword()), onConfirm: {}))
    }
}

/// Password entry inside the deletion dialog; it performs the deletion itself so the password never leaves this view.
private struct DeletionPassword: View {
    @Environment(AppModel.self) private var model
    @State private var password = ""
    @State private var busy = false
    @State private var error: String?
    var body: some View {
        VStack(spacing: Space.md) {
            PasswordField($password, label: "Current password", error: error)
            FbButton("Confirm deletion", kind: .danger, enabled: !password.isEmpty, loading: busy) {
                busy = true
                error = nil
                Task {
                    do {
                        try await model.deleteAccount(password: password)
                        model.dismissDialog()
                        model.toast("Account deleted", tone: .success)
                        model.select(.send)
                    } catch { self.error = readableMessage(error) }
                    busy = false
                }
            }
        }
        .privacySensitive()
    }
}
