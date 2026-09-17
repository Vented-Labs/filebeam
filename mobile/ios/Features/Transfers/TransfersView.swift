import SwiftUI
import UIKit
import FilebeamDomain

struct TransfersView: View {
    @Bindable var model: AppModel
    @State private var filter = TransferFilter.all

    enum TransferFilter: String, CaseIterable, Identifiable {
        case all = "All", active = "Active", completed = "Completed"
        var id: Self { self }
    }

    private enum Item: Identifiable {
        case live(TransferSnapshot)
        case saved(TransferRecord)

        var id: String {
            switch self {
            case let .live(snapshot): return "job:\(snapshot.id.uuidString)"
            case let .saved(record): return "record:\(record.id.rawValue)"
            }
        }

        var isCompleted: Bool {
            switch self {
            case let .live(snapshot): return snapshot.lifecycle == .complete
            case let .saved(record):
                switch record.phase {
                case .complete, .verifiedAwaitingExport, .expired, .revoked, .ended: return true
                default: return false
                }
            }
        }
    }

    var body: some View {
        NavigationStack {
            List {
                Picker("Transfers", selection: $filter) {
                    ForEach(TransferFilter.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .listRowInsets(EdgeInsets())

                ForEach(filtered) { item in
                    switch item {
                    case let .live(snapshot):
                        NavigationLink { DynamicTransferDetailView(model: model, initial: snapshot) } label: {
                            TransferCard(snapshot: snapshot)
                        }
                    case let .saved(record):
                        NavigationLink { SavedTransferDetailView(model: model, record: record) } label: {
                            SavedTransferCard(record: record)
                        }
                    }
                }
                if filtered.isEmpty {
                    ContentUnavailableView("No transfers", systemImage: "arrow.left.arrow.right", description: Text("Your local transfer activity and recovery records appear here."))
                }
            }
            .navigationTitle("Transfers")
            .refreshable { await model.refreshTransfers() }
        }
    }

    private var items: [Item] {
        let activeTransferIDs = Set(model.transfers.compactMap(\.transferID))
        let activeCheckpointIDs = Set(model.transfers.compactMap(\.checkpointID))
        let live = model.transfers.map(Item.live)
        let saved = model.records.filter { !activeTransferIDs.contains($0.id) && !activeCheckpointIDs.contains($0.id.rawValue) }.map(Item.saved)
        return live + saved
    }

    private var filtered: [Item] {
        items.filter {
            switch filter {
            case .all: return true
            case .active: return !$0.isCompleted
            case .completed: return $0.isCompleted
            }
        }
    }
}

struct TransferCard: View {
    let snapshot: TransferSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(snapshot.kind == .note ? "Encrypted note" : "Encrypted files", systemImage: snapshot.direction == .upload ? "arrow.up.circle" : "arrow.down.circle")
                Spacer()
                Text(snapshot.phase.rawValue).foregroundStyle(.secondary)
            }
            if let total = snapshot.totalBytes, total > 0 {
                ProgressView(value: Double(snapshot.completedBytes), total: Double(total))
            }
            if let error = snapshot.error { Text(error).font(.footnote).foregroundStyle(.red) }
        }
        .padding(.vertical, 4)
    }
}

private struct SavedTransferCard: View {
    let record: TransferRecord

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Label(record.kind == .note ? "Encrypted note" : "Encrypted files", systemImage: record.direction == .upload ? "arrow.up.circle" : "arrow.down.circle")
                Spacer()
                Text(record.phase.rawValue).foregroundStyle(.secondary)
            }
            Text("Recovery record").font(.footnote).foregroundStyle(.secondary)
            if record.verifiedPrivately { Label("Verified on this device", systemImage: "checkmark.seal.fill").font(.footnote).foregroundStyle(.green) }
        }
        .padding(.vertical, 4)
    }
}

struct DynamicTransferDetailView: View {
    @Bindable var model: AppModel
    let initial: TransferSnapshot
    @State private var activity: TransferActivity?
    @State private var destructiveAction: TransferAction?

    private var snapshot: TransferSnapshot { model.transfers.first(where: { $0.id == initial.id }) ?? initial }

