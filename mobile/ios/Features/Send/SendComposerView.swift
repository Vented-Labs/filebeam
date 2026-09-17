import SwiftUI
import FilebeamDomain

struct SendComposerView: View {
    @Bindable var model: AppModel
    let importSources: ([URL]) async -> [FileSource]
    @State private var mode = ComposerMode.files
    @State private var showingPicker = false
    @State private var pickerAllowsFolders = false
    @State private var showingOptions = false
    @State private var password = ""
    @State private var showingPassword = false

    enum ComposerMode: String, CaseIterable, Identifiable { case files = "Files", notes = "Notes"; var id: Self { self } }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Picker("Content type", selection: $mode) { ForEach(ComposerMode.allCases) { Text($0.rawValue).tag($0) } }.pickerStyle(.segmented)
                    if mode == .files { files } else { note }
                    if let error = model.operationError { InlineNotice(text: error) }
                }.frame(maxWidth: 640).padding()
            }
            .navigationTitle("Send")
            .safeAreaInset(edge: .bottom) {
                PrimaryActionButton(title: "Encrypt and share", disabled: !canSubmit) {
                    Task { if mode == .files { _ = await model.startFileTransfer(password: password.isEmpty ? nil : password) } else { _ = await model.startNote(password: password.isEmpty ? nil : password) }; password = "" }
                }.padding().background(.bar)
            }
        }
        .sheet(isPresented: $showingPicker) { NativeDocumentPicker(allowsFolders: pickerAllowsFolders) { urls in model.importSources(urls, importer: importSources); showingPicker = false } }
        .sheet(isPresented: $showingOptions) {
            if mode == .files { FileOptionsSheet(options: $model.drafts.files.options, password: $password) }
            else { NoteOptionsSheet(options: $model.drafts.note.options, password: $password) }
        }
    }

    private var canSubmit: Bool {
        guard let policy = model.policy else { return false }
        let validation = mode == .files ? SendPolicy.validate(model.drafts.files, policy: policy, isAuthenticated: model.session != nil) : SendPolicy.validate(model.drafts.note, policy: policy, isAuthenticated: model.session != nil)
        if case .ready = validation.submissionStatus { return true }; return false
    }

    private var files: some View {
        Group {
            Text("Send encrypted files").font(.title2.bold())
            Text("Encrypted on your device before sharing.").foregroundStyle(.secondary)
            TransportPicker(transport: $model.drafts.files.options.transport)
            if model.drafts.files.sources.isEmpty { VStack(spacing: 12) { Image("FilebeamMark").resizable().scaledToFit().frame(width: 48, height: 68).accessibilityHidden(true); ContentUnavailableView("No files selected", systemImage: "document.badge.plus", description: Text("Choose files to prepare them for sharing.")) } }
            ForEach(model.drafts.files.sources) { source in
                HStack { Image(systemName: "doc"); VStack(alignment: .leading) { Text(source.name).lineLimit(2); Text(sourceDescription(source)).font(.footnote).foregroundStyle(.secondary) }; Spacer(); if case .importFailed = source.state { Button("Retry") { model.retrySource(source, importer: importSources) }.accessibilityLabel("Retry \(source.name)") }; Button("Remove", role: .destructive) { model.drafts.files.sources.removeAll { $0.id == source.id } }.accessibilityLabel("Remove \(source.name)") }.padding().background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
            }
            HStack { Button(model.drafts.files.sources.isEmpty ? "Choose files" : "Add more") { pickerAllowsFolders = false; showingPicker = true }.buttonStyle(.bordered); Button("Choose folder") { pickerAllowsFolders = true; showingPicker = true }.buttonStyle(.bordered) }
            Button { showingOptions = true } label: { Label(fileSummary, systemImage: "slider.horizontal.3") }.buttonStyle(.bordered)
        }
    }

    private var note: some View {
        Group {
            Text("Share an encrypted note").font(.title2.bold())
            TextField("Title (optional)", text: $model.drafts.note.title)
            Picker("Syntax", selection: $model.drafts.note.language) { ForEach(NoteLanguage.allCases, id: \.self) { Text($0.rawValue.capitalized).tag($0) } }
            TextEditor(text: $model.drafts.note.text).font(model.drafts.note.language == .plain ? .body : .system(.body, design: .monospaced)).frame(minHeight: 250).autocorrectionDisabled(model.drafts.note.language != .plain).textInputAutocapitalization(model.drafts.note.language == .plain ? .sentences : .never).overlay(RoundedRectangle(cornerRadius: 12).stroke(.separator))
            Button { showingOptions = true } label: { Label(noteSummary, systemImage: "slider.horizontal.3") }.buttonStyle(.bordered)
        }
    }

    private var fileSummary: String { "\(model.drafts.files.options.transport == .http ? "HTTP" : "WebRTC") · \(model.drafts.files.options.includeKeyInLink ? "Key in link" : "Key separate")" }
    private var noteSummary: String { "\(model.drafts.note.options.live ? "WebRTC" : "HTTP") · \(model.drafts.note.options.includeKeyInLink ? "Key in link" : "Key separate")" }
    private func sourceDescription(_ source: FileSource) -> String { switch source.state { case .ready: return source.sizeBytes.map(\.filebeamBytes) ?? "Size pending"; case .sizePending: return "Size pending"; case .importing: return "Preparing"; case let .importFailed(error): return error } }
}

