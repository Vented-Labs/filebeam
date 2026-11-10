import SwiftUI
import FilebeamDomain

struct ReceiveView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var reporting = false
    @State private var exporting: [URL]?
    @State private var previewing: URL?

    var body: some View {
        let receive = model.receive
        let stage = receive.stage
        TabScreen {
            VStack(alignment: .leading, spacing: Space.lg) {
                switch stage {
                case .idle: LinkEntry()
                case .inspecting: WorkingPanel("Loading encrypted transfer", body: "Asking the instance what this link points to.")
                case let .unlocking(phase): WorkingPanel(phase.label, body: "Checking the key and reading the encrypted file list.")
                case let .needKey(error): KeyStep(error: error)
                case let .needPassword(error): PasswordStep(error: error)
                case let .needConsent(peer):
                    StepPanel(customIcon: .webrtcP2p, title: "Connect directly?", body: "Live transfers connect your device to the sender. Each side may see the other's network address.") {
                        if let peer { Text(peer).fbText(.codeSmall, color: theme.textMuted) }
                        Notice("Turn on relay-only live transfers in Settings to hide your address.", tone: .info)
                        FbButton("Allow and connect") { Task { await receive.submitConsent(true) } }
                        FbButton("Not now", kind: .quiet) { Task { await receive.submitConsent(false) } }
                    }
                case let .noteLocked(burnOnRead):
                    StepPanel(.note, title: "Secure note", body: "The note is decrypted on this device when you open it.") {
                        if burnOnRead { Notice("This note burns after reading. Decrypting it removes it for everyone.", tone: .warning, icon: .trash) }
                        FbButton("Decrypt note", icon: .unlock) { Task { await receive.decryptNote() } }
                        FbButton("Cancel", kind: .quiet) { receive.reset() }
                    }
                case let .files(items): FilesReady(items: items)
                case let .downloading(items, selection): DownloadProgress(count: selection?.count ?? items.count)
                case let .saved(files): SavedFiles(files: files, onSave: { exporting = [$0] }, onSaveAll: { exporting = files }, onPreview: { previewing = $0 })
                case let .note(note, burnPending): NoteResult(note: note, burnPending: burnPending)
                case let .unavailable(message): Unavailable(message: message)
                }
                if stage.showsFooterActions {
                    if case .files = stage, !receive.input.isEmpty {
                        CliCard(link: receive.input, available: !receive.live && receive.input.contains("#"), reason: receive.live ? "Live transfers are received in the app or the browser." : "This transfer is not currently available for CLI download.")
                    }
                    HStack(spacing: Space.md) {
                        FbButton("Open another", kind: .secondary, icon: .link) { receive.reset() }
                        FbButton("Report", kind: .quiet, icon: .alert) { reporting = true }
                    }
                }
            }
            .id(stage.kind)
            .transition(.asymmetric(insertion: .opacity.combined(with: .move(edge: .bottom)).combined(with: .scale(scale: 0.98)), removal: .opacity))
        }
        .motion(Motion.enter, value: stage.kind)
        .privacySensitive(stage.isSensitive)
        .sheet(isPresented: $reporting, onDismiss: { receive.clearReport() }) { ReportSheet() }
        .sheet(item: Binding(get: { exporting.map(ExportItems.init) }, set: { if $0 == nil { exporting = nil } })) { item in
            DocumentExportSheet(urls: item.urls) { result in
                exporting = nil
                if case let .success(destinations) = result, let jobID = receive.jobID {
                    Task { await model.recordExport(jobID: jobID, paths: destinations.map(\.path)); model.toast("Saved", tone: .success) }
                } else if case let .failure(error) = result { model.fail(error) }
            }
            .ignoresSafeArea()
        }
        .sheet(item: Binding(get: { previewing.map(ExportItems.init) }, set: { if $0 == nil { previewing = nil } })) { item in
            NativePreviewSheet(url: item.urls[0]).ignoresSafeArea()
        }
        .task(id: model.pendingReceiveLink) {
            guard let link = model.pendingReceiveLink else { return }
            model.pendingReceiveLink = nil
            receive.open(link)
        }
    }
}

private struct ExportItems: Identifiable {
    let urls: [URL]
    init(_ urls: [URL]) { self.urls = urls }
    init(_ url: URL) { urls = [url] }
    var id: String { urls.map(\.path).joined() }
}

