import SwiftUI
import FilebeamDomain

private enum SendSheet: Identifiable {
    case password, retention, recipient, notePassword, noteRetention, language, attachedLanguage, pickFiles, pickFolder
    var id: Self { self }
}

struct SendView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var sheet: SendSheet?

    var body: some View {
        @Bindable var send = model.send
        Group {
            if case .sharing = model.send.stage {
                ShareView().transition(.asymmetric(insertion: .opacity.combined(with: .move(edge: .bottom)).combined(with: .scale(scale: 0.96)), removal: .opacity.combined(with: .scale(scale: 0.98))))
            } else {
                TabScreen {
                    SegmentedRail(segments: [Segment(value: SendMode.files, label: "Files", icon: .folder), Segment(value: SendMode.notes, label: "Notes", icon: .note)], selection: $send.mode)
                        .staggered(0)
                    if let policy = model.policy, !policy.anonymousUploads, model.session == nil {
                        SignInGate().staggered(1)
                    } else if model.send.mode == .files {
                        FilesComposer(open: { sheet = $0 })
                    } else {
                        NotesComposer(open: { sheet = $0 })
                    }
                }
                .transition(.opacity)
            }
        }
        .motion(Motion.enter, value: model.send.stage)
        .sheet(item: $sheet) { item in sheetContent(item) }
        .task(id: model.pendingRecipient) {
            guard let username = model.pendingRecipient else { return }
            model.pendingRecipient = nil
            await model.send.chooseRecipient(username)
        }
    }

    @ViewBuilder private func sheetContent(_ item: SendSheet) -> some View {
        let send = model.send
        switch item {
        case .password: PasswordSheet(current: send.filePassword) { send.filePassword = $0 }
        case .notePassword: PasswordSheet(current: send.notePassword) { send.notePassword = $0 }
        case .retention:
            RetentionSheet(options: model.policy?.fileRetentionChoices() ?? [], selected: send.files.options.retentionHours ?? model.policy?.fileDefaultRetentionHours ?? 24, live: send.files.options.transport == .webRTC) { send.files.options.retentionHours = $0 }
        case .noteRetention:
            RetentionSheet(options: model.policy?.noteRetentionChoices() ?? [], selected: send.note.options.retentionHours ?? model.policy?.noteDefaultRetentionHours ?? 720, live: send.note.options.live) { send.note.options.retentionHours = $0 }
        case .recipient: RecipientSheet()
        case .language: LanguageSheet(selected: send.note.language) { send.note.language = $0 }
        case .attachedLanguage: LanguageSheet(selected: send.files.options.attachedNote?.language ?? .plain) { send.files.options.attachedNote?.language = $0 }
        case .pickFiles: NativeDocumentPicker(allowsFolders: false) { urls in sheet = nil; model.send.addFiles(urls) }.ignoresSafeArea()
        case .pickFolder: NativeDocumentPicker(allowsFolders: true) { urls in sheet = nil; model.send.addFiles(urls) }.ignoresSafeArea()
        }
    }
}

private struct SignInGate: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    var body: some View {
        Panel {
            VStack(spacing: Space.lg) {
                Emblem()
                Text("Sign in to share files").fbText(.headline).multilineTextAlignment(.center)
                Text("This instance only accepts transfers from signed-in accounts.").fbText(.body, color: theme.textMuted).multilineTextAlignment(.center)
                FbButton("Sign in", icon: .login) { model.push(.auth(.login)) }
            }
            .frame(maxWidth: .infinity)
            .padding(Space.xxl)
        }
    }
}

