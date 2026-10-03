import SwiftUI
import FilebeamDomain

struct StagedInboxView: View {
    @Bindable var model: AppModel
    var body: some View {
        List {
            ForEach(model.stagedInbox.filter { $0.state != "dismissed" }) { item in
                if item.state == "staged-locked" {
                    NavigationLink { StagedInboxSaveView(model: model, item: item) } label: {
                        VStack(alignment: .leading) { Text("Encrypted files staged privately"); Text("\(item.bytes.filebeamBytes) · Unlock to save").font(.footnote).foregroundStyle(.secondary) }
                    }
                } else { LabeledContent("Incoming encrypted files", value: item.state) }
                Button("Remove local ciphertext", role: .destructive) {
                    Task {
                        do { try await model.service.dismissStagedInbox(instance: model.instance, id: item.id); await model.refreshAutomaticInbox() }
                        catch { model.operationError = model.message(error) }
                    }
                }
            }
            if model.stagedInbox.isEmpty { Text("No automatically staged files yet.").foregroundStyle(.secondary) }
        }.navigationTitle("Staged files").task { await model.refreshAutomaticInbox() }.refreshable { await model.refreshAutomaticInbox() }
    }
}

private struct StagedInboxSaveView: View {
    @Bindable var model: AppModel
    let item: StagedInboxItem
    @State private var bundle: AccountKeyBundle?
    @State private var password = ""
    @State private var recoveryKey = ""
    @State private var urls: [URL] = []
    @State private var exporting = false
    @State private var busy = false
    @State private var error: String?
    var body: some View {
        Form {
            Text("Downloaded ciphertext remains private. Unlock and verify it before saving to Files.")
            if bundle?.custody == .password { SecureField("Receiving-key password", text: $password) }
            else { SecureField("Self-custody recovery export", text: $recoveryKey).textInputAutocapitalization(.never).autocorrectionDisabled() }
            Button("Unlock and verify") {
                busy = true
                Task {
                    defer { busy = false }
                    do {
                        let key: Data
                        if let bundle, bundle.custody == .password, let envelope = bundle.encryptedPrivateKey {
                            key = try await model.service.unwrapPasswordCustodyKey(envelope: envelope, password: password, userID: bundle.userID, publicKey: bundle.publicKey)
                        } else { key = try await model.service.importSelfCustodyKey(recoveryKey) }
                        password = ""; recoveryKey = ""
                        urls = try await model.verifyStagedInbox(item, privateKey: key)
                        error = nil
                    } catch { self.error = model.message(error) }
                }
            }.disabled(busy || (password.isEmpty && recoveryKey.isEmpty))
            if !urls.isEmpty { Button("Save verified files to Files") { exporting = true } }
            if let error { InlineNotice(text: error) }
        }.navigationTitle("Save staged files")
            .task { bundle = try? await model.service.accountKeys(instance: model.instance).first { $0.id == item.keyBundleID } }
            .sheet(isPresented: $exporting) { DocumentExportSheet(urls: urls) { _ in exporting = false } }
            .onDisappear { password = ""; recoveryKey = "" }
    }
}
