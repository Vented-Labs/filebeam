import SwiftUI
import FilebeamDomain

/// Everything about receiving into this account: who may send, the receiving key, and notifications.
struct SecureInboxView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var directory: ContactDirectory?
    @State private var situation: KeySituation?
    @State private var activeKey: AccountKeyBundle?
    @State private var wizard = false
    @State private var error: String?
    @State private var exporting: URL?
    @State private var importing = false
    @State private var revision = 0

    var body: some View {
        let session = model.session
        let hasKey = situation?.activeBundleID != nil
        let inboxOn = session?.inboxEnabled ?? false
        let routing = model.policy?.usernameRoutingEnabled != false
        SubScreen("Secure inbox") {
            if let error { Notice(error, tone: .danger) }
            if !routing { Notice("New username inbox setup is unavailable on this instance.", tone: .warning) }
            SectionLabel("Receiving key")
            Panel {
                VStack(alignment: .leading, spacing: Space.md) {
                    HStack(spacing: Space.md) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(hasKey && inboxOn ? "Secure inbox active" : hasKey ? "Receiving paused" : "Not set up yet").fbText(.title)
                            if let key = activeKey { Text("Key version \(key.version)").fbText(.small, color: theme.textMuted) }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        StatusPill(hasKey && inboxOn ? "On" : "Off", tone: hasKey && inboxOn ? .success : .neutral)
                    }
                    if let key = activeKey { Text(key.fingerprint.chunked(4).joined(separator: " ")).fbText(.codeSmall, color: theme.textMuted) }
                    if hasKey {
                        Hairline()
                        SwitchRow("Accept incoming transfers", isOn: Binding(get: { inboxOn }, set: { enabled in act { try await model.updatePreferences(inboxEnabled: enabled) } }), description: "People allowed by your receiving settings can send to your username.", icon: .inbox)
                    }
                    FbButton(hasKey ? "Replace lost key" : "Activate secure inbox", kind: hasKey ? .secondary : .primary, icon: .key, enabled: routing || hasKey) { wizard = true }
                    if hasKey {
                        HStack(spacing: Space.md) {
                            FbButton("Export key", kind: .quiet) { exportKey() }
                            FbButton("Import key", kind: .quiet) { importing = true }
                        }
                    }
                }
                .padding(Space.lg)
            }
            SectionLabel("Receiving defaults")
            ReceivingDefaultsPanel(directory: directory) { policy, auto in act { try await model.service.receivingDefaults(instance: model.instance, policy: policy, autoDownload: auto) } }
            SectionLabel("Notifications")
            Panel {
                VStack(alignment: .leading, spacing: Space.md) {
                    Text("New transfer notifications").fbText(.bodyStrong)
                    ChoiceRow("Email", selected: session?.notificationChannel == "mail", icon: .bell) { act { try await model.updatePreferences(notificationChannel: "mail") } }
                    ChoiceRow("In-app", selected: session?.notificationChannel == "database", icon: .notification) { act { try await model.updatePreferences(notificationChannel: "database") } }
                    Text("Filenames are never included in notifications.").fbText(.caption, color: theme.textMuted)
                }
                .padding(Space.lg)
            }
        }
        .task(id: "\(session?.id ?? 0)-\(revision)") { await load() }
        .onChange(of: model.session == nil, initial: true) { _, signedOut in if signedOut { model.replace(.auth(.login)) } }
        .sheet(isPresented: $wizard) { KeyWizard(situation: situation) { custody, password, acknowledged in
            try await setupKey(custody: custody, password: password, acknowledged: acknowledged)
        } }
        .sheet(item: Binding(get: { exporting.map(ExportKeyFile.init) }, set: { if $0 == nil { exporting = nil } })) { item in
            DocumentExportSheet(urls: [item.url]) { result in
                try? FileManager.default.removeItem(at: item.url.deletingLastPathComponent())
                exporting = nil
                if case .success = result { model.toast("Key exported", tone: .success) }
            }
            .ignoresSafeArea()
        }
        .sheet(isPresented: $importing) {
            NativeDocumentPicker(allowsFolders: false) { urls in
                importing = false
                guard let url = urls.first else { return }
                act {
                    let accessing = url.startAccessingSecurityScopedResource()
                    defer { if accessing { url.stopAccessingSecurityScopedResource() } }
                    let data = try Data(contentsOf: url).prefix(16 * 1024)
                    guard let text = String(data: data, encoding: .utf8) else { throw FilebeamDomainError.invalidInput("The selected file is not a key export.") }
                    try await model.importSelfCustodyKey(text)
                    model.toast("Key imported", tone: .success)
                }
            }
            .ignoresSafeArea()
        }
    }

    private func load() async {
        guard model.session != nil else { return }
        directory = try? await model.service.contacts(instance: model.instance)
        do {
            situation = try await model.service.keySituation(instance: model.instance)
            activeKey = try await model.service.accountKeys(instance: model.instance).first { $0.isActive }
        } catch { self.error = readableMessage(error) }
    }

    private func act(_ block: @escaping () async throws -> Void) {
        error = nil
        Task {
            do { try await block() } catch { self.error = readableMessage(error) }
            revision += 1
        }
    }

    private func exportKey() {
        act {
            let exported = try await model.exportActiveKey()
            let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let url = directory.appendingPathComponent("filebeam-account-key.fbsk1")
            try Data(exported.utf8).write(to: url, options: [.atomic, .completeFileProtection])
            exporting = url
        }
    }

    /// Creates or replaces the receiving key and activates the inbox.
    private func setupKey(custody: KeyCustody, password: String?, acknowledged: Bool) async throws {
        guard let session = model.session else { throw FilebeamDomainError.invalidInput("Sign in first.") }
        let current = try await model.service.keySituation(instance: model.instance)
        if current.replacementAcknowledgementRequired && !acknowledged { throw FilebeamDomainError.invalidInput("Replacing the active receiving key requires acknowledgement.") }
        let generated = try await model.service.generateSelfCustodyKey()
        var envelope: String?
        if custody == .password {
            guard let password, !password.isEmpty else { throw FilebeamDomainError.invalidInput("An account password is required for password custody.") }
            envelope = try await model.service.wrapPasswordCustodyKey(privateKey: generated.privateKey, password: password, userID: session.id, publicKey: generated.publicKey)
        }
        let request = AccountKeyUploadRequest(publicKey: generated.publicKey, fingerprint: generated.fingerprint, custody: custody, encryptedPrivateKey: envelope, currentPassword: custody == .password ? password : nil, replace: acknowledged)
        let bundle = try await model.service.uploadAccountKey(instance: model.instance, request: request)
        model.rememberInboxKey(generated.privateKey, bundleID: bundle.id, userID: session.id)
        if !session.inboxEnabled { try await model.updatePreferences(inboxEnabled: true) }
        revision += 1
    }
}