/// The transport choice with the limits that apply to it.
struct TransportRail: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Binding var transport: Transport
    let showFileCount: Bool
    var liveCaption = "Keep the app open"

    var body: some View {
        let policy = model.policy
        let driver = policy?.driver(for: transport)
        VStack(alignment: .leading, spacing: Space.md) {
            SegmentedRail(segments: [
                Segment(value: Transport.http, label: "HTTP", customIcon: .httpServer, caption: "Download later", enabled: policy?.enabledTransports.contains(.http) ?? true),
                Segment(value: Transport.webRTC, label: "WebRTC", customIcon: .webrtcP2p, caption: liveCaption, enabled: policy?.enabledTransports.contains(.webRTC) ?? true),
            ], selection: $transport)
            HStack(spacing: Space.sm) {
                FbIcon(.storage, size: 18)
                let limit = showFileCount ? driver?.maximumTransferBytes : driver?.maximumNoteBytes
                Text(limit?.finiteValue.map { "\(formatBytes($0)) per \(showFileCount ? "transfer" : "note")" } ?? "Unlimited transfer size").fbText(.smallStrong).frame(maxWidth: .infinity, alignment: .leading)
                if showFileCount, let count = driver?.maximumFileCount.finiteValue {
                    Text(count == 1 ? "Up to 1 file" : "Up to \(count) files").fbText(.small, color: theme.textMuted)
                }
            }
        }
    }
}

private struct FilesComposer: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let open: (SendSheet) -> Void

    var body: some View {
        @Bindable var send = model.send
        let draft = send.files
        let blocker = send.fileBlocker()
        let live = draft.options.transport == .webRTC
        let signedIn = model.session != nil
        let canTurbo = !live && draft.options.recipient == nil

        TransportRail(transport: Binding(get: { send.files.options.transport }, set: { send.setTransport($0) }), showFileCount: true).staggered(1)

        if draft.sources.isEmpty {
            FilesHero(onFiles: { open(.pickFiles) }, onFolder: { open(.pickFolder) }).staggered(2)
        } else {
            let heading = dynamicTypeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: Space.md)) : AnyLayout(HStackLayout(alignment: .center))
            heading {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Your files").fbText(.title)
                    let known = draft.sources.allSatisfy { $0.sizeBytes != nil }
                    Text("\(draft.sources.count) ready to protect" + (known ? " · " + formatBytes(draft.sources.compactMap(\.sizeBytes).reduce(0, +)) : "")).fbText(.small, color: theme.textMuted)
                }
                Spacer()
                FbButton("Add more", kind: .secondary, icon: .plus, height: 44, fill: false) { open(.pickFiles) }
            }
            ForEach(draft.sources) { source in
                FileRow(source: source)
                    .transition(.asymmetric(insertion: .opacity.combined(with: .move(edge: .top)), removal: .opacity.combined(with: .scale(scale: 0.96))))
            }
            .motion(Motion.spatial, value: draft.sources)
            if let message = send.queueMessage { Notice(message, tone: .info) }
        }

        AttachedNoteSection(open: open).staggered(3)

        Panel {
            let friend = draft.options.recipient
            OptionRow(.key, "Password", value: friend != nil ? "Not used for friends" : send.filePassword != nil ? "Password set" : "Optional", enabled: friend == nil, highlighted: send.filePassword != nil) { open(.password) }
            Hairline()
            OptionRow(.clock, live ? "Link lifetime" : "Retained for", value: formatRetention(hours: draft.options.retentionHours ?? model.policy?.fileDefaultRetentionHours ?? 24)) { open(.retention) }
            if signedIn {
                Hairline()
                OptionRow(.users, "Send to friend", value: friend.map { "@\($0.username)" } ?? "Off", highlighted: friend != nil) { open(.recipient) }
            }
            if friend == nil {
                Hairline()
                SwitchRow("Include key in link", isOn: $send.files.options.includeKeyInLink, description: draft.options.includeKeyInLink ? nil : "Share the decryption key separately.", icon: .link).padding(.horizontal, Space.lg)
            }
            if draft.sources.count > 1 {
                Hairline()
                SwitchRow("Combine into a ZIP", isOn: $send.files.options.archive, description: "Recipients download one archive.", icon: .archive).padding(.horizontal, Space.lg)
            }
        }
        .motion(Motion.spatial, value: draft.sources.count > 1)
        .motion(Motion.spatial, value: draft.options.recipient == nil)
        .staggered(4)

        if let blocker, let message = blocker.message {
            Notice(message, tone: .danger) {
                if blocker.offersWebRTC { FbButton("Use WebRTC", kind: .secondary, height: 40, fill: false) { send.setTransport(.webRTC) } }
            }
            .transition(.opacity.combined(with: .move(edge: .top)))
        }

        VStack(spacing: Space.md) {
            FbButton(draft.options.recipient != nil ? "Send to inbox" : "Send encrypted", icon: .lock, enabled: blocker == nil) { Task { await send.sendFiles(turbo: false) } }
            if canTurbo {
                FbButton("Turbo Transfer", kind: .secondary, icon: .bolt, enabled: blocker == nil) { Task { await send.sendFiles(turbo: true) } }
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
            HStack(spacing: Space.sm) {
                FbIcon(.shield, size: 16)
                Text(canTurbo ? "Turbo shares the link while files are still uploading." : "Encrypted on this device before upload.").fbText(.caption, color: theme.textMuted).multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
        }
        .motion(Motion.spatial, value: canTurbo)
        .staggered(5)
    }
}

