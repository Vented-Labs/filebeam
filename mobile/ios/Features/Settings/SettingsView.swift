import SwiftUI
import FilebeamDomain

struct SettingsView: View {
    @Bindable var model: AppModel
    var body: some View {
        NavigationStack {
            Form {
                Section("Account") { if let session = model.session { NavigationLink(session.email) { AccountView(model: model, session: session) } } else { NavigationLink("Sign in") { SignInView(model: model, returnToInbox: false) }; if model.policy?.registrationEnabled != false { NavigationLink("Create account") { RegistrationView(model: model) } } else { Text("Account registration is disabled by this instance.").foregroundStyle(.secondary) } } }
                Section("Connection") { NavigationLink("Filebeam instance") { InstanceView(model: model) }; Toggle("Relay-only transfers", isOn: Binding(get: { model.relayOnly }, set: { value in Task { _ = await model.setRelayOnly(value) } })); Text("Relay-only changes apply the next time Filebeam creates its native transfer client.").font(.footnote).foregroundStyle(.secondary); Button("Check connection") { Task { await model.refreshPolicy() } } }
                Section("Appearance") { LabeledContent("Appearance", value: "Follows system") }
                Section("Support") { NavigationLink("Report a transfer") { ReportView(model: model) }; LabeledContent("Version", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "Unknown") }
                if let error = model.operationError { Section { InlineNotice(text: error) } }
            }.navigationTitle("Settings")
        }
        .sheet(isPresented: Binding(get: { model.pendingInviteLink != nil }, set: { if !$0 { model.pendingInviteLink = nil } })) { if let link = model.pendingInviteLink { InviteAcceptanceView(model: model, link: link) } }
        .sheet(isPresented: Binding(get: { model.pendingResetLink != nil }, set: { if !$0 { model.pendingResetLink = nil } })) { if let link = model.pendingResetLink { ResetCompletionView(model: model, link: link) } }
        .sheet(isPresented: Binding(get: { model.pendingVerificationLink != nil }, set: { if !$0 { model.pendingVerificationLink = nil } })) { if let link = model.pendingVerificationLink { VerificationView(model: model, link: link) } }
    }
}

struct SignInView: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    let returnToInbox: Bool
    @State private var email = ""
    @State private var password = ""
    @State private var remember = true
    @State private var error: String?
    var body: some View { Form { Section { TextField("Email", text: $email).textContentType(.emailAddress).textInputAutocapitalization(.never); SecureField("Password", text: $password).textContentType(.password); Toggle("Remember me", isOn: $remember); if let error { InlineNotice(text: error) }; Button("Sign in") { Task { if await model.login(email: email, password: password, remember: remember) { password = ""; if returnToInbox { model.selectedTab = .inbox }; dismiss() } else { error = model.operationError } } }.disabled(email.isEmpty || password.isEmpty) }; Section { if model.policy?.registrationEnabled != false { NavigationLink("Create account") { RegistrationView(model: model) } }; NavigationLink("Reset password") { PasswordResetView(model: model) } }.navigationTitle("Sign in") }.onDisappear { password = "" } }
}

struct RegistrationView: View {
    @Bindable var model: AppModel
    @State private var username = ""; @State private var name = ""; @State private var email = ""; @State private var password = ""; @State private var error: String?
    var body: some View { Form { TextField("Username", text: $username).textInputAutocapitalization(.never); TextField("Name (optional)", text: $name); TextField("Email", text: $email).textInputAutocapitalization(.never); SecureField("Password", text: $password); if let error { InlineNotice(text: error) }; Button("Create account") { Task { do { model.session = try await model.service.register(.init(instance: model.instance, username: username, name: name.nilIfEmpty, email: email, password: password)) } catch { self.error = model.message(error) } } }.disabled(username.isEmpty || email.isEmpty || password.isEmpty) }.navigationTitle("Create account") }
}

struct PasswordResetView: View {
    @Bindable var model: AppModel
    @State private var email = ""; @State private var sent = false; @State private var error: String?
    var body: some View { Form { TextField("Email", text: $email).textInputAutocapitalization(.never); if let error { InlineNotice(text: error) }; if sent { Text("Check your email for a reset link.").foregroundStyle(.secondary) }; Button("Send reset link") { Task { do { try await model.service.requestPasswordReset(instance: model.instance, email: email); sent = true } catch { self.error = model.message(error) } } }.disabled(email.isEmpty) }.navigationTitle("Reset password") }
}

