import SwiftUI
import FilebeamDomain

struct ContactsView: View {
    @Environment(\.dismiss) private var dismiss
    @Bindable var model: AppModel
    @State private var directory: ContactDirectory?
    @State private var username = ""
    @State private var busy = false
    @State private var localReceiving = false
    @State private var error: String?

    var body: some View {
        Form {
            Section("Add a friend") {
                TextField("Exact @username", text: $username).textInputAutocapitalization(.never).autocorrectionDisabled()
                PrimaryActionButton(title: "Send friend request", isLoading: busy, disabled: username.isEmpty) { perform(username, "request") }
                Text("Friendships belong to this instance. Your overrides control incoming files.").font(.footnote).foregroundStyle(.secondary)
            }
            if let directory {
                Section("Account receiving defaults") {
                    Picker("Who can send", selection: Binding(get: { directory.settings.receivingPolicy }, set: { policy in saveDefaults(policy, directory.settings.autoDownloadFriends) })) {
                        Text("Anyone, including anonymous").tag("anyone")
                        Text("Signed-in users").tag("authenticated")
                        Text("Friends only").tag("friends")
                        Text("Nobody unless allowed").tag("nobody")
                    }
                    Toggle("Automatically download from friends", isOn: Binding(get: { directory.settings.autoDownloadFriends }, set: { saveDefaults(directory.settings.receivingPolicy, $0) }))
                    Toggle("Stage eligible deliveries on this device", isOn: Binding(get: { localReceiving }, set: { value in Task { do { localReceiving = try await model.service.inboxReceiver(instance: model.instance, enabled: value).enabled; if value { await model.refreshAutomaticInbox() } } catch { self.error = model.message(error) } } }))
                    Text("Background timing is controlled by iOS. Catch-up runs when you return. Files remain locked until you choose to save them.").font(.footnote).foregroundStyle(.secondary)
                }.disabled(busy)
                ForEach(["incoming", "outgoing", "accepted"], id: \.self) { state in
                    Section(state == "accepted" ? "Friends" : state == "incoming" ? "Incoming requests" : "Sent requests") {
                        ForEach(directory.contacts.filter { $0.status == state }) { contact in
                            VStack(alignment: .leading, spacing: 8) {
                                Text("@\(contact.username)").font(.headline)
                                if state == "incoming" {
                                    HStack { Button("Accept") { perform(contact.username, "accept") }; Button("Decline") { perform(contact.username, "decline") } }
                                } else if state == "outgoing" { Button("Cancel request") { perform(contact.username, "cancel") } }
                                else {
                                    Picker("Can send me files", selection: Binding<Int>(get: { selection(contact.canSend) }, set: { perform(contact.username, "preferences", value($0), contact.autoDownload) })) {
                                        Text("Inherit account default").tag(-1); Text("Allow").tag(1); Text("Deny").tag(0)
                                    }
                                    Picker("Automatic download", selection: Binding<Int>(get: { selection(contact.autoDownload) }, set: { perform(contact.username, "preferences", contact.canSend, value($0)) })) {
                                        Text("Inherit account default").tag(-1); Text("On").tag(1); Text("Off").tag(0)
                                    }
                                    Text("Effective: sending \(contact.effective.canSend ? "allowed" : "denied"), automatic download \(contact.effective.autoDownload ? "on" : "off")").font(.footnote).foregroundStyle(.secondary)
                                    Button("Send files") { Task { do { let recipient = try await model.service.recipient(instance: model.instance, username: contact.username); guard recipient.userID == contact.id else { throw FilebeamDomainError.invalidInput("The saved contact account changed. Refresh contacts before sending.") }; model.drafts.files.options.recipient = recipient; model.selectedTab = .send; dismiss() } catch { self.error = model.message(error) } } }
                                    Button("Remove friend") { perform(contact.username, "remove") }
                                }
                                Button("Block", role: .destructive) { perform(contact.username, "block") }
                            }.disabled(busy)
                        }
                    }
                }
                Section("Blocked accounts") { ForEach(directory.blocked) { contact in Button("Unblock @\(contact.username)") { perform(contact.username, "unblock") }.disabled(busy) } }
            }
            if let error { Section { InlineNotice(text: error) } }
        }.navigationTitle("Contacts").task { await load() }.refreshable { await load() }
    }
    private func load() async {
        do { directory = try await model.service.contacts(instance: model.instance); localReceiving = try await model.service.inboxReceiver(instance: model.instance, enabled: nil).enabled }
        catch { self.error = model.message(error) }
    }
    private func perform(_ target: String, _ action: String, _ canSend: Bool? = nil, _ autoDownload: Bool? = nil) {
        guard !busy else { return }; busy = true
        Task { defer { busy = false }; do { directory = try await model.service.contactAction(instance: model.instance, username: target, action: action, canSend: canSend, autoDownload: autoDownload); error = nil } catch { self.error = model.message(error) } }
    }
    private func saveDefaults(_ policy: String, _ autoDownload: Bool) {
        guard !busy else { return }; busy = true
        Task { defer { busy = false }; do { try await model.service.receivingDefaults(instance: model.instance, policy: policy, autoDownload: autoDownload); directory = try await model.service.contacts(instance: model.instance) } catch { self.error = model.message(error) } }
    }
    private func selection(_ value: Bool?) -> Int { switch value { case nil: -1; case true: 1; case false: 0 } }
    private func value(_ selection: Int) -> Bool? { selection == -1 ? nil : selection == 1 }
}