private extension ReceiveStage {
    var showsFooterActions: Bool {
        switch self { case .files, .saved, .note, .noteLocked: true; default: false }
    }
    var isSensitive: Bool {
        switch self { case .note, .needKey, .needPassword: true; default: false }
    }
}

private struct LinkEntry: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    var body: some View {
        @Bindable var receive = model.receive
        ScreenHeader("Receive a transfer", subtitle: "Paste a Filebeam link or ID. Files are decrypted on this device.").staggered(0)
        Panel {
            VStack(spacing: Space.lg) {
                FbTextField($receive.input, label: "Transfer link or ID", placeholder: "https://…", icon: .link, keyboard: .URL, submitLabel: .go, onSubmit: { receive.open() }, identifier: "receive-input")
                FbButton("Open transfer", icon: .unlock, enabled: !receive.input.isBlank) { hideKeyboard(); receive.open() }
                FbButton("Paste from clipboard", kind: .secondary, icon: .paste) {
                    if let clip = Clipboard.paste(), !clip.isBlank { receive.input = clip; receive.open(clip) } else { model.toast("The clipboard is empty.") }
                }
            }
            .padding(Space.lg)
        }
        .staggered(1)
        HStack(spacing: Space.sm) {
            FbIcon(.shield, size: 16)
            Text("Decrypted on this device, never on the server.").fbText(.caption, color: theme.textMuted)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, Space.sm)
        .staggered(2)
    }
}

private struct KeyStep: View {
    @Environment(AppModel.self) private var model
    let error: Bool
    @State private var key = ""
    var body: some View {
        StepPanel(.key, title: "Enter the decryption key", body: "This link does not include its key. The sender shared it separately.") {
            FbTextField($key, label: "Generated key", placeholder: "v1.…", error: error ? "Invalid decryption key. Enter the key the sender shared." : nil, monospaced: true, keyboard: .asciiCapable, submitLabel: .go, onSubmit: { Task { await model.receive.submitKey(key) } })
            FbButton("Continue", enabled: !key.isBlank) { Task { await model.receive.submitKey(key) } }
            FbButton("Cancel", kind: .quiet) { model.receive.reset() }
        }
    }
}

private struct PasswordStep: View {
    @Environment(AppModel.self) private var model
    let error: Bool
    @State private var password = ""
    var body: some View {
        StepPanel(.lock, title: "Password required", body: "The sender protected this transfer with a password.") {
            PasswordField($password, label: "Password", error: error ? "That password did not unlock the transfer." : nil, submitLabel: .go, onSubmit: { Task { await model.receive.submitPassword(password) } })
            FbButton("Unlock", icon: .unlock, enabled: !password.isEmpty) { Task { await model.receive.submitPassword(password) } }
            HStack(spacing: Space.md) {
                FbButton("Change key", kind: .quiet) { model.receive.changeKey() }
                FbButton("Cancel", kind: .quiet) { model.receive.reset() }
            }
        }
    }
}

private struct FilesReady: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let items: [TransferSelectionItem]
    var body: some View {
        let receive = model.receive
        let total = items.reduce(0) { $0 + $1.size }
        ScreenHeader(items.count == 1 ? "1 file ready to download" : "\(items.count) files ready to download", subtitle: formatBytes(total) + (receive.live ? " · Live transfer: the sender must stay connected" : ""))
        if let note = receive.attachedNote { AttachedNoteCard(note: note) }
        ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
            ItemRow(name: item.name, detail: formatBytes(item.size)) {
                if items.count > 1 { IconAction(.download, "Download \(item.name)", tint: theme.accentText) { Task { await receive.download([item.id]) } } }
            }
            .staggered(min(index, 8))
        }
        FbButton(items.count > 1 ? "Download all" : "Download", icon: .download) { Task { await receive.download(nil) } }
        Text("Your download progress is shared anonymously with the sender.").fbText(.caption, color: theme.textSubtle).multilineTextAlignment(.center).frame(maxWidth: .infinity)
    }
}

