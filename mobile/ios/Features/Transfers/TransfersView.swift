import SwiftUI
import FilebeamDomain

/// What is moving right now, what is saved on this device, and the way into account history.
struct TransfersView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var selectedJob: TransferSnapshot?
    @State private var selectedRecord: TransferRecord?

    private var live: [TransferSnapshot] { model.transfers.filter { $0.isActive || $0.lifecycle == .failed } }
    private var saved: [TransferRecord] {
        let activeIDs = Set(model.transfers.compactMap(\.transferID)) 
        let checkpoints = Set(model.transfers.compactMap(\.checkpointID))
        return model.records.filter { !activeIDs.contains($0.id) && !checkpoints.contains($0.id.rawValue) }
    }

    var body: some View {
        TabScreen(actions: { IconAction(.refresh, "Refresh") { Task { await model.refreshTransfers() } } }) {
            ScreenHeader("Transfers", subtitle: "Running, paused and saved transfers on this device.").staggered(0)
            ForEach(live) { snapshot in
                ActiveCard(snapshot: snapshot) { selectedJob = snapshot }
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
            let completed = model.transfers.filter { $0.lifecycle == .complete }
            if !saved.isEmpty || !completed.isEmpty {
                SectionLabel("On this device")
                ForEach(completed) { snapshot in
                    CompletedRow(snapshot: snapshot) { selectedJob = snapshot }
                }
                ForEach(saved) { record in
                    SavedRow(record: record) { selectedRecord = record }
                }
            }
            if live.isEmpty && saved.isEmpty && completed.isEmpty {
                EmptyState(.transfers, "No transfers yet", body: "Transfers in progress, paused work and verified downloads appear here.") {
                    FbButton("Send a transfer", icon: .upload, fill: false) { model.select(.send) }
                }
            }
            Panel {
                LinkRow(.clock, "Transfer history", detail: model.session == nil ? "Sign in" : nil) { model.push(model.session != nil ? .history : .auth(.login)) }
                Hairline()
                LinkRow(.storage, "Storage usage") { model.push(.storage) }
            }
            .staggered(2)
        }
        .motion(Motion.spatial, value: live.map(\.id))
        .task { await model.refreshTransfers() }
        .refreshable { await model.refreshTransfers() }
        .sheet(item: $selectedJob) { JobSheet(initial: $0) }
        .sheet(item: $selectedRecord) { RecordSheet(record: $0) }
    }
}

extension TransferRecord {
    var isSend: Bool { direction == .upload }
}

private struct ActiveCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let snapshot: TransferSnapshot
    let open: () -> Void

    var body: some View {
        let send = snapshot.direction == .upload
        let busy = snapshot.lifecycle == .running || snapshot.lifecycle == .pausing
        Button(action: open) {
            Panel(border: theme.choiceBorder.opacity(0.6)) {
                VStack(alignment: .leading, spacing: Space.md) {
                    HStack(spacing: Space.md) {
                        Group { if busy { Spinner(size: 26) } else { FbIcon(snapshot.lifecycle == .failed ? .alert : .clock, size: 26, tint: snapshot.lifecycle == .failed ? theme.danger.ink : theme.accentText) } }
                            .frame(width: 44, height: 44)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(snapshot.phase.label).fbText(.bodyStrong).lineLimit(1).id(snapshot.phase).transition(.opacity)
                            Text(snapshot.kind == .note ? (send ? "Sending a note" : "Receiving a note") : send ? "Sending" : "Receiving").fbText(.small, color: theme.textMuted).lineLimit(1)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        if let fraction = snapshot.fraction { Text(progressLabel(fraction)).fbText(.numeric, color: theme.textMuted) }
                    }
                    SmoothProgress(busy ? snapshot.fraction : snapshot.fraction ?? 0)
                    if snapshot.prompt != nil { Notice("This transfer is waiting for you. Open it to continue.", tone: .warning) }
                    if let error = snapshot.error, snapshot.lifecycle == .failed { Notice(error, tone: .danger) }
                    if let total = snapshot.totalBytes { Text("\(formatBytes(snapshot.completedBytes)) of \(formatBytes(total))").fbText(.caption, color: theme.textSubtle) }
                    if busy, snapshot.actions.pause { FbButton("Pause", kind: .secondary) { Task { await model.act(on: snapshot, action: .pause) } } }
                    else if snapshot.actions.resume { FbButton("Resume", icon: .arrowRight) { Task { await model.act(on: snapshot, action: .resume) } } }
                }
                .padding(Space.lg)
            }
        }
        .buttonStyle(.pressScale(0.985))
    }
}

