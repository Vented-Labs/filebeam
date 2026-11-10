import SwiftUI
import FilebeamDomain
import FilebeamTheme

/// Sets, generates or removes the optional transfer password.
struct PasswordSheet: View {
    @Environment(\.dismiss) private var dismiss
    let current: String?
    let onChange: (String?) -> Void
    @State private var value: String

    init(current: String?, onChange: @escaping (String?) -> Void) {
        self.current = current; self.onChange = onChange
        _value = State(initialValue: current ?? "")
    }

    var body: some View {
        let tooShort = !value.isEmpty && value.count < passwordMinimumLength
        FbSheet(title: "Transfer password", subtitle: "Recipients need this as well as the link. It never leaves this device.") {
            PasswordField($value, placeholder: "At least 8 characters", error: tooShort ? "Use at least 8 characters." : nil, contentType: .newPassword, onSubmit: { if !tooShort { onChange(value.nilIfEmpty); dismiss() } })
            FbButton("Generate secure password", kind: .secondary, icon: .bolt) { value = generatePassword() }
            HStack(spacing: Space.md) {
                if current != nil { FbButton("Remove", kind: .danger) { onChange(nil); dismiss() } }
                FbButton("Done", enabled: !tooShort) { onChange(value.nilIfEmpty); dismiss() }
            }
        }
        .privacySensitive()
    }
}

struct RetentionSheet: View {
    @Environment(\.dismiss) private var dismiss
    let options: [UInt64]
    let selected: UInt64
    let live: Bool
    let onSelect: (UInt64) -> Void

    var body: some View {
        FbSheet(title: live ? "Link lifetime" : "Retained for", subtitle: live ? "The server may shorten this lifetime." : "How long the encrypted transfer stays available.") {
            VStack(spacing: Space.sm) {
                ForEach(Array(Set(options + [selected])).sorted(), id: \.self) { hours in
                    ChoiceRow(formatRetention(hours: hours), selected: hours == selected) { onSelect(hours); dismiss() }
                }
            }
        }
    }
}

struct LanguageSheet: View {
    @Environment(\.dismiss) private var dismiss
    let selected: NoteLanguage
    let onSelect: (NoteLanguage) -> Void

    var body: some View {
        FbSheet(title: "Language") {
            VStack(spacing: Space.sm) {
                ForEach(NoteLanguage.allCases, id: \.self) { language in
                    let key = NoteLanguageKey(rawValue: language.rawValue) ?? .plain
                    ChoiceRow(key.label, selected: language == selected, description: "untitled.\(key.fileExtension)") { onSelect(language); dismiss() }
                }
            }
        }
    }
}

extension NoteLanguage {
    var label: String { (NoteLanguageKey(rawValue: rawValue) ?? .plain).label }
    var fileExtension: String { (NoteLanguageKey(rawValue: rawValue) ?? .plain).fileExtension }
}

/// Answers a prompt from a transfer no feature screen is driving, e.g. a resumed download.
struct PromptSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    @Bindable var model: AppModel
    let context: PromptContext
    @State private var value = ""
    @State private var selected = Set<String>()
    @State private var submitting = false

    var body: some View {
        FbSheet(title: title, subtitle: context.prompt.message ?? detail) {
            switch context.prompt.kind {
            case .filesSelection:
                VStack(spacing: Space.sm) {
                    ForEach(context.prompt.selectionItems) { item in
                        let on = selected.contains(item.id)
                        Button {
                            Haptics.tick()
                            if on { selected.remove(item.id) } else { selected.insert(item.id) }
                        } label: {
                            ItemRow(name: item.name, detail: formatBytes(item.size)) {
                                FbIcon(on ? .check : .plus, size: 20, tint: on ? theme.accentText : theme.textMuted).frame(width: 44, height: 44)
                            }
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(on ? [.isSelected] : [])
                    }
                }
            case .directoryChoice:
                VStack(spacing: Space.sm) {
                    ChoiceRow("Individual files", selected: value == "individual", description: "Recipients choose what to download.") { value = "individual" }
                    ChoiceRow("Combine into a ZIP", selected: value == "zip", description: "Recipients download one archive.") { value = "zip" }
                }
            case .peerConsent:
                Notice("Turn on relay-only live transfers in Settings to hide your address.", tone: .info)
            case .shareReady:
                Text("The encrypted share is ready.").fbText(.small, color: theme.textMuted)
            case .password:
                PasswordField($value, label: "Password", onSubmit: submit)
            case .decryptionKey:
                FbTextField($value, label: "Generated key", placeholder: "v1.…", monospaced: true, keyboard: .asciiCapable, submitLabel: .go, onSubmit: submit)
            }
            HStack(spacing: Space.md) {
                FbButton(context.prompt.kind == .peerConsent ? "Not now" : "Cancel", kind: .quiet) {
                    Task {
                        if context.prompt.kind == .peerConsent { _ = await model.respond(context: context, allowed: false) } else { await model.dismissPrompt(context) }
                        dismiss()
                    }
                }
                FbButton(context.prompt.kind == .peerConsent ? "Allow and connect" : "Continue", enabled: canSubmit, loading: submitting) { submit() }
            }
        }
        .privacySensitive()
        .onAppear { if context.prompt.kind == .filesSelection { selected = Set(context.prompt.selectionItems.map(\.id)) } }
        .interactiveDismissDisabled()
    }

    private var canSubmit: Bool {
        switch context.prompt.kind {
        case .filesSelection: !selected.isEmpty
        case .directoryChoice, .password, .decryptionKey: !value.isEmpty
        default: true
        }
    }

    private var title: String {
        switch context.prompt.kind {
        case .decryptionKey: "Enter the decryption key"
        case .password: "Password required"
        case .peerConsent: "Connect directly?"
        case .directoryChoice: "Package the folder"
        case .filesSelection: "Choose files"
        case .shareReady: "Share ready"
        }
    }

    private var detail: String {
        switch context.prompt.kind {
        case .peerConsent: "Live transfers connect your device to the other side. Each side may see the other's network address."
        case .filesSelection: "Choose the files to download."
        case .directoryChoice: "How recipients receive the folder."
        default: "This value is used only to unlock this transfer."
        }
    }

    private func submit() {
        guard canSubmit else { return }
        let response: String
        if context.prompt.kind == .filesSelection {
            response = String(data: (try? JSONEncoder().encode(Array(selected).sorted())) ?? Data("[]".utf8), encoding: .utf8) ?? "[]"
        } else { response = value.isEmpty ? "continue" : value }
        submitting = true
        Task {
            let ok = context.prompt.kind == .peerConsent ? await model.respond(context: context, allowed: true) : await model.respond(context: context, value: response)
            submitting = false
            if ok { value = ""; dismiss() }
        }
    }
}