private struct FilesHero: View {
    @Environment(\.theme) private var theme
    let onFiles: () -> Void
    let onFolder: () -> Void
    @State private var pressed = false

    var body: some View {
        Panel {
            VStack(spacing: Space.lg) {
                Button(action: onFiles) { Emblem(engaged: pressed) }
                    .buttonStyle(.plain)
                    .simultaneousGesture(DragGesture(minimumDistance: 0).onChanged { _ in pressed = true }.onEnded { _ in pressed = false })
                    .accessibilityLabel("Choose files")
                (Text("Send your files ") + Text("privately").foregroundStyle(theme.accentText))
                    .fbText(.display)
                    .multilineTextAlignment(.center)
                Text("They are encrypted on this device before they leave it.").fbText(.body, color: theme.textMuted).multilineTextAlignment(.center)
                FbButton("Choose files", icon: .folder, trailingIcon: .arrowUp, action: onFiles)
                FbButton("Choose a folder", kind: .secondary, icon: .archive, action: onFolder)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Space.xl)
            .padding(.vertical, Space.xxl)
        }
    }
}

private struct FileRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let source: FileSource

    var body: some View {
        let failed: Bool = if case .importFailed = source.state { true } else { false }
        let detail: String = switch source.state {
        case .ready: source.sizeBytes.map(formatBytes) ?? "Size unknown"
        case .sizePending: "Size unknown"
        case .importing: "Preparing"
        case .importFailed: "This file can no longer be read. Remove it and choose it again."
        }
        ItemRow(name: source.name, detail: detail, error: failed) {
            if failed { IconAction(.refresh, "Retry \(source.name)") { model.send.retrySource(source) } }
            IconAction(.close, "Remove \(source.name)") { withMotion(Motion.spatial) { model.send.removeSource(source.id) } }
        }
    }
}