private struct ExportKeyFile: Identifiable {
    let url: URL
    init(_ url: URL) { self.url = url }
    var id: String { url.path }
}

/// Creates or replaces the receiving key: choose who holds it, then confirm.
private struct KeyWizard: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let situation: KeySituation?
    let onCreate: (KeyCustody, String?, Bool) async throws -> Void
    @State private var custody = KeyCustody.password
    @State private var password = ""
    @State private var acknowledged = false
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        let replacing = situation?.replacementAcknowledgementRequired == true
        FbSheet(title: replacing ? "Replace your receiving key" : "Activate secure inbox", subtitle: "Files sent to you are encrypted to a key only you can use. Choose who keeps it.") {
            if replacing { Notice("Files already sent to the old key can only be opened with that old key.", tone: .warning) }
            ChoiceRow("Protect with password (recommended)", selected: custody == .password, description: "The key is stored encrypted with your account password, so you can unlock it on any device.", icon: .lock) { custody = .password }
            ChoiceRow("Keep the key yourself", selected: custody == .selfCustody, description: "Only this device holds the key. Export it and store it safely.", icon: .key) { custody = .selfCustody }
            if custody == .password {
                PasswordField($password, label: "Current password", supporting: "Used on this device to encrypt the key. It is not sent anywhere new.")
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
            if replacing { SwitchRow("I understand earlier deliveries need the old key", isOn: $acknowledged) }
            if let error { Notice(error, tone: .danger) }
            FbButton(replacing ? "Replace lost key" : "Activate secure inbox", enabled: (custody == .selfCustody || !password.isEmpty) && (!replacing || acknowledged), loading: busy) {
                busy = true
                error = nil
                Task {
                    do { try await onCreate(custody, custody == .password ? password : nil, acknowledged); dismiss() } catch { self.error = readableMessage(error) }
                    busy = false
                }
            }
            if custody == .selfCustody { Text("After activating, export the key from this screen and keep the file somewhere safe.").fbText(.caption, color: theme.textMuted) }
        }
        .motion(Motion.spatial, value: custody)
        .privacySensitive()
        .interactiveDismissDisabled(busy)
    }
}
