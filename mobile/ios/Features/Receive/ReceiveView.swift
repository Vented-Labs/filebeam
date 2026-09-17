import SwiftUI
import FilebeamDomain

struct ReceiveView: View {
    @Bindable var model: AppModel
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text("Receive a transfer").font(.title2.bold())
                    Text("Paste the complete link, including its key. A separately shared key or password can be entered when requested.").foregroundStyle(.secondary)
                    TextField("Transfer link or ID", text: $model.receiveInput, axis: .vertical).textInputAutocapitalization(.never).autocorrectionDisabled().textContentType(.URL).accessibilityIdentifier("receive-input").onChange(of: model.receiveInput) { _, _ in model.receiveError = nil }
                    PasteButton(payloadType: String.self) { strings in if let string = strings.first { model.receiveInput = string } }.buttonStyle(.bordered)
                    if let error = model.receiveError { InlineNotice(text: error) }
                    if let job = model.receiveJob { ReceiveProgress(snapshot: job) }
                    PrimaryActionButton(title: "Download and verify", disabled: model.receiveInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) { Task { await model.inspectAndReceive() } }
                }.frame(maxWidth: 640).padding()
            }.navigationTitle("Receive")
        }
        .sheet(isPresented: Binding(get: { model.pendingNoteInspection != nil }, set: { if !$0 { model.pendingNoteInspection = nil; model.pendingNoteInput = nil; model.pendingNoteInstance = nil } })) {
            if let inspection = model.pendingNoteInspection { NoteReceiveSheet(model: model, inspection: inspection) }
        }
        .sheet(item: $model.pendingReceiveRoute) { route in ReceiveConfirmationSheet(model: model, route: route) }
    }
}

struct ReceiveConfirmationSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    let route: ReceiveRouteConfirmation
    var body: some View { NavigationStack { VStack(alignment: .leading, spacing: 16) { Text(route.isForeignInstance ? "Transfer from another instance" : "Ready to receive").font(.title2.bold()); Text(route.isForeignInstance ? "This link uses \(route.instance.origin). Your default instance will not change." : "The selected instance is \(route.instance.origin).").foregroundStyle(.secondary); Label(route.inspection.kind == .note ? "Encrypted note" : "Encrypted files", systemImage: route.inspection.kind == .note ? "note.text" : "doc"); Text(route.input).font(.footnote.monospaced()).textSelection(.enabled); Spacer() }.padding().navigationTitle("Receive transfer").toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { model.pendingReceiveRoute = nil; dismiss() } }; ToolbarItem(placement: .confirmationAction) { Button("Download and verify") { Task { await model.beginConfirmedReceive(); if model.pendingReceiveRoute == nil { dismiss() } } } } } } }
}

struct NoteReceiveSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    let inspection: NoteInspection
    @State private var key = ""
    @State private var password = ""
    @State private var burnAcknowledged = false
    var body: some View {
        NavigationStack {
            Form {
                Section("Encrypted note") {
                    LabeledContent("Delivery", value: inspection.transport == .http ? "HTTP" : "WebRTC")
                    Text("The note content remains unavailable until it is decrypted.").foregroundStyle(.secondary)
                }
                Section("Unlock") {
                    SecureField("Decryption key (if shared separately)", text: $key).textInputAutocapitalization(.never).autocorrectionDisabled()
                    if inspection.passwordRequired { SecureField("Transfer password", text: $password) }
                }
                if inspection.burnOnRead { Section("One-time access") { Toggle("I understand opening this note may consume it", isOn: $burnAcknowledged); Text("This does not prevent a recipient keeping a copy.").font(.footnote).foregroundStyle(.secondary) } }
                if let error = model.receiveError { InlineNotice(text: error) }
            }.navigationTitle("Receive note").toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { key = ""; password = ""; dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Download and verify") { Task { await model.receiveNote(separateKey: key.nilIfEmpty, password: password.nilIfEmpty, burnAcknowledged: burnAcknowledged); key = ""; password = ""; if model.pendingNoteInspection == nil { dismiss() } } }.disabled(inspection.burnOnRead && !burnAcknowledged) }
            }
        }
    }
}

struct ReceiveProgress: View {
    let snapshot: TransferSnapshot
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(phaseTitle(snapshot.phase)).font(.headline)
            if let total = snapshot.totalBytes, total > 0 { ProgressView(value: Double(snapshot.completedBytes), total: Double(total)); Text("\(snapshot.completedBytes.filebeamBytes) of \(total.filebeamBytes)").font(.footnote).foregroundStyle(.secondary) }
            if snapshot.phase == .verifiedAwaitingExport { Label("Verified on this device", systemImage: "checkmark.seal.fill").foregroundStyle(.green) }
        }
        .padding()
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Transfer status")
        .accessibilityValue(accessibilityValue)
    }
    private func phaseTitle(_ phase: TransferPhase) -> String { phase.rawValue.replacingOccurrences(of: "([A-Z])", with: " $1", options: .regularExpression).capitalized }
    private var accessibilityValue: String {
        guard let total = snapshot.totalBytes, total > 0 else { return phaseTitle(snapshot.phase) }
        let percent = Int((Double(snapshot.completedBytes) / Double(total) * 100).rounded())
        let milestone = min(100, (percent / 25) * 25)
        return "\(phaseTitle(snapshot.phase)), \(milestone) percent"
    }
}