/// "Attach note" on the files composer: a switch that unfolds a note sharing the files' encryption and expiry.
private struct AttachedNoteSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let open: (SendSheet) -> Void

    var body: some View {
        @Bindable var send = model.send
        let attached = send.files.options.attachedNote != nil
        VStack(alignment: .leading, spacing: Space.md) {
            Panel {
                SwitchRow("Attach note", isOn: Binding(get: { send.files.options.attachedNote != nil }, set: { send.files.options.attachedNote = $0 ? AttachedNoteDraft() : nil }), description: "Send context alongside your files.", icon: .note)
                    .padding(.horizontal, Space.lg)
            }
            if attached {
                VStack(alignment: .leading, spacing: Space.sm) {
                    NoteFields(
                        title: Binding(get: { send.files.options.attachedNote?.title ?? "" }, set: { send.files.options.attachedNote?.title = $0 }),
                        text: Binding(get: { send.files.options.attachedNote?.text ?? "" }, set: { send.files.options.attachedNote?.text = $0 }),
                        language: send.files.options.attachedNote?.language ?? .plain,
                        wrap: .constant(true), showWrap: false,
                        maximumBytes: UInt64(attachedNoteMaximumBytes),
                        onPickLanguage: { open(.attachedLanguage) }
                    )
                    HStack(spacing: Space.sm) {
                        FbIcon(.lock, size: 14)
                        Text("Same encryption. Same expiry as the files.").fbText(.caption, color: theme.textMuted)
                    }
                }
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .motion(Motion.spatial, value: attached)
    }
}

/// The note surface shared by standalone and attached notes: a title, a language and wrap
/// bar, the editor, and a byte counter that turns red past `maximumBytes`.
struct NoteFields: View {
    @Environment(\.theme) private var theme
    @Binding var title: String
    @Binding var text: String
    let language: NoteLanguage
    @Binding var wrap: Bool
    var showWrap = true
    let maximumBytes: UInt64?
    let onPickLanguage: () -> Void

    var body: some View {
        let bytes = UInt64(text.utf8.count)
        let over = maximumBytes.map { bytes > $0 } ?? false
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: Space.md) {
                FbIcon(.note, size: 20)
                TextField("", text: Binding(get: { title }, set: { title = String($0.replacingOccurrences(of: "\n", with: " ").prefix(noteTitleMaximumCharacters)) }), prompt: Text("Untitled note").foregroundStyle(theme.textSubtle))
                    .fbText(.bodyStrong)
                    .tint(theme.accentText)
                    .autocorrectionDisabled()
                    .submitLabel(.next)
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, Space.md)
            Hairline()
            HStack(spacing: Space.md) {
                Button(action: onPickLanguage) {
                    HStack(spacing: Space.sm) {
                        Text(language.label).fbText(.smallStrong).lineLimit(1)
                        FbIcon(.chevronDown, size: 14)
                    }
                    .padding(.horizontal, Space.md)
                    .frame(height: 36)
                    .overlay(Shapes.control.strokeBorder(theme.controlBorder, lineWidth: 1))
                    .fixedSize()
                }
                .buttonStyle(.pressScale)
                .accessibilityLabel("Language, \(language.label)")
                Text("untitled.\(language.fileExtension)").fbText(.codeSmall, color: theme.textMuted).lineLimit(1).truncationMode(.middle).frame(maxWidth: .infinity, alignment: .leading)
                if showWrap {
                    Text("Wrap").fbText(.caption, color: theme.textMuted)
                    FbSwitch(isOn: $wrap)
                }
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, Space.sm)
            CodeEditor(text: $text, language: language.rawValue, wrap: wrap, placeholder: "Write or paste a private note")
                .padding(.horizontal, Space.sm)
            Text(maximumBytes.map { "\(formatBytes(bytes)) / \(formatBytes($0))" } ?? formatBytes(bytes))
                .fbText(.numeric, color: over ? theme.danger.ink : theme.textMuted)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.horizontal, Space.lg)
                .padding(.vertical, Space.sm)
                .motion(Motion.fade, value: over)
        }
        .background(theme.surface, in: Shapes.card)
        .overlay(Shapes.card.strokeBorder(theme.border, lineWidth: 1))
    }
}