    var body: some View {
        List {
            Section("Status") { ReceiveProgress(snapshot: snapshot) }
            if let activity { ActivitySection(activity: activity) }
            verifiedResult
            actions
        }
        .navigationTitle("Transfer")
        .task(id: snapshot.checkpointID) { await pollActivity(checkpointID: snapshot.checkpointID) }
        .confirmationDialog(destructiveTitle, isPresented: Binding(get: { destructiveAction != nil }, set: { if !$0 { destructiveAction = nil } })) {
            Button(destructiveTitle, role: .destructive) {
                guard let destructiveAction else { return }
                Task { await model.act(on: snapshot, action: destructiveAction); self.destructiveAction = nil }
            }
        } message: { Text(destructiveDetail) }
    }

    @ViewBuilder private var verifiedResult: some View {
        if let note = snapshot.verifiedNote {
            Section { NavigationLink("Open verified note") { VerifiedNoteView(note: note) } }
        }
        if !snapshot.verifiedFilePaths.isEmpty {
            Section("Verified result") { VerifiedFilesView(model: model, jobID: snapshot.id, paths: snapshot.verifiedFilePaths, exportStatus: snapshot.exportStatus, retryAllowed: snapshot.actions.retryExport) }
        }
    }

    @ViewBuilder private var actions: some View {
        Section("Actions") {
            if snapshot.actions.pause { Button("Pause") { Task { await model.act(on: snapshot, action: .pause) } } }
            if snapshot.actions.resume { Button("Resume") { Task { await model.act(on: snapshot, action: .resume) } } }
            if snapshot.actions.retryNoteBurn { Button("Retry one-time note completion") { Task { _ = await model.retryNoteBurn(jobID: snapshot.id) } } }
            if snapshot.transferID != nil { Button("Open share receipt") { Task { await model.act(on: snapshot, action: .receipt) } } }
            if snapshot.shareURL != nil { Button("Download with CLI") { Task { await model.act(on: snapshot, action: .downloadCommand) } } }
            if snapshot.actions.endLive { Button("End live share", role: .destructive) { destructiveAction = .endLive } }
            if snapshot.actions.revokeRemote { Button("Revoke remote transfer", role: .destructive) { destructiveAction = .revokeRemote } }
            if snapshot.actions.discardLocal { Button("Remove local state", role: .destructive) { destructiveAction = .discardLocal } }
        }
    }

    private func pollActivity(checkpointID: String?) async {
        guard let checkpointID else { return }
        while !Task.isCancelled {
            if let refreshed = try? await model.service.transferActivity(checkpointID: checkpointID), !Task.isCancelled { activity = refreshed }
            try? await Task.sleep(for: .seconds(5))
        }
    }

    private var destructiveTitle: String {
        switch destructiveAction { case .endLive: return "End live share"; case .revokeRemote: return "Revoke remote transfer"; case .discardLocal: return "Remove local state"; default: return "" }
    }
    private var destructiveDetail: String {
        switch destructiveAction { case .endLive: return "This stops this live session."; case .revokeRemote: return "This changes remote availability; existing copies are unaffected."; case .discardLocal: return "This removes recovery data from this device and does not revoke remote access."; default: return "" }
    }
}

private struct ActivitySection: View {
    let activity: TransferActivity
    var body: some View {
        Section("Activity") {
            if activity.unavailable { Text("Activity is temporarily unavailable.").foregroundStyle(.secondary) }
            ForEach(activity.records) { record in
                HStack { Text(record.state); Spacer(); if let progress = record.progress { Text(normalized(progress), format: .percent) } }
            }
        }
    }
    private func normalized(_ value: Double) -> Double { value > 1 ? value / 100 : value }
}

private struct SavedTransferDetailView: View {
    @Bindable var model: AppModel
    let record: TransferRecord
    @State private var destructive: SavedAction?
    private enum SavedAction { case discard, revoke, end }