private struct CompletedRow: View {
    @Environment(\.theme) private var theme
    let snapshot: TransferSnapshot
    let open: () -> Void
    var body: some View {
        let send = snapshot.direction == .upload
        Button(action: open) {
            Panel(radius: Radius.card) {
                HStack(spacing: Space.md) {
                    FbIcon(send ? .upload : .download, size: 24, tint: theme.accentText).frame(width: 44, height: 44)
                    VStack(alignment: .leading, spacing: Space.xs) {
                        Text(snapshot.kind == .note ? (send ? "Sent note" : "Received note") : send ? "Sent transfer" : "Received transfer").fbText(.bodyStrong).lineLimit(1)
                        Text(snapshot.totalBytes.map { formatBytes($0) } ?? (snapshot.phase == .verifiedAwaitingExport ? "Verified, not saved yet" : "Complete")).fbText(.small, color: theme.textMuted).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    StatusPill(snapshot.phase == .verifiedAwaitingExport ? "Verified" : "Complete", tone: .success)
                }
                .padding(Space.md)
            }
        }
        .buttonStyle(.pressScale(0.985))
    }
}

private struct SavedRow: View {
    @Environment(\.theme) private var theme
    let record: TransferRecord
    let open: () -> Void
    var body: some View {
        Button(action: open) {
            Panel(radius: Radius.card) {
                HStack(spacing: Space.md) {
                    FbIcon(record.isSend ? .upload : .download, size: 24, tint: theme.accentText).frame(width: 44, height: 44)
                    VStack(alignment: .leading, spacing: Space.xs) {
                        Text(record.kind == .note ? (record.isSend ? "Sent note" : "Received note") : record.isSend ? "Sent transfer" : "Received transfer").fbText(.bodyStrong).lineLimit(1)
                        Text(record.expiresAt.map { "Expires \(formatAbsolute($0))" } ?? "Saved on this device").fbText(.small, color: theme.textMuted).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    StatusPill(label(record.phase), tone: tone(record.phase))
                }
                .padding(Space.md)
            }
        }
        .buttonStyle(.pressScale(0.985))
    }
    private func label(_ phase: TransferPhase) -> String {
        switch phase { case .complete, .verifiedAwaitingExport: "Complete"; case .paused: "Paused"; case .failed: "Could not finish"; case .waitingForRecipient: "Live"; default: "Saved" }
    }
    private func tone(_ phase: TransferPhase) -> Tone {
        switch phase { case .complete, .verifiedAwaitingExport: .success; case .failed: .danger; case .paused: .warning; default: .neutral }
    }
}

/// Details and actions for one running or finished job.
private struct JobSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let initial: TransferSnapshot
    @State private var exporting = false
    @State private var preview: URL?
    @State private var activity: TransferActivity?

    private var snapshot: TransferSnapshot { model.snapshot(initial.id) ?? initial }

    var body: some View {
        let snapshot = snapshot
        let send = snapshot.direction == .upload
        FbSheet(title: snapshot.kind == .note ? (send ? "Sent note" : "Received note") : send ? "Sent transfer" : "Received transfer", subtitle: snapshot.phase.label) {
            if let fraction = snapshot.fraction, snapshot.isActive { SmoothProgress(fraction) }
            if let error = snapshot.error, snapshot.lifecycle == .failed { Notice(error, tone: .danger) }
            if let note = snapshot.verifiedNote {
                CodeEditor(note.text, language: note.language.rawValue, minHeight: 96, maxHeight: 300)
                FbButton("Copy note", kind: .secondary, icon: .copy) { Clipboard.copySensitive(note.text); model.toast("Copied", tone: .success) }
            }
            if !snapshot.verifiedFilePaths.isEmpty {
                StatusPill("Integrity verified", tone: .success, icon: .shield)
                ForEach(snapshot.verifiedFilePaths, id: \.self) { path in
                    Button { preview = URL(fileURLWithPath: path) } label: {
                        ItemRow(name: (path as NSString).lastPathComponent, detail: "Tap to preview")
                    }
                    .buttonStyle(.plain)
                }
                FbButton(snapshot.exportStatus == .exported ? "Save again" : "Save to Files", icon: .download) { exporting = true }
                if case let .failedRetained(message) = snapshot.exportStatus { Notice(message, tone: .warning) }
            }
            if let activity, !activity.records.isEmpty {
                Panel {
                    VStack(alignment: .leading, spacing: Space.sm) {
                        Text("Download activity").fbText(.bodyStrong)
                        ForEach(activity.records) { record in
                            HStack { Text(record.state.capitalized).fbText(.small); Spacer(); if let progress = record.progress { Text(progressLabel(progress > 1 ? progress / 100 : progress)).fbText(.numeric, color: theme.textMuted) } }
                        }
                    }
                    .padding(Space.lg)
                }
            }
            VStack(spacing: Space.sm) {
                if snapshot.actions.pause { FbButton("Pause", kind: .secondary) { Task { await model.act(on: snapshot, action: .pause) } } }
                if snapshot.actions.resume { FbButton("Resume", icon: .arrowRight) { Task { await model.act(on: snapshot, action: .resume) } } }
                if snapshot.actions.retryNoteBurn { FbButton("Retry removal", kind: .secondary) { Task { _ = try? await model.service.retryNoteBurn(jobID: snapshot.id) } } }
                if send, snapshot.transferID != nil, snapshot.lifecycle == .complete { FbButton("Share link", kind: .secondary, icon: .link) { Task { await model.act(on: snapshot, action: .receipt) } } }
                if send, snapshot.shareURL != nil, snapshot.kind == .files { FbButton("Download with CLI", kind: .quiet, icon: .terminal) { Task { await model.act(on: snapshot, action: .downloadCommand) } } }
                if snapshot.actions.endLive { FbButton("Stop live transfer", kind: .danger) { confirm("Stop the live transfer?", "Recipients who are still downloading will be cut off.", "Stop live transfer", .endLive) } }
                if snapshot.actions.revokeRemote { FbButton("Delete for everyone", kind: .danger, icon: .trash) { confirm("Delete this transfer for everyone?", "The link stops working immediately. This cannot be undone.", "Delete now", .revokeRemote) } }
                if snapshot.actions.discardLocal { FbButton("Remove from this device", kind: .quiet) { confirm("Remove from this device?", "Local recovery data and verified files for this transfer are deleted. The shared link is not affected.", "Remove", .discardLocal) } }
            }
        }
        .task(id: snapshot.checkpointID) {
            guard let checkpoint = snapshot.checkpointID, send else { return }
            while !Task.isCancelled {
                if let value = try? await model.service.transferActivity(checkpointID: checkpoint) { activity = value }
                try? await Task.sleep(for: .seconds(5))
            }
        }
        .sheet(isPresented: $exporting) {
            DocumentExportSheet(urls: snapshot.verifiedFilePaths.map(URL.init(fileURLWithPath:))) { result in
                exporting = false
                if case let .success(destinations) = result { Task { await model.recordExport(jobID: snapshot.id, paths: destinations.map(\.path)); model.toast("Saved", tone: .success) } }
                else if case let .failure(error) = result { model.fail(error) }
            }
            .ignoresSafeArea()
        }
        .sheet(item: Binding(get: { preview.map(PreviewItem.init) }, set: { if $0 == nil { preview = nil } })) { NativePreviewSheet(url: $0.url).ignoresSafeArea() }
    }

