import SwiftUI
import FilebeamDomain

/// The landing screen for a profile link: confirm who the files go to, then choose them.
struct RecipientView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let username: String
    @State private var recipient: Recipient?
    @State private var failed = false
    @State private var showFingerprint = false

    var body: some View {
        SubScreen("@\(username)") {
            if failed {
                EmptyState(.alert, "This inbox is not available", body: "The account may not accept files from you, or the link is wrong.", tone: .danger) {
                    FbButton("Start a new transfer", kind: .secondary, fill: false) { model.select(.send) }
                }
            } else if let policy = model.policy, !policy.enabledTransports.contains(.http) {
                EmptyState(.alert, "Stored transfers are unavailable on this instance.", tone: .warning)
            } else if let recipient {
                Panel {
                    VStack(spacing: Space.lg) {
                        Emblem()
                        Text("Send files to @\(recipient.username)").fbText(.headline).multilineTextAlignment(.center)
                        Text("Files are encrypted for their account before upload. Only they can open them.").fbText(.body, color: theme.textMuted).multilineTextAlignment(.center)
                        FbButton("Choose files", icon: .folder) {
                            model.pendingRecipient = recipient.username
                            model.select(.send)
                        }
                        FbButton("Recipient key fingerprint", kind: .quiet, trailingIcon: showFingerprint ? .chevronUp : .chevronDown) { withMotion(Motion.spatial) { showFingerprint.toggle() } }
                        if showFingerprint {
                            Text(recipient.fingerprint.chunked(4).joined(separator: " ")).fbText(.codeSmall, color: theme.textMuted).multilineTextAlignment(.center)
                                .transition(.opacity.combined(with: .move(edge: .top)))
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(Space.xl)
                }
            } else { CenteredSpinner() }
        }
        .task(id: username) {
            do { recipient = try await model.service.recipient(instance: model.instance, username: username) } catch { failed = true }
        }
    }
}

extension String {
    func chunked(_ size: Int) -> [String] {
        var result: [String] = []
        var current = ""
        for character in self {
            current.append(character)
            if current.count == size { result.append(current); current = "" }
        }
        if !current.isEmpty { result.append(current) }
        return result
    }
}