private struct NotesComposer: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let open: (SendSheet) -> Void
    @State private var wrap = true

    var body: some View {
        @Bindable var send = model.send
        let note = send.note
        let live = note.options.live
        let blocker = send.noteBlocker()
        TransportRail(transport: Binding(get: { send.note.options.live ? .webRTC : .http }, set: { send.note.options.live = $0 == .webRTC }), showFileCount: false).staggered(1)
        NoteFields(title: $send.note.title, text: $send.note.text, language: note.language, wrap: $wrap, maximumBytes: model.policy?.driver(for: live ? .webRTC : .http)?.maximumNoteBytes.finiteValue, onPickLanguage: { open(.language) })
            .staggered(2)
        Panel {
            OptionRow(.key, "Password", value: send.notePassword != nil ? "Password set" : "Optional", highlighted: send.notePassword != nil) { open(.notePassword) }
            Hairline()
            OptionRow(.clock, live ? "Link lifetime" : "Retained for", value: formatRetention(hours: note.options.retentionHours ?? model.policy?.noteDefaultRetentionHours ?? 720)) { open(.noteRetention) }
            Hairline()
            SwitchRow("Include key in link", isOn: $send.note.options.includeKeyInLink, description: note.options.includeKeyInLink ? nil : "Share the decryption key separately.", icon: .link).padding(.horizontal, Space.lg)
            Hairline()
            SwitchRow("Burn on read", isOn: $send.note.options.burnOnRead, description: live ? "Revoked after the first successful decrypt." : "Removed after the first successful decrypt.", icon: .trash).padding(.horizontal, Space.lg)
        }
        .staggered(3)
        if let message = blocker?.message { Notice(message, tone: .danger).transition(.opacity) }
        VStack(spacing: Space.md) {
            FbButton("Send encrypted note", icon: .lock, enabled: blocker == nil, loading: send.noteBusy) { Task { await send.sendNote() } }
            HStack(spacing: Space.sm) {
                FbIcon(.shield, size: 16)
                Text("Encrypted on this device before upload.").fbText(.caption, color: theme.textMuted)
            }
            .frame(maxWidth: .infinity)
        }
        .staggered(4)
    }
}

/// Picks an accepted friend, or resolves an exact username, as the inbox recipient.
struct RecipientSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var username = ""
    @State private var friends: [Contact]?
    @State private var awaiting = false

    var body: some View {
        let send = model.send
        let selected = send.files.options.recipient
        FbSheet(title: "Send to a friend", subtitle: "Files go to their encrypted inbox. Stored transfers only, without a password.") {
            if let selected {
                Notice("Files will go to @\(selected.username)'s inbox.", tone: .accent, icon: .users) {
                    FbButton("Clear", kind: .quiet, height: 40, fill: false) { send.clearRecipient() }
                }
            }
            FbTextField($username, label: "Username", placeholder: "@username", error: { if case let .failed(message) = send.recipientLookup { message.isEmpty ? "No inbox was found for that username." : message } else { nil } }(), icon: .user, submitLabel: .search, onSubmit: choose) {
                if send.recipientLookup == .searching { Spinner(size: 20).padding(.trailing, Space.sm) }
            }
            FbButton("Use this username", enabled: username.trimmingCharacters(in: CharacterSet(charactersIn: "@ ")).count >= 3 && send.recipientLookup != .searching) { choose() }
            if let friends {
                if friends.isEmpty {
                    Text("Friends you add appear here for quick sending.").fbText(.small, color: theme.textMuted)
                } else {
                    VStack(alignment: .leading, spacing: Space.sm) {
                        Text("Friends").fbText(.eyebrow, color: theme.textMuted)
                        ForEach(friends) { friend in
                            let chosen = selected?.username == friend.username
                            Button {
                                awaiting = true
                                Task { await send.chooseRecipient(friend.username) }
                            } label: {
                                HStack(spacing: Space.md) {
                                    Avatar(name: friend.username)
                                    Text("@\(friend.username)").fbText(.bodyStrong).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                                    FbIcon(.chevronRight, size: 16)
                                }
                                .padding(Space.md)
                                .background(chosen ? theme.selectedSurface : Color.clear, in: Shapes.control)
                                .overlay(Shapes.control.strokeBorder(chosen ? theme.choiceBorder : theme.border, lineWidth: 1))
                            }
                            .buttonStyle(.pressScale(0.985))
                        }
                    }
                }
            } else { CenteredSpinner() }
        }
        .task { friends = (try? await model.service.contacts(instance: model.instance).contacts.filter { $0.status == "accepted" }) ?? [] }
        .onChange(of: send.files.options.recipient) { _, recipient in
            // Close once a lookup lands on a recipient chosen from this sheet.
            if awaiting, recipient != nil, send.recipientLookup == .idle { dismiss() }
        }
    }

    private func choose() {
        awaiting = true
        Task { await model.send.chooseRecipient(username) }
    }
}