/// The retained share link and separate key for a transfer sent from this device.
struct ShareReceiptSheet: View {
    @Environment(\.theme) private var theme
    @Environment(AppModel.self) private var model
    let receipt: ShareReceipt
    @State private var showKey = false

    var body: some View {
        FbSheet(title: "Share link", subtitle: receipt.separateKey == nil ? "The key stays in the link fragment and is never sent to the server." : "Share the generated key separately.") {
            CopyField(icon: .link, value: receipt.link, label: "Copy link", primary: true) {
                Clipboard.copySensitive(receipt.link)
                model.toast("Link copied", tone: .success)
            }
            if let key = receipt.separateKey {
                CopyField(icon: .key, value: showKey ? key : String(repeating: "•", count: 24), label: "Copy key") {
                    Clipboard.copySensitive(key)
                    model.toast("Key copied", tone: .success)
                }
                FbButton(showKey ? "Hide key" : "Show key", kind: .quiet, icon: showKey ? .eyeOff : .eye) { showKey.toggle() }
            }
            ShareLink(item: receipt.link) {
                HStack(spacing: Space.sm) { FbIcon(.arrowUpRight, size: 20, tint: theme.text); Text("Share link").fbText(.label) }
                    .frame(maxWidth: .infinity, minHeight: Space.control)
                    .background(theme.surface, in: Shapes.control)
                    .overlay(Shapes.control.strokeBorder(theme.controlBorder, lineWidth: 1))
            }
            .buttonStyle(.pressScale)
            if let expires = receipt.expiresAt { ExpiryChip(expiresAt: expires).frame(maxWidth: .infinity) }
        }
        .privacySensitive()
    }
}

struct DownloadCommandSheet: View {
    @Environment(AppModel.self) private var model
    let command: String
    var body: some View {
        FbSheet(title: "Download with CLI", subtitle: "Run this command in a terminal.") {
            CopyField(icon: .terminal, value: command, label: "Copy command", primary: true) {
                Clipboard.copySensitive(command)
                model.toast("Command copied", tone: .success)
            }
            CodeEditor(command, language: "plain", minHeight: 72, maxHeight: 160)
        }
        .privacySensitive()
    }
}

/// Asks for the account password when the receiving key is password-protected; a blank value uses a key held on this device.
struct UnlockSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let title: String
    let body_: String
    let onUnlock: (String?) async throws -> Void
    @State private var password = ""
    @State private var busy = false
    @State private var error: String?

    init(title: String, body: String, onUnlock: @escaping (String?) async throws -> Void) { self.title = title; body_ = body; self.onUnlock = onUnlock }

    var body: some View {
        FbSheet(title: title, subtitle: body_) {
            PasswordField($password, label: "Key password", supporting: "Leave empty if this device already holds your key.", error: error, onSubmit: unlock)
            FbButton("Unlock", icon: .unlock, loading: busy) { unlock() }
            Text("For a key you keep yourself, import it under Secure inbox first.").fbText(.caption, color: theme.textMuted)
        }
        .privacySensitive()
        .interactiveDismissDisabled(busy)
    }

    private func unlock() {
        busy = true
        error = nil
        Task {
            do { try await onUnlock(password.nilIfEmpty); dismiss() } catch { self.error = readableMessage(error) }
            busy = false
        }
    }
}