struct VerifiedNoteView: View {
    let note: VerifiedNote
    @State private var copied = false
    @State private var share = false
    @State private var exportDocument: ExportDocument?
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Label("Verified on this device", systemImage: "checkmark.seal.fill").foregroundStyle(.green)
                if let title = note.title, !title.isEmpty { Text(title).font(.title2.bold()) }
                Text(note.text).font(note.language == .plain ? .body : .system(.body, design: .monospaced)).textSelection(.enabled)
                HStack {
                    Button(copied ? "Copied" : "Copy note") { UIPasteboard.general.string = note.text; copied = true; Task { try? await Task.sleep(for: .seconds(2)); copied = false } }.frame(minWidth: 100, alignment: .leading)
                    Button("Share note") { share = true }
                    Button("Save as note file") { makeExportFile() }
                }
                Text("Choose a folder in Files to save an optional note copy. Sharing does not change its verified state.").font(.footnote).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
        }
        .sheet(isPresented: $share) { NativeShareSheet(items: [note.text]) }
        .sheet(item: $exportDocument) { document in DocumentExportSheet(urls: [document.url]) { _ in exportDocument = nil } }
        .navigationTitle("Encrypted note")
    }
    private func makeExportFile() {
        let filename = (note.title?.nilIfEmpty ?? "filebeam-note").replacingOccurrences(of: "/", with: "-")
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(filename).txt")
        try? Data(note.text.utf8).write(to: url, options: .atomic)
        exportDocument = ExportDocument(url: url)
    }
}

struct PromptSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    let context: PromptContext
    @State private var value = ""
    @State private var showValue = false
    @State private var selectedItemIDs = Set<String>()
    @State private var choosingDirectory = false
    @State private var submitting = false
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(context.prompt.message ?? detail).foregroundStyle(.secondary)
                if context.prompt.kind == .filesSelection { List(context.prompt.selectionItems) { item in Toggle(isOn: Binding(get: { selectedItemIDs.contains(item.id) }, set: { selected in if selected { selectedItemIDs.insert(item.id) } else { selectedItemIDs.remove(item.id) } })) { VStack(alignment: .leading) { Text(item.name); Text(item.size.filebeamBytes).font(.footnote).foregroundStyle(.secondary) } } }.listStyle(.plain) }
                else if context.prompt.kind == .directoryChoice { Button(value.isEmpty ? "Choose folder" : URL(fileURLWithPath: value).lastPathComponent) { choosingDirectory = true }.buttonStyle(.bordered); if !value.isEmpty { Text(value).font(.footnote.monospaced()).textSelection(.enabled) } }
                else if context.prompt.kind == .peerConsent { Text("Allow a direct peer connection for this transfer?").font(.body) }
                else if context.prompt.kind == .shareReady { Text("The encrypted share is ready. Continue to return to the transfer.").foregroundStyle(.secondary) }
                else { Group { if showValue { TextField(title, text: $value) } else { SecureField(title, text: $value) } }.textInputAutocapitalization(.never).autocorrectionDisabled(); Button(showValue ? "Hide" : "Show") { showValue.toggle() }.buttonStyle(.bordered) }
                Spacer()
            }.padding().navigationTitle(title).toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(context.prompt.kind == .peerConsent ? "Deny" : "Cancel") { if context.prompt.kind == .peerConsent { Task { if await model.respond(context: context, allowed: false) { dismiss() } } } else { Task { await model.dismissPrompt(context); dismiss() } } }.disabled(submitting) }
                ToolbarItem(placement: .confirmationAction) { Button(context.prompt.kind == .peerConsent ? "Allow" : "Continue") { submit() }.disabled(submitting || requiresValue && value.isEmpty || (context.prompt.kind == .filesSelection && selectedItemIDs.isEmpty)) }
            }
        }
        .sheet(isPresented: $choosingDirectory) { NativeDocumentPicker(allowsFolders: true) { urls in if let url = urls.first { value = url.path }; choosingDirectory = false } }
        .onAppear { if context.prompt.kind == .filesSelection { selectedItemIDs = Set(context.prompt.selectionItems.map(\.id)) } }
    }
    private var title: String { switch context.prompt.kind { case .decryptionKey: return "Enter decryption key"; case .password: return "Enter transfer password"; case .peerConsent: return "Direct connection"; case .directoryChoice: return "Choose a folder"; case .filesSelection: return "Choose files"; case .shareReady: return "Share ready" } }
    private var detail: String { context.prompt.kind == .peerConsent ? "A direct peer connection may reveal your network address to the other participant." : "This value is used only to unlock this transfer." }
    private var requiresValue: Bool { ![.peerConsent, .shareReady].contains(context.prompt.kind) }
    private func submit() {
        let response = context.prompt.kind == .filesSelection ? String(data: (try? JSONEncoder().encode(Array(selectedItemIDs).sorted())) ?? Data("[]".utf8), encoding: .utf8) ?? "[]" : value
        submitting = true
        Task {
            if await model.respond(context: context, value: response) {
                value = ""
                dismiss()
            }
            submitting = false
        }
    }
}

private extension String { var nilIfEmpty: String? { isEmpty ? nil : self } }