    var body: some View {
        List {
            Section("Status") {
                LabeledContent("Phase", value: record.phase.rawValue)
                LabeledContent("Recovery ID", value: record.id.rawValue)
                if record.verifiedPrivately { Label("Verified on this device", systemImage: "checkmark.seal.fill").foregroundStyle(.green) }
                exportStatus
            }
            Section("Actions") {
                if record.actions.resume { Button("Resume") { Task { await model.resume(record: record) } } }
                if record.actions.retryExport { Text("Verified output remains local. Reopen the original transfer to save it to Files.").foregroundStyle(.secondary) }
                if record.actions.endLive { Button("End live share", role: .destructive) { destructive = .end } }
                if record.actions.revokeRemote { Button("Revoke remote transfer", role: .destructive) { destructive = .revoke } }
                if record.actions.discardLocal { Button("Remove local state", role: .destructive) { destructive = .discard } }
            }
        }
        .navigationTitle("Recovery record")
        .confirmationDialog(title, isPresented: Binding(get: { destructive != nil }, set: { if !$0 { destructive = nil } })) {
            Button(title, role: .destructive) {
                guard let destructive else { return }
                Task {
                    switch destructive { case .discard: await model.discard(record: record); case .revoke: await model.revoke(record: record); case .end: await model.end(record: record) }
                    self.destructive = nil
                }
            }
        }
    }
    @ViewBuilder private var exportStatus: some View {
        switch record.exportStatus { case .failedRetained(let message): Text(message).foregroundStyle(.red); case .failedNotRetained(let message): Text(message).foregroundStyle(.red); default: EmptyView() }
    }
    private var title: String { switch destructive { case .discard: return "Remove local state"; case .revoke: return "Revoke remote transfer"; case .end: return "End live share"; default: return "" } }
}

struct VerifiedFilesView: View {
    @Bindable var model: AppModel
    let jobID: TransferJobID
    let paths: [String]
    let exportStatus: ExportStatus
    let retryAllowed: Bool
    @State private var exporting = false
    @State private var preview: URL?
    @State private var exportError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Verified on this device", systemImage: "checkmark.seal.fill").foregroundStyle(.green)
            ForEach(paths, id: \.self) { path in Button(URL(fileURLWithPath: path).lastPathComponent) { preview = URL(fileURLWithPath: path) } }
            Button(retryAllowed ? "Retry Save to Files" : "Save to Files") { exporting = true }.buttonStyle(.bordered)
            Text("Saving is separate from verification.").font(.footnote).foregroundStyle(.secondary)
            if case let .failedRetained(message) = exportStatus { InlineNotice(text: message) }
            if case let .failedNotRetained(message) = exportStatus { InlineNotice(text: message) }
            if let exportError { InlineNotice(text: exportError) }
        }
        .sheet(isPresented: $exporting) {
            DocumentExportSheet(urls: paths.map(URL.init(fileURLWithPath:))) { result in
                exporting = false
                switch result {
                case let .success(destinations): Task { await model.recordExport(jobID: jobID, paths: destinations.map(\.path)) }
                case let .failure(error): exportError = error.localizedDescription
                case nil: break
                }
            }
        }
        .sheet(isPresented: Binding(get: { preview != nil }, set: { if !$0 { preview = nil } })) { if let preview { NativePreviewSheet(url: preview) } }
    }
}

struct ShareReceiptView: View {
    @Environment(\.dismiss) private var dismiss
    let receipt: ShareReceipt
    @State private var showKey = false
    @State private var copied = false

    var body: some View {
        NavigationStack {
            List {
                Section("Share link") {
                    Text(receipt.link).textSelection(.enabled)
                    Button(copied ? "Copied" : "Copy link") { copy(receipt.link) }.frame(minWidth: 100, alignment: .leading)
                    ShareLink(item: receipt.link)
                }
                if let key = receipt.separateKey {
                    Section("Decryption key") {
                        Text("Share this separately from the link.").foregroundStyle(.secondary)
                        if showKey { Text(key).textSelection(.enabled); Button("Copy key") { copy(key) } }
                        Button(showKey ? "Hide key" : "Show key") { showKey.toggle() }
                    }
                }
            }
            .navigationTitle("Share receipt")
            .toolbar { Button("Done") { dismiss() } }
        }
    }
    private func copy(_ value: String) { UIPasteboard.general.string = value; copied = true; Task { try? await Task.sleep(for: .seconds(2)); copied = false } }
}

#if DEBUG
#Preview("Transfer components") {
    List {
        TransferCard(snapshot: .init(id: UUID(), transferID: TransferID("preview"), checkpointID: "preview", lifecycle: .running, phase: .receiving, direction: .download, kind: .files, transport: .http, completedBytes: 50, totalBytes: 100))
        ReceiveProgress(snapshot: .init(id: UUID(), transferID: nil, checkpointID: nil, lifecycle: .complete, phase: .verifiedAwaitingExport, direction: .download, kind: .note, transport: .http, completedBytes: 100, totalBytes: 100))
    }
}
#endif
