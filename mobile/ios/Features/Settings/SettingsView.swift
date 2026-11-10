import SwiftUI
import FilebeamDomain

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var instance = ""
    @State private var relayOnly = false
    @State private var checking = false
    @State private var failure: String?

    private var busy: Bool { model.transfers.contains { $0.lifecycle == .running } }
    private var dirty: Bool { instance.trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: "/")) != model.instance.origin || relayOnly != model.relayOnly }

    var body: some View {
        SubScreen("Settings") {
            Panel {
                LinkRow(.brush, "Appearance") { model.push(.appearance) }
                Hairline()
                LinkRow(.storage, "Storage usage") { model.push(.storage) }
                Hairline()
                LinkRow(.info, "About") { model.push(.about) }
            }
            .staggered(0)
            SectionLabel("Connection")
            Panel {
                VStack(alignment: .leading, spacing: Space.lg) {
                    FbTextField($instance, label: "Filebeam instance", placeholder: "https://filebeam.io", supporting: "The server this app sends through and signs in to.", error: failure, icon: .globe, keyboard: .URL, contentType: .URL, submitLabel: .done, onSubmit: { if dirty && !busy { save() } })
                        .onChange(of: instance) { _, _ in failure = nil }
                    Hairline()
                    SwitchRow("Relay-only live transfers", isOn: $relayOnly, description: "Route WebRTC through the instance's TURN relay so peers never see your address.", icon: .shield)
                    if busy && dirty { Notice("Finish or pause the running transfer before changing the connection.", tone: .warning) }
                    FbButton("Check and save", enabled: dirty && !busy, loading: checking) { save() }
                    Text("Relay-only changes apply the next time Filebeam creates its native transfer client.").fbText(.caption, color: theme.textMuted)
                }
                .padding(Space.lg)
            }
            .staggered(1)
        }
        .onAppear { instance = model.instance.origin; relayOnly = model.relayOnly }
    }

    /// An instance is only saved after it answers, so a typo can never strand the app.
    private func save() {
        checking = true
        failure = nil
        Task {
            if relayOnly != model.relayOnly, !(await model.setRelayOnly(relayOnly)) { failure = model.instanceChangeError }
            let origin = instance.trimmingCharacters(in: .whitespaces)
            if origin != model.instance.origin {
                model.stageInstance(origin: origin)
                if let error = model.instanceChangeError { failure = error }
                else if model.pendingInstance != nil {
                    if await model.confirmInstanceChange() { instance = model.instance.origin; model.toast("Connected", tone: .success) }
                    else { failure = model.instanceChangeError ?? "That instance did not answer. Check the address and your connection." }
                }
            } else if failure == nil { model.toast("Connected", tone: .success) }
            checking = false
        }
    }
}
