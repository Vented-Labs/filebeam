import SwiftUI
import FilebeamDomain

/// Registered devices for this account: review, rename, revoke one, or reset them all.
struct DevicesView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var registry: DeviceRegistry?
    @State private var error: String?
    @State private var revision = 0
    @State private var renaming: RegisteredDevice?
    @State private var revoking: RegisteredDevice?
    @State private var resetting = false

    var body: some View {
        SubScreen("My devices", actions: { IconAction(.refresh, "Refresh") { revision += 1 } }) {
            if let registry {
                let active = registry.devices.filter { $0.status == "active" }.count
                VStack(alignment: .leading, spacing: Space.xs) {
                    Text(active == 1 ? "1 active device" : "\(active) active devices").fbText(.headline)
                    Text("Registry revision \(registry.revision)").fbText(.caption, color: theme.textSubtle)
                    Text("Devices pair with the Beam CLI to send files to each other directly. Revoke any device you no longer use.").fbText(.small, color: theme.textMuted)
                }
                if let error { Notice(error, tone: .danger) }
                if registry.devices.isEmpty { EmptyState(.laptop, "No devices yet", body: "Devices you add with the Beam CLI appear here.") }
                ForEach(registry.devices) { device in
                    DeviceCard(device: device, onRename: { renaming = device }, onRevoke: { revoking = device })
                }
                if registry.devices.contains(where: { !$0.isRevoked }) { FbButton("Reset all devices", kind: .danger) { resetting = true } }
            } else if let error {
                EmptyState(.alert, "Devices could not be loaded", body: readableMessage(error), tone: .danger) {
                    FbButton("Retry", kind: .secondary, fill: false) { revision += 1 }
                }
            } else { CenteredSpinner() }
        }
        .task(id: revision) {
            error = nil
            do { registry = try await model.service.devices(instance: model.instance) } catch { self.error = readableMessage(error) }
        }
        .onChange(of: model.session == nil) { _, signedOut in if signedOut { model.replace(.auth(.login)) } }
        .sheet(item: $renaming) { device in RenameSheet(device: device) { revision += 1 } }
        .sheet(item: $revoking) { device in PasswordConfirmSheet(title: "Revoke \(device.displayName)?", body: "This device loses access immediately. Enter your password to confirm.", confirmLabel: "Revoke") { password in
            try await model.service.revokeDevice(instance: model.instance, deviceID: device.id, currentPassword: password)
            model.toast("Device revoked", tone: .success)
            revision += 1
        } }
        .sheet(isPresented: $resetting) { PasswordConfirmSheet(title: "Reset every device?", body: "Every device is revoked and must be added again. Enter your password to confirm.", confirmLabel: "Reset all devices") { password in
            try await model.service.resetDevices(instance: model.instance, currentPassword: password)
            model.toast("All devices reset", tone: .success)
            revision += 1
        } }
    }
}

private struct DeviceCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let device: RegisteredDevice
    let onRename: () -> Void
    let onRevoke: () -> Void
    @State private var expanded = false
    @State private var fingerprint = ""

    var body: some View {
        let mobile = device.platform.localizedCaseInsensitiveContains("android") || device.platform.localizedCaseInsensitiveContains("ios")
        Panel {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.md) {
                    FbIcon(mobile ? .mobile : .laptop, size: 26, tint: theme.accentText).frame(width: 44, height: 44)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(device.displayName).fbText(.bodyStrong).lineLimit(1)
                        Text("\(device.platform) · key version \(device.keyVersion)").fbText(.small, color: theme.textMuted).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    StatusPill(device.isRevoked ? "Revoked" : device.status == "pending" ? "Pending approval" : device.isOnline ? "Ready" : "Offline", tone: device.isRevoked ? .danger : device.status == "pending" ? .warning : device.isOnline ? .success : .neutral)
                }
                Button { withMotion(Motion.spatial) { expanded.toggle() } } label: {
                    Text(expanded ? fingerprint.chunked(4).joined(separator: " ") : String(fingerprint.prefix(16)).chunked(4).joined(separator: " ") + "…").fbText(.codeSmall, color: theme.textMuted).multilineTextAlignment(.leading)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Show full fingerprint")
                if let revoked = device.revokedAt { Text("Revoked \(formatAbsolute(revoked))").fbText(.caption, color: theme.danger.ink) }
                if !device.isRevoked {
                    HStack(spacing: Space.md) {
                        FbButton("Rename", kind: .secondary, height: 44, action: onRename)
                        FbButton("Revoke", kind: .danger, height: 44, action: onRevoke)
                    }
                }
            }
            .padding(Space.lg)
        }
        .task(id: device.publicKey) { fingerprint = (try? await model.service.deviceFingerprint(publicKey: device.publicKey)) ?? device.publicKey }
    }
}

private struct RenameSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(AppModel.self) private var model
    let device: RegisteredDevice
    let done: () -> Void
    @State private var name: String
    @State private var busy = false
    @State private var error: String?

    init(device: RegisteredDevice, done: @escaping () -> Void) {
        self.device = device; self.done = done
        _name = State(initialValue: device.displayName)
    }

    var body: some View {
        FbSheet(title: "Rename device") {
            FbTextField($name, label: "Device name", error: error, autocapitalization: .words)
                .onChange(of: name) { _, value in if value.count > 80 { name = String(value.prefix(80)) } }
            FbButton("Rename", enabled: !name.isBlank && name.trimmingCharacters(in: .whitespaces) != device.displayName, loading: busy) {
                busy = true
                Task {
                    do {
                        try await model.service.renameDevice(instance: model.instance, deviceID: device.id, name: name.trimmingCharacters(in: .whitespaces))
                        model.toast("Device renamed", tone: .success)
                        done()
                        dismiss()
                    } catch { self.error = readableMessage(error) }
                    busy = false
                }
            }
        }
    }
}

/// Confirms a destructive account action with the current password.
struct PasswordConfirmSheet: View {
    @Environment(\.dismiss) private var dismiss
    let title: String
    let body_: String
    let confirmLabel: String
    let action: (String) async throws -> Void
    @State private var password = ""
    @State private var busy = false
    @State private var error: String?

    init(title: String, body: String, confirmLabel: String, action: @escaping (String) async throws -> Void) {
        self.title = title; body_ = body; self.confirmLabel = confirmLabel; self.action = action
    }

    var body: some View {
        FbSheet(title: title, subtitle: body_) {
            PasswordField($password, label: "Current password", error: error)
            FbButton(confirmLabel, kind: .danger, enabled: !password.isEmpty, loading: busy) {
                busy = true
                error = nil
                Task {
                    do { try await action(password); dismiss() } catch { self.error = readableMessage(error) }
                    busy = false
                }
            }
        }
        .privacySensitive()
        .interactiveDismissDisabled(busy)
    }
}
