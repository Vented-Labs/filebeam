import SwiftUI
import FilebeamDomain

private let historyStatuses: [(String, String)] = [
    ("pending", "Uploading"), ("available", "Available"), ("live", "Live"), ("ended", "Ended"), ("deleting", "Awaiting cleanup"),
    ("expired", "Expired"), ("deleted", "Deleted"), ("abandoned", "Abandoned upload"), ("burned", "Burned after reading"), ("removed", "Removed"),
]

private func statusTone(_ status: String) -> Tone {
    switch status {
    case "available", "live": .success
    case "pending": .accent
    case "deleting", "abandoned": .warning
    case "expired", "deleted", "burned", "removed": .danger
    default: .neutral
    }
}

/// Sent transfers recorded for this account: filter, extend retention, or delete for everyone.
struct HistoryView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var status: String?
    @State private var kind: String?
    @State private var driver: String?
    @State private var entries: [HistoryEntry] = []
    @State private var cursor: String?
    @State private var loading = true
    @State private var loadingMore = false
    @State private var error: String?
    @State private var revision = 0
    @State private var extending: HistoryEntry?

    var body: some View {
        SubScreen("Transfer history", actions: { IconAction(.refresh, "Refresh") { revision += 1 } }) {
            VStack(alignment: .leading, spacing: Space.sm) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: Space.sm) {
                        FilterChip("All", selected: kind == nil) { kind = nil }
                        FilterChip("Files", selected: kind == "files", icon: .folder) { kind = "files" }
                        FilterChip("Notes", selected: kind == "note", icon: .note) { kind = "note" }
                        FilterChip("Stored", selected: driver == "http") { driver = driver == "http" ? nil : "http" }
                        FilterChip("Live", selected: driver == "webrtc") { driver = driver == "webrtc" ? nil : "webrtc" }
                    }
                }
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: Space.sm) {
                        FilterChip("Any status", selected: status == nil) { status = nil }
                        ForEach(historyStatuses, id: \.0) { key, label in FilterChip(label, selected: status == key) { status = key } }
                    }
                }
                HStack(spacing: Space.sm) {
                    FbIcon(.shield, size: 16)
                    Text("Names and share-link keys stay on your device.").fbText(.caption, color: theme.textMuted)
                }
            }
            if let error {
                Notice(error, tone: .danger) { FbButton("Retry", kind: .secondary, height: 40, fill: false) { revision += 1 } }
            }
            if loading && entries.isEmpty {
                CenteredSpinner()
            } else if entries.isEmpty && error == nil {
                EmptyState(.clock, "No transfers to show", body: "Transfers you send while signed in appear here.") {
                    FbButton("Send a transfer", icon: .upload, fill: false) { model.select(.send) }
                }
            } else {
                ForEach(entries) { entry in
                    HistoryCard(entry: entry, onExtend: { extending = entry }, onDelete: { delete(entry) })
                        .transition(.opacity.combined(with: .move(edge: .top)))
                }
                if cursor != nil { FbButton("Older transfers", kind: .secondary, loading: loadingMore) { Task { await loadMore() } } }
                Text("Removed transfers remain in your history for 90 days.").fbText(.caption, color: theme.textSubtle)
            }
        }
        .motion(Motion.spatial, value: entries.map(\.id))
        .task(id: "\(status ?? "")|\(kind ?? "")|\(driver ?? "")|\(revision)") { await load() }
        .sheet(item: $extending) { ExtendSheet(entry: $0) { revision += 1 } }
        .onChange(of: model.session == nil, initial: true) { _, signedOut in if signedOut { model.replace(.auth(.login)) } }
    }

    private var filter: HistoryFilter { HistoryFilter(status: status, kind: kind, driver: driver) }

    private func load() async {
        loading = true
        error = nil
        do {
            let page = try await model.service.accountHistory(instance: model.instance, filter: filter, cursor: nil, limit: 20)
            entries = page.entries
            cursor = page.nextCursor
        } catch { self.error = readableMessage(error) }
        loading = false
    }

    private func loadMore() async {
        loadingMore = true
        do {
            let page = try await model.service.accountHistory(instance: model.instance, filter: filter, cursor: cursor, limit: 20)
            entries += page.entries.filter { next in !entries.contains { $0.id == next.id } }
            cursor = page.nextCursor
        } catch { self.error = readableMessage(error) }
        loadingMore = false
    }

    private func delete(_ entry: HistoryEntry) {
        model.present(FbDialogSpec(title: "Delete this transfer for everyone?", body: "The link stops working immediately. This cannot be undone.", confirmLabel: "Delete", destructive: true) {
            model.dismissDialog()
            Task {
                do { try await model.service.deleteHistory(instance: model.instance, transferID: TransferID(entry.id)); model.toast("Transfer deleted", tone: .success) }
                catch { model.fail(error) }
                revision += 1
            }
        })
    }
}