private struct DownloadProgress: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let count: Int
    var body: some View {
        let receive = model.receive
        let snapshot = receive.snapshot
        Panel {
            VStack(spacing: Space.lg) {
                Emblem(engaged: true, size: 108)
                Text((snapshot?.phase ?? .receiving).label).fbText(.headline).multilineTextAlignment(.center).id(snapshot?.phase).transition(.opacity)
                Text(count == 1 ? "Downloading 1 file" : "Downloading \(count) files").fbText(.small, color: theme.textMuted)
                HStack {
                    Spinner(size: 16)
                    Text(receive.live ? "Live transfer: the sender must stay connected" : "Every file is verified before it is saved").fbText(.caption, color: theme.textMuted).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading).padding(.leading, Space.sm)
                    if let fraction = snapshot?.fraction { Text(progressLabel(fraction)).fbText(.numeric, color: theme.textMuted) }
                }
                SmoothProgress(snapshot?.fraction)
                FbButton("Cancel", kind: .danger) { Task { await receive.cancelDownload() } }
            }
            .frame(maxWidth: .infinity)
            .padding(Space.xl)
        }
    }
}

private struct SavedFiles: View {
    @Environment(AppModel.self) private var model
    let files: [URL]
    let onSave: (URL) -> Void
    let onSaveAll: () -> Void
    let onPreview: (URL) -> Void
    var body: some View {
        ScreenHeader("Download completed", subtitle: "Verified and kept privately in Filebeam until you save it.")
        if let note = model.receive.attachedNote { AttachedNoteCard(note: note) }
        ForEach(Array(files.enumerated()), id: \.element) { index, file in
            Button { onPreview(file) } label: {
                ItemRow(name: file.lastPathComponent, detail: formatBytes(UInt64(max(0, (try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)))) {
                    FbButton("Save", kind: .secondary, icon: .download, height: 40, fill: false) { onSave(file) }
                }
            }
            .buttonStyle(.plain)
            .accessibilityHint("Preview")
            .staggered(min(index, 8))
        }
        if files.isEmpty { Notice("The verified files are no longer available on this device.", tone: .warning) }
        else if files.count > 1 { FbButton("Save all to Files", icon: .download, action: onSaveAll) }
        StatusPill("Integrity verified", tone: .success, icon: .shield)
    }
}

private struct NoteResult: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let note: VerifiedNote
    let burnPending: Bool
    @State private var exporting: URL?

    var body: some View {
        ScreenHeader(note.title?.nilIfEmpty ?? "Secure note")
        if note.consumed { Notice("This note was removed after this read.", tone: .warning, icon: .trash) }
        if burnPending {
            Notice("The note was read but could not be removed yet.", tone: .danger) {
                FbButton("Retry removal", kind: .secondary, height: 40, fill: false) { Task { await model.receive.retryBurn() } }
            }
        }
        CodeEditor(note.text, language: note.language.rawValue, minHeight: 120, maxHeight: 520)
        HStack(spacing: Space.md) {
            FbButton("Copy", icon: .copy) {
                Clipboard.copySensitive(note.text)
                model.toast("Copied", tone: .success)
            }
            FbButton("Save", kind: .secondary, icon: .download) {
                let name = ((note.title?.nilIfEmpty ?? "note").replacingOccurrences(of: "/", with: "-")) + "." + note.language.fileExtension
                let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true).appendingPathComponent(name)
                do {
                    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
                    try Data(note.text.utf8).write(to: url, options: [.atomic, .completeFileProtection])
                    exporting = url
                } catch { model.fail(error) }
            }
        }
        .sheet(item: Binding(get: { exporting.map(ExportItems.init) }, set: { if $0 == nil { exporting = nil } })) { item in
            DocumentExportSheet(urls: item.urls) { result in
                try? FileManager.default.removeItem(at: item.urls[0].deletingLastPathComponent())
                exporting = nil
                if case .success = result { model.toast("Saved", tone: .success) }
            }
            .ignoresSafeArea()
        }
    }
}

private struct Unavailable: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let message: String?
    var body: some View {
        Panel {
            VStack(spacing: Space.lg) {
                FbIcon(.alert, size: 30, tint: theme.danger.ink)
                    .frame(width: 72, height: 72)
                    .background(theme.danger.surface, in: Shapes.panel)
                    .overlay(Shapes.panel.strokeBorder(theme.danger.border.opacity(0.5), lineWidth: 1))
                Text("Transfer unavailable").fbText(.headline).multilineTextAlignment(.center)
                Text(message.map(readableMessage) ?? "It may have expired, been deleted, or the link may be incomplete.").fbText(.small, color: theme.textMuted).multilineTextAlignment(.center)
                FbButton("Try another link", kind: .secondary, icon: .link) { model.receive.reset() }
                FbButton("Start a new transfer", icon: .upload) { model.receive.reset(); model.select(.send) }
            }
            .frame(maxWidth: .infinity)
            .padding(Space.xxl)
        }
    }
}
