import SwiftUI
import FilebeamDomain

struct DevicesView: View {
    @Bindable var model: AppModel
    @State private var deviceName = UIDevice.current.name
    @State private var pairingText = ""
    @State private var fingerprint = ""
    @State private var approverFingerprint = ""

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text("Registered devices are managed by this Filebeam instance. A device is ready only when its runtime and receive destination are usable.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                if let directory = model.deviceDirectory {
                    if directory.stale { Text(directory.detail ?? "Device status may be stale.").foregroundStyle(.orange) }
                    ForEach(directory.devices) { device in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack { Text(device.name).font(.headline); Spacer(); Text(device.availability.rawValue).foregroundStyle(.secondary) }
                            Text("\(device.platform) · \(device.trust.rawValue)").font(.footnote).foregroundStyle(.secondary)
                            if let detail = device.detail { Text(detail).font(.footnote).foregroundStyle(.secondary) }
                        }
                    }
                } else {
                    ContentUnavailableView("My devices unavailable", systemImage: "iphone.slash", description: Text(model.deviceDirectoryError ?? "Refresh after signing in to an instance that supports registered devices."))
                }
                Section("Set up this device") {
                    TextField("Device name", text: $deviceName)
                    Button("Register this device") { Task { await model.enrollThisDevice(name: deviceName, platform: UIDevice.current.systemName) } }
                        .disabled(deviceName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Button("Copy pairing payload") {
                        Task {
                            await model.loadDevicePairing()
                            if let payload = model.pendingDevicePairing,
                               let data = try? JSONEncoder().encode(payload) {
                                UIPasteboard.general.string = String(decoding: data, as: UTF8.self)
                            }
                        }
                    }
                }
                Section("Approve another device") {
                    TextEditor(text: $pairingText).frame(minHeight: 88)
                    Button("Paste pairing payload") { pairingText = UIPasteboard.general.string ?? "" }
                    TextField("Confirm candidate key fingerprint", text: $fingerprint)
                    Button("Approve and pin device") {
                        guard let payload = try? JSONDecoder().decode(DevicePairingPayload.self, from: Data(pairingText.utf8)) else {
                            model.deviceOperationError = "The pairing payload is invalid."
                            return
                        }
                        Task { await model.approveDevicePairing(payload, fingerprint: fingerprint) }
                    }.disabled(pairingText.isEmpty || fingerprint.isEmpty)
                    TextField("Confirm approving device fingerprint", text: $approverFingerprint)
                    Button("Confirm approver and pin") { Task { await model.confirmDeviceApprover(fingerprint: approverFingerprint) } }
                        .disabled(approverFingerprint.isEmpty)
                    Text("Compare the candidate fingerprint out of band before approving. Pinning occurs only after authenticated registry sync.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                if let error = model.deviceOperationError { Text(error).foregroundStyle(.red) }
            }
            .navigationTitle("My devices")
            .refreshable { await model.refreshDevices() }
            .toolbar { Button("Refresh") { Task { await model.refreshDevices() } } }
        }
    }
}