private struct HistoryCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let entry: HistoryEntry
    let onExtend: () -> Void
    let onDelete: () -> Void

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.md) {
                    FbIcon(entry.isNote ? .note : .folder, size: 22, tint: theme.accentText)
                    Text(entry.isNote ? "Encrypted note" : entry.itemCount == 1 ? "1 encrypted file" : "\(entry.itemCount) encrypted files").fbText(.bodyStrong).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                    StatusPill(historyStatuses.first { $0.0 == entry.status }?.1 ?? "Ended", tone: statusTone(entry.status))
                }
                HStack {
                    Text(entry.id).fbText(.codeSmall, color: theme.textMuted).lineLimit(1).truncationMode(.middle).frame(maxWidth: .infinity, alignment: .leading)
                    IconAction(.copy, "Copy transfer ID", size: 36) { Clipboard.copySensitive(entry.id); model.toast("Copied", tone: .success) }
                }
                Text([entry.driver == "webrtc" ? "Live WebRTC" : "Stored HTTP", entry.delivery == "inbox" ? "Inbox" : "Link share", entry.burnOnRead ? "Burn on read" : nil].compactMap { $0 }.joined(separator: " · ") + " · " + formatBytes(entry.ciphertextBytes)).fbText(.small, color: theme.textMuted)
                VStack(alignment: .leading, spacing: 2) {
                    if let created = entry.createdAt { Text("Created \(formatAbsolute(created))").fbText(.caption, color: theme.textSubtle) }
                    if let removed = entry.removedAt { Text("Removed \(formatAbsolute(removed))").fbText(.caption, color: theme.textSubtle) }
                    else if let expires = entry.expiresAt { Text("\(expires > Date() ? "Expires" : "Expired") \(formatAbsolute(expires))").fbText(.caption, color: theme.textSubtle) }
                }
                if entry.canExtend || entry.canDelete {
                    HStack(spacing: Space.md) {
                        if entry.canExtend { FbButton("Extend", kind: .secondary, icon: .clock, height: 44, action: onExtend) }
                        if entry.canDelete { FbButton("Delete", kind: .danger, icon: .trash, height: 44, action: onDelete) }
                    }
                }
            }
            .padding(Space.lg)
        }
    }
}

private struct ExtendSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let entry: HistoryEntry
    let done: () -> Void
    @State private var hours: String
    @State private var busy = false
    @State private var failure: String?

    init(entry: HistoryEntry, done: @escaping () -> Void) {
        self.entry = entry; self.done = done
        _hours = State(initialValue: String(min(entry.retentionHours + 24, entry.maximumRetentionHours)))
    }

    var body: some View {
        let value = UInt64(hours)
        let valid = value.map { $0 > entry.retentionHours && $0 <= entry.maximumRetentionHours } ?? false
        FbSheet(title: "Extend retention", subtitle: "Currently kept for \(formatRetention(hours: entry.retentionHours)). This plan allows up to \(formatRetention(hours: entry.maximumRetentionHours)) in total.") {
            FbTextField($hours, label: "Total retention in hours", supporting: valid && entry.createdAt != nil ? "New expiry: \(formatAbsolute(entry.createdAt!.addingTimeInterval(Double(value!) * 3600)))" : nil, error: failure, keyboard: .numberPad)
                .onChange(of: hours) { _, text in hours = String(text.filter(\.isNumber).prefix(6)) }
            if entry.driver == "webrtc" { Text("Live transfers also end when the sender stops sharing.").fbText(.caption, color: theme.textMuted) }
            FbButton("Extend", enabled: valid, loading: busy) {
                busy = true
                failure = nil
                Task {
                    do {
                        _ = try await model.service.extendHistory(instance: model.instance, transferID: TransferID(entry.id), retentionHours: value ?? 0)
                        model.toast("Retention extended", tone: .success)
                        done()
                        dismiss()
                    } catch { failure = readableMessage(error) }
                    busy = false
                }
            }
        }
    }
}