struct ResetCompletionView: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel; let link: String
    @State private var email = ""; @State private var token = ""; @State private var password = ""; @State private var error: String?
    var body: some View { NavigationStack { Form { TextField("Email", text: $email).textInputAutocapitalization(.never); SecureField("New password", text: $password); if let error { InlineNotice(text: error) }; Button("Reset password") { Task { do { try await model.service.resetPassword(instance: resetInstance, email: email, token: token, password: password); password = ""; model.pendingResetLink = nil; dismiss() } catch { self.error = model.message(error) } } }.disabled(email.isEmpty || token.isEmpty || password.isEmpty) }.navigationTitle("Reset password").toolbar { Button("Cancel") { password = ""; dismiss() } }.onAppear { parseLink() } } }
    private var resetInstance: FilebeamInstance { (URL(string: link).flatMap { url in url.scheme.flatMap { scheme in url.host.flatMap { host in FilebeamInstance(origin: "\(scheme)://\(host)\(url.port.map { ":\($0)" } ?? "")") } } }) ?? model.instance }
    private func parseLink() { guard case .resetPassword = InputRouter.route(link, selectedInstance: model.instance), let components = URLComponents(string: link) else { error = "Invalid password reset link."; return }; email = components.queryItems?.first(where: { $0.name == "email" })?.value ?? ""; token = components.queryItems?.first(where: { $0.name == "token" })?.value ?? components.path.split(separator: "/").last.map(String.init) ?? ""; if email.isEmpty || token.isEmpty { error = "This reset link is incomplete." } }
}

struct VerificationView: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel; let link: String; @State private var error: String?
    var body: some View { NavigationStack { VStack(spacing: 16) { Text("Verify email").font(.title2.bold()); Text("Verify this email link with the selected Filebeam service.").foregroundStyle(.secondary); if let error { InlineNotice(text: error) }; Button("Verify email") { Task { do { try await model.service.verifyEmail(link: link); model.pendingVerificationLink = nil; dismiss() } catch { self.error = model.message(error) } } }.buttonStyle(.borderedProminent); Spacer() }.padding().navigationTitle("Email verification") } }
}

struct InviteAcceptanceView: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel; let link: String
    @State private var inspection: InviteInspection?; @State private var username = ""; @State private var name = ""; @State private var email = ""; @State private var password = ""; @State private var error: String?
    var body: some View { NavigationStack { Form { if let inspection, let invitedEmail = inspection.email { LabeledContent("Invited email", value: invitedEmail) }; TextField("Username", text: $username).textInputAutocapitalization(.never); TextField("Name (optional)", text: $name); if inspection?.email == nil { TextField("Email", text: $email).textInputAutocapitalization(.never) }; SecureField("Password", text: $password); if let error { InlineNotice(text: error) }; Button("Accept invitation") { Task { do { model.session = try await model.service.acceptInvite(.init(link: link, username: username, name: name.nilIfEmpty, email: email, password: password)); password = ""; model.pendingInviteLink = nil; dismiss() } catch { self.error = model.message(error) } } }.disabled(username.isEmpty || email.isEmpty || password.isEmpty) }.navigationTitle("Accept invitation").task { do { let value = try await model.service.inspectInvite(link: link); inspection = value; email = value.email ?? email } catch { self.error = model.message(error) } } } }
}

struct AccountView: View {
    @Bindable var model: AppModel; let session: AccountSession
    var body: some View { Form { Section("Identity") { LabeledContent("Email", value: session.email); LabeledContent("Verification", value: session.emailVerifiedAt == nil ? "Not verified" : "Verified"); if session.emailVerifiedAt == nil { Button("Resend verification") { Task { do { try await model.service.resendVerification(instance: model.instance) } catch { model.operationError = model.message(error) } } } } }; Section("Preferences") { NavigationLink("Inbox notifications") { PreferencesView(model: model) } }; Section("Security") { NavigationLink("Encryption keys") { KeyManagementView(model: model) }; NavigationLink("Delete account") { AccountDeletionView(model: model) } }; Section { Button("Sign out", role: .destructive) { Task { await model.logout() } } } }.navigationTitle("Account") }
}