struct TransportPicker: View {
    @Binding var transport: Transport
    var body: some View { VStack(alignment: .leading) { Picker("Delivery method", selection: $transport) { Text("HTTP").tag(Transport.http); Text("WebRTC").tag(Transport.webRTC) }.pickerStyle(.segmented); Text(transport == .http ? "Download later." : "Keep Filebeam open.").font(.footnote).foregroundStyle(.secondary) } }
}

struct FileOptionsSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Binding var options: FileTransferOptions
    @Binding var password: String
    @State private var transaction: FileOptionsTransaction
    @State private var passwordDraft: String
    init(options: Binding<FileTransferOptions>, password: Binding<String>) { _options = options; _password = password; _transaction = State(initialValue: .init(options: options.wrappedValue)); _passwordDraft = State(initialValue: password.wrappedValue) }
    var body: some View { NavigationStack { Form { Section("Delivery") { Toggle("Turbo Transfer", isOn: $transaction.draft.turbo).disabled(transaction.draft.transport != .http) }; Section("Packaging") { Toggle("Combine files into a ZIP", isOn: $transaction.draft.archive) }; Section("Protection") { Toggle("Optional transfer password", isOn: $transaction.draft.passwordEnabled); if transaction.draft.passwordEnabled { SecureField("Transfer password", text: $passwordDraft) } }; Section("Link sharing") { Toggle("Include key in link", isOn: $transaction.draft.includeKeyInLink) }; Section("Lifetime") { TextField("Hours", value: $transaction.draft.retentionHours, format: .number).keyboardType(.numberPad) } }.navigationTitle("Transfer options").toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }; ToolbarItem(placement: .confirmationAction) { Button("Done") { options = transaction.commit(); password = passwordDraft; dismiss() } } } } }
}

struct NoteOptionsSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Binding var options: NoteOptions
    @Binding var password: String
    @State private var transaction: NoteOptionsTransaction
    @State private var passwordDraft: String
    init(options: Binding<NoteOptions>, password: Binding<String>) { _options = options; _password = password; _transaction = State(initialValue: .init(options: options.wrappedValue)); _passwordDraft = State(initialValue: password.wrappedValue) }
    var body: some View { NavigationStack { Form { Section("Delivery") { Toggle("Live note", isOn: $transaction.draft.live) }; Section("Protection") { Toggle("Optional transfer password", isOn: $transaction.draft.passwordEnabled); if transaction.draft.passwordEnabled { SecureField("Transfer password", text: $passwordDraft) }; Toggle("Burn on read", isOn: $transaction.draft.burnOnRead) }; Section("Link sharing") { Toggle("Include key in link", isOn: $transaction.draft.includeKeyInLink) }; Section("Lifetime") { TextField("Hours", value: $transaction.draft.retentionHours, format: .number).keyboardType(.numberPad) } }.navigationTitle("Transfer options").toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }; ToolbarItem(placement: .confirmationAction) { Button("Done") { options = transaction.commit(); password = passwordDraft; dismiss() } } } } }
}