    private func confirm(_ title: String, _ body: String, _ label: String, _ action: TransferAction) {
        dismiss()
        model.present(FbDialogSpec(title: title, body: body, confirmLabel: label, destructive: true) {
            model.dismissDialog()
            Task { await model.act(on: snapshot, action: action) }
        })
    }
}

private struct PreviewItem: Identifiable {
    let url: URL
    init(_ url: URL) { self.url = url }
    var id: String { url.path }
}

/// Details and actions for one saved transfer. Only actions the checkpoint still supports are offered.
private struct RecordSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let record: TransferRecord

    var body: some View {
        FbSheet(title: record.isSend ? "Sent transfer" : "Received transfer", subtitle: record.phase.label) {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text([record.kind == .note ? "Encrypted note" : "Files", record.transport == .webRTC ? "Live WebRTC" : "Stored HTTP"].joined(separator: " · ")).fbText(.body)
                if let expires = record.expiresAt { Text("Expires \(formatAbsolute(expires))").fbText(.small, color: theme.textMuted) }
                if record.verifiedPrivately { StatusPill("Integrity verified", tone: .success, icon: .shield) }
                if case let .failedRetained(message) = record.exportStatus { Notice(message, tone: .warning) }
                if case let .failedNotRetained(message) = record.exportStatus { Notice(message, tone: .danger) }
            }
            if record.actions.resume { FbButton("Resume", icon: .arrowRight) { dismiss(); Task { await model.resume(record: record); model.select(.transfers) } } }
            if record.actions.retryExport { Notice("Verified output remains on this device. Resume the transfer to save it to Files.", tone: .info) }
            if record.actions.endLive { FbButton("Stop live transfer", kind: .secondary) { confirm("Stop the live transfer?", "Recipients who are still downloading will be cut off.", "Stop live transfer") { await model.end(record: record) } } }
            if record.actions.revokeRemote { FbButton("Delete for everyone", kind: .danger, icon: .trash) { confirm("Delete this transfer for everyone?", "The link stops working immediately. This cannot be undone.", "Delete now") { await model.revoke(record: record); model.toast("Transfer deleted", tone: .success) } } }
            if record.actions.discardLocal { FbButton("Remove from this device", kind: .quiet) { confirm("Remove from this device?", "Local recovery data and verified files for this transfer are deleted. The shared link is not affected.", "Remove") { await model.discard(record: record); model.toast("Removed from this device", tone: .success) } } }
        }
    }

    private func confirm(_ title: String, _ body: String, _ label: String, action: @escaping () async -> Void) {
        dismiss()
        model.present(FbDialogSpec(title: title, body: body, confirmLabel: label, destructive: true) {
            model.dismissDialog()
            Task { await action() }
        })
    }
}