struct KeyManagementView: View {
    @Bindable var model: AppModel
    @State private var bundles: [AccountKeyBundle] = []; @State private var error: String?
    var body: some View { List { NavigationLink("Set up or replace key") { KeySetupView(model: model, replacing: !bundles.isEmpty) }; if bundles.isEmpty { Text("No account keys available.") }; ForEach(bundles) { bundle in VStack(alignment: .leading) { Text(bundle.isActive ? "Active key" : "Historical key"); Text(bundle.fingerprint).font(.footnote.monospaced()).textSelection(.enabled); Text(bundle.custody == .selfCustody ? "Self custody" : "Password custody").font(.footnote).foregroundStyle(.secondary) } }; if let error { InlineNotice(text: error) } }.navigationTitle("Encryption keys").task { do { bundles = try await model.service.accountKeys(instance: model.instance) } catch { self.error = model.message(error) } } }
}

struct AccountDeletionView: View {
    @Bindable var model: AppModel; @State private var password = ""; @State private var confirmation = ""; @State private var result: String?; @State private var error: String?
    var body: some View { Form { Text("This requests deletion of the account at this instance. Local files and remote recipient copies are not implied by this request.").foregroundStyle(.secondary); SecureField("Current password", text: $password); TextField("Type DELETE", text: $confirmation); if let result { Text(result).foregroundStyle(.secondary) }; if let error { InlineNotice(text: error) }; Button("Request account deletion", role: .destructive) { Task { do { let response = try await model.service.requestAccountDeletion(.init(instance: model.instance, currentPassword: password, confirmation: confirmation)); password = ""; result = response.accepted ? "Deletion request accepted." : "Deletion request was not accepted." } catch { self.error = model.message(error) } } }.disabled(password.isEmpty || confirmation != "DELETE") }.navigationTitle("Delete account") }
}

struct PreferencesView: View {
    @Bindable var model: AppModel
    @State private var preferences = AccountPreferences(inboxEnabled: false, notificationChannel: "database")
    @State private var error: String?
    var body: some View { Form { Toggle("Enable Inbox", isOn: $preferences.inboxEnabled); Picker("Notifications", selection: $preferences.notificationChannel) { Text("Database").tag("database"); Text("Mail").tag("mail") }; if let error { InlineNotice(text: error) }; Button("Save preferences") { Task { do { try await model.service.updatePreferences(instance: model.instance, preferences: preferences) } catch { self.error = model.message(error) } } } }.navigationTitle("Inbox notifications").task { do { preferences = try await model.service.preferences(instance: model.instance) } catch { self.error = model.message(error) } } }
}

struct KeySetupView: View {
    @Bindable var model: AppModel
    let replacing: Bool
    @State private var custody = KeyCustody.password.rawValue
    @State private var password = ""
    @State private var generated: GeneratedAccountKey?
    @State private var exported = ""
    @State private var importedBackup = ""
    @State private var exportDocument: ExportDocument?
    @State private var showRecoveryExport = false
    @State private var busy = false
    @State private var replacementAcknowledged = false
    @State private var error: String?
    var body: some View { Form { Section("Custody") { Picker("Private key custody", selection: $custody) { Text("Password custody").tag(KeyCustody.password.rawValue); Text("Self custody").tag(KeyCustody.selfCustody.rawValue) }.pickerStyle(.segmented); Text(selectedCustody == .password ? "The private key is encrypted with your account password for recovery." : "Keep a recovery export and prove it can be imported before activating this key.").font(.footnote).foregroundStyle(.secondary) }; if selectedCustody == .password { Section("Password") { SecureField("Current account password", text: $password) } }; if selectedCustody == .selfCustody, generated != nil { Section("Recovery export") { Button("Prepare recovery export") { Task { await prepareExport() } }; if !exported.isEmpty { Button(showRecoveryExport ? "Hide recovery export" : "Show recovery export") { showRecoveryExport.toggle() }; if showRecoveryExport { Text(exported).font(.system(.footnote, design: .monospaced)).textSelection(.enabled) }; Button("Save recovery export to Files") { makeExportFile() }; SecureField("Paste recovery export to validate", text: $importedBackup).textInputAutocapitalization(.never).autocorrectionDisabled() } } }; if replacing { Section("Replacement") { Toggle("I understand older inbox items may require a historical key", isOn: $replacementAcknowledged) } }; if let error { InlineNotice(text: error) }; Button(generated == nil ? "Generate key" : "Validate and activate key") { Task { if generated == nil { await generate() } else { await upload() } } }.disabled(busy || (selectedCustody == .password && password.isEmpty) || (selectedCustody == .selfCustody && (exported.isEmpty || importedBackup.isEmpty)) || (replacing && !replacementAcknowledged)) }.navigationTitle(replacing ? "Replace account key" : "Set up account key").sheet(item: $exportDocument) { document in DocumentExportSheet(urls: [document.url]) { _ in exportDocument = nil } }.onDisappear { password = ""; exported = ""; importedBackup = ""; generated = nil } }
    private var selectedCustody: KeyCustody { custody == KeyCustody.selfCustody.rawValue ? .selfCustody : .password }
    private func generate() async { busy = true; defer { busy = false }; do { generated = try await model.service.generateSelfCustodyKey() } catch { self.error = model.message(error) } }
    private func prepareExport() async { guard let generated else { return }; busy = true; defer { busy = false }; do { exported = try await model.service.exportSelfCustodyKey(generated.privateKey); error = nil } catch { self.error = model.message(error) } }
    private func makeExportFile() { guard !exported.isEmpty else { return }; let url = FileManager.default.temporaryDirectory.appendingPathComponent("filebeam-recovery-key.txt"); do { try Data(exported.utf8).write(to: url, options: .atomic); exportDocument = ExportDocument(url: url) } catch { self.error = model.message(error) } }
    private func upload() async { guard let generated else { return }; busy = true; defer { busy = false }; do { var envelope: String?; if selectedCustody == .password { guard let session = model.session else { error = "Sign in before setting up an account key."; return }; envelope = try await model.service.wrapPasswordCustodyKey(privateKey: generated.privateKey, password: password, userID: session.id, publicKey: generated.publicKey) } else { let imported = try await model.service.importSelfCustodyKey(importedBackup); guard imported == generated.privateKey else { error = "The pasted recovery export does not match the generated key."; return } }; let request = AccountKeyUploadRequest(publicKey: generated.publicKey, fingerprint: generated.fingerprint, custody: selectedCustody, encryptedPrivateKey: envelope, currentPassword: selectedCustody == .password ? password : nil, replace: replacing); let validation = try await model.service.validateAccountKeyUpload(instance: model.instance, request: request); guard validation.isValid else { error = validation.reason ?? "This key cannot be activated."; return }; guard !validation.replacementAcknowledgementRequired || replacementAcknowledged else { error = "A replacement acknowledgement is required."; return }; _ = try await model.service.uploadAccountKey(instance: model.instance, request: request); password = ""; exported = ""; importedBackup = ""; self.generated = nil } catch { self.error = model.message(error) } }
}

