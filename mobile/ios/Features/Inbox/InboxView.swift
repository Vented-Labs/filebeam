import SwiftUI
import FilebeamDomain

struct InboxView: View {
    @Bindable var model: AppModel
    var body: some View {
        NavigationStack {
            Group {
                if model.session == nil { SignInView(model: model, returnToInbox: true) }
                else if model.session?.inboxEnabled != true { EmptyStateView(title: "Inbox unavailable", detail: "This account or instance does not have Inbox enabled.") }
                else if let error = model.inboxError { ContentUnavailableView("Inbox unavailable", systemImage: "exclamationmark.triangle", description: Text(error)) }
                else { List(model.inboxItems) { item in NavigationLink { InboxItemView(model: model, item: item) } label: { VStack(alignment: .leading) { Text("Encrypted incoming transfer"); Text("\(item.ciphertextBytes.filebeamBytes) · \(item.itemCount) item\(item.itemCount == 1 ? "" : "s")").font(.footnote).foregroundStyle(.secondary) } } }.overlay { if model.inboxItems.isEmpty { ContentUnavailableView("No incoming transfers", systemImage: "tray", description: Text("Account-addressed transfers appear here.")) } }.refreshable { await model.refreshInbox() } }
            }
            .navigationTitle("Inbox")
            .toolbar {
                if model.inboxUnreadCount > 0 {
                    Button("Mark notifications read") { Task { await model.markInboxNotificationsRead() } }
                }
            }
            .task { await model.refreshInbox() }
        }
    }
}

struct InboxItemView: View {
    @Bindable var model: AppModel
    let item: InboxItem
    @State private var metadata: InboxMetadata?
    @State private var showingUnlock = false
    @State private var confirmingDelete = false
    @State private var error: String?
    var body: some View { VStack(alignment: .leading, spacing: 16) { Text("Encrypted incoming transfer").font(.title2.bold()); Text("Unlock an account key before receiving this item. No metadata is shown until it is authorized.").foregroundStyle(.secondary); Text("\(item.ciphertextBytes.filebeamBytes)").font(.footnote); if let metadata { Text("Requires key \(metadata.recipientKeyBundle.fingerprint)").font(.footnote.monospaced()) }; if let error { InlineNotice(text: error) }; Button("Unlock and receive") { showingUnlock = true }.buttonStyle(.borderedProminent); Button("Delete inbox item", role: .destructive) { confirmingDelete = true }; Spacer() }.frame(maxWidth: .infinity, alignment: .leading).padding().navigationTitle("Incoming transfer").task { do { metadata = try await model.service.inboxMetadata(instance: model.instance, transferID: item.id) } catch { self.error = model.message(error) } }.sheet(isPresented: $showingUnlock) { if let metadata { InboxKeyUnlockSheet(model: model, item: item, bundle: metadata.recipientKeyBundle) } }.confirmationDialog("Delete inbox item?", isPresented: $confirmingDelete) { Button("Delete inbox item", role: .destructive) { Task { await model.deleteInboxItem(item) } } } message: { Text("This removes this account-addressed inbox item. This action cannot be undone.") } }
}

struct InboxKeyUnlockSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    let item: InboxItem
    let bundle: AccountKeyBundle
    @State private var password = ""
    @State private var exportedKey = ""
    @State private var error: String?
    var body: some View { NavigationStack { Form { Text("Key \(bundle.fingerprint)").font(.footnote.monospaced()); if bundle.custody == .password { SecureField("Account password", text: $password) } else { TextEditor(text: $exportedKey).font(.system(.body, design: .monospaced)).frame(minHeight: 140); Text("Import your self-custody recovery key for this receive only.").font(.footnote).foregroundStyle(.secondary) }; if let error { InlineNotice(text: error) } }.navigationTitle("Unlock account key").toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { password = ""; exportedKey = ""; dismiss() } }; ToolbarItem(placement: .confirmationAction) { Button("Receive") { Task { await unlockAndReceive() } }.disabled(bundle.custody == .password ? password.isEmpty : exportedKey.isEmpty) } } }.onDisappear { password = ""; exportedKey = "" } }
    private func unlockAndReceive() async { do { let key: Data; if bundle.custody == .password { guard let envelope = bundle.encryptedPrivateKey else { error = "This password-custody key is unavailable."; return }; key = try await model.service.unwrapPasswordCustodyKey(envelope: envelope, password: password, userID: bundle.userID, publicKey: bundle.publicKey) } else { key = try await model.service.importSelfCustodyKey(exportedKey) }; password = ""; exportedKey = ""; await model.receiveInbox(item: item, privateKey: key); dismiss() } catch { error = model.message(error) } }
}