struct InstanceView: View {
    @Bindable var model: AppModel
    @State private var origin = ""; @State private var error: String?
    var body: some View { Form { TextField("Instance URL", text: $origin).textInputAutocapitalization(.never).autocorrectionDisabled(); Text("Changing the instance clears the active account and inbox. Drafts remain bound to their original instance until you switch back.").font(.footnote).foregroundStyle(.secondary); if let error { InlineNotice(text: error) }; Button("Save instance") { model.stageInstance(origin: origin); error = model.instanceChangeError } }.confirmationDialog("Switch Filebeam instance?", isPresented: Binding(get: { model.pendingInstance != nil }, set: { if !$0 { model.cancelInstanceChange() } })) { Button("Switch instance", role: .destructive) { Task { if !(await model.confirmInstanceChange()) { error = model.instanceChangeError } else { origin = model.instance.origin } } }; Button("Cancel", role: .cancel) { model.cancelInstanceChange() } } message: { Text("New transfers use the new instance. Existing jobs keep their original connection.") }.navigationTitle("Filebeam instance").onAppear { origin = model.instance.origin } }
}

struct ReportView: View {
    @Bindable var model: AppModel
    @State private var transferID = ""; @State private var category = ReportCategory.allCases.first!; @State private var detail = ""; @State private var email = ""; @State private var error: String?; @State private var submitted = false
    var body: some View { Form { TextField("Transfer ID", text: $transferID); Picker("Category", selection: $category) { ForEach(ReportCategory.allCases, id: \.self) { Text($0.rawValue.replacingOccurrences(of: "_", with: " ").capitalized).tag($0) } }; TextField("Email for follow-up (optional)", text: $email).textInputAutocapitalization(.never); TextEditor(text: $detail).frame(minHeight: 120); if submitted { Text("Report submitted.").foregroundStyle(.secondary) }; if let error { InlineNotice(text: error) }; Button("Submit report") { Task { do { _ = try await model.service.report(.init(instance: model.instance, transferID: TransferID(transferID), category: category, description: detail, email: email.nilIfEmpty)); submitted = true } catch { self.error = model.message(error) } } }.disabled(transferID.isEmpty || detail.isEmpty || submitted) }.navigationTitle("Report transfer") }
}

private extension String { var nilIfEmpty: String? { isEmpty ? nil : self } }
