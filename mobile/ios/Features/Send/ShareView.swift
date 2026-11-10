import SwiftUI
import FilebeamDomain

/// The Send tab while a transfer started here is in flight or finished.
struct ShareView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var completedAt: Date?
    @State private var pulsed = false

    var body: some View {
        let send = model.send
        guard case let .sharing(kind, turbo, _) = send.stage else { return AnyView(EmptyView()) }
        let snapshot = send.snapshot
        let note = kind == .note || kind == .liveNote
        let live = kind == .liveFiles || kind == .liveNote
        let link = send.receipt?.link ?? snapshot?.shareURL
        let separateKey = send.receipt?.separateKey
        let running = note && !live ? send.noteBusy : (snapshot?.lifecycle == .running || snapshot?.lifecycle == .pausing)
        let failed = !running && snapshot?.lifecycle == .failed
        let paused = !running && snapshot?.lifecycle == .paused
        let complete = snapshot?.lifecycle == .complete
        let hasLink = link != nil
        let consent = snapshot?.prompt?.kind == .peerConsent
        let fraction = snapshot?.fraction
        let hours: UInt64 = note ? (send.note.options.retentionHours ?? model.policy?.noteDefaultRetentionHours ?? 720) : (send.files.options.retentionHours ?? model.policy?.fileDefaultRetentionHours ?? 24)

        return AnyView(TabScreen {
            Panel {
                VStack(spacing: Space.lg) {
                    ShareBadge(running: running, done: hasLink || complete, failed: failed)
                    if turbo || live { Text(turbo ? "Turbo Transfer" : "Live transfer").fbText(.eyebrow, color: theme.accentText) }
                    let title: String = failed ? "The transfer could not finish" : paused ? "Transfer paused" : kind == .inbox && complete ? "Files sent" : kind == .inbox ? "Sending to inbox" : hasLink && live ? "Your live transfer is ready" : hasLink ? "Your encrypted link is ready" : (snapshot?.phase ?? .encrypting).label
                    Text(title).fbText(.headline).multilineTextAlignment(.center).id(title)
                        .transition(.opacity.combined(with: .move(edge: .bottom)))
                        .accessibilityAddTraits(.updatesFrequently)
                    let subtitle: String = failed ? readableMessage(snapshot?.error ?? "Nothing was shared. Your selection is still in the composer.")
                        : paused ? "Resume it from Transfers, or go back to change the selection."
                        : kind == .inbox ? "Encrypted for @\(send.files.options.recipient?.username ?? ""). Only their account key can open them."
                        : live && hasLink ? "Keep Filebeam open while recipients download. No payload is stored."
                        : turbo && running && hasLink ? "Uploading is still in progress. Keep Filebeam open until it finishes."
                        : hasLink && separateKey != nil && (send.filePassword != nil || send.notePassword != nil) ? "Share the generated key and the password separately."
                        : hasLink && separateKey != nil ? "Share the generated key separately."
                        : hasLink ? "The key stays in the link fragment and is never sent to the server."
                        : "Your files are being encrypted on this device."
                    Text(subtitle).fbText(.small, color: failed ? theme.danger.ink : theme.textMuted).multilineTextAlignment(.center)

                    if consent {
                        ConsentPanel(peer: snapshot?.prompt?.message) { allowed in Task { await send.respondConsent(allowed) } }
                    } else if running && !(note && !live) {
                        VStack(spacing: Space.sm) {
                            HStack {
                                Text((snapshot?.phase ?? .preparing).label).fbText(.small, color: theme.textMuted).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                                if let fraction { Text(progressLabel(fraction)).fbText(.numeric, color: theme.textMuted) }
                            }
                            SmoothProgress(live && hasLink ? nil : fraction)
                        }
                        .transition(.opacity)
                    }

                    if hasLink, kind != .inbox, let link {
                        VStack(spacing: Space.md) {
                            CopyField(icon: .link, value: link, label: "Copy link", primary: true) {
                                Clipboard.copySensitive(link)
                                model.toast("Link copied", tone: .success)
                            }
                            if let separateKey {
                                CopyField(icon: .key, value: separateKey, label: "Copy key") {
                                    Clipboard.copySensitive(separateKey)
                                    model.toast("Key copied", tone: .success)
                                }
                            }
                            ShareLink(item: link) {
                                HStack(spacing: Space.sm) { FbIcon(.arrowUpRight, size: 20, tint: theme.text); Text("Share link").fbText(.label) }
                                    .frame(maxWidth: .infinity, minHeight: Space.control)
                                    .background(theme.surface, in: Shapes.control)
                                    .overlay(Shapes.control.strokeBorder(theme.controlBorder, lineWidth: 1))
                            }
                            .buttonStyle(.pressScale)
                        }
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                    }
                    if hasLink && kind != .inbox && !live {
                        if complete, let completedAt { ExpiryChip(expiresAt: completedAt.addingTimeInterval(TimeInterval(hours * 3600))) }
                        else if turbo { Text("Retention starts when uploading finishes.").fbText(.caption, color: theme.textSubtle) }
                    }
                    if note && send.note.options.burnOnRead && hasLink { StatusPill("Burns after the first read", tone: .warning, icon: .trash) }
                }
                .frame(maxWidth: .infinity)
                .padding(.horizontal, Space.xl)
                .padding(.vertical, Space.xxl)
                .motion(Motion.spatial, value: hasLink)
                .motion(Motion.spatial, value: running)
            }
            .privacySensitive(hasLink)
            .staggered(0)

            if hasLink, kind != .inbox, let link {
                CliCard(link: link, available: !note && !live, reason: note ? "Notes are opened in the app or the browser." : "Live transfers are received in the app or the browser.").staggered(1)
            }
            if (turbo || live), hasLink, !note, let checkpoint = snapshot?.checkpointID {
                DownloadActivity(checkpointID: checkpoint, live: live).staggered(2)
            }

            VStack(spacing: Space.md) {
                if failed || paused {
                    FbButton("Back to composer", icon: .arrowLeft) { send.backToComposer() }
                } else if running {
                    if hasLink || complete { FbButton("New transfer", kind: .secondary, icon: .plus, enabled: !turbo && !live) { send.newTransfer() } }
                    FbButton(live ? "Stop live transfer" : "Cancel upload", kind: .danger) {
                        if live {
                            model.present(FbDialogSpec(title: "Stop the live transfer?", body: "Recipients who are still downloading will be cut off.", confirmLabel: "Stop live transfer", destructive: true) { model.dismissDialog(); Task { await send.endLive(); send.newTransfer() } })
                        } else {
                            model.present(FbDialogSpec(title: "Cancel this upload?", body: "Uploading stops and your selection returns to the composer.", confirmLabel: "Cancel upload", dismissLabel: "Keep uploading", destructive: true) { model.dismissDialog(); Task { await send.cancelUpload() } })
                        }
                    }
                } else {
                    FbButton(kind == .inbox ? "Send more files" : "New transfer", icon: .plus) { send.newTransfer() }
                    if !note, kind != .inbox, snapshot?.actions.revokeRemote == true {
                        FbButton("Delete now", kind: .danger, icon: .trash) {
                            model.present(FbDialogSpec(title: "Delete this transfer for everyone?", body: "The link stops working immediately. This cannot be undone.", confirmLabel: "Delete now", destructive: true) {
                                model.dismissDialog()
                                Task { await send.deleteShare(); model.toast("Transfer deleted", tone: .success); send.newTransfer() }
                            })
                        }
                    }
                }
            }
            .staggered(3)
        }
        .task(id: send.jobID) {
            while !Task.isCancelled {
                await send.sync()
                try? await Task.sleep(for: .milliseconds(600))
            }
        }
        .onChange(of: complete, initial: true) { _, done in if done, completedAt == nil { completedAt = Date() } }
        .onChange(of: hasLink, initial: true) { _, ready in if ready, !pulsed { pulsed = true; Haptics.confirm() } })
    }
}

/// The round status emblem above the share title: a ring while working, a lock once protected.
private struct ShareBadge: View {
    @Environment(\.theme) private var theme
    let running: Bool
    let done: Bool
    let failed: Bool

    var body: some View {
        ZStack {
            Circle().fill(RadialGradient(colors: [theme.selectedSurface, theme.accentSurface], center: .center, startRadius: 0, endRadius: 40))
            Circle().strokeBorder(failed ? theme.danger.border : done ? theme.actionBorder : theme.border, lineWidth: 1)
            Group {
                if failed { FbIcon(.alert, size: 30, tint: theme.danger.ink) }
                else if done { FbIcon(.lock, size: 32, tint: theme.accentText) }
                else if running { Spinner(size: 30, stroke: 2.5) }
                else { FbIcon(.clock, size: 30, tint: theme.accentText) }
            }
            .id("\(failed)-\(done)-\(running)")
            .transition(.scale(scale: 0.4).combined(with: .opacity))
        }
        .frame(width: 76, height: 76)
        .scaleEffect(done ? 1 : 0.92)
        .motion(Motion.bouncy, value: done)
        .motion(Motion.bouncy, value: failed)
        .accessibilityHidden(true)
    }
}

/// A live transfer wants to connect peers directly, which reveals network addresses.
struct ConsentPanel: View {
    @Environment(\.theme) private var theme
    let peer: String?
    let answer: (Bool) -> Void
    var body: some View {
        VStack(spacing: Space.md) {
            Text("Connect directly?").fbText(.title)
            Text("Live transfers connect your device to the other side. Each side may see the other's network address.").fbText(.small, color: theme.textMuted).multilineTextAlignment(.center)
            if let peer { Text(peer).fbText(.codeSmall, color: theme.textMuted) }
            Notice("Turn on relay-only live transfers in Settings to hide your address.", tone: .info)
            FbButton("Allow and connect") { answer(true) }
            FbButton("Not now", kind: .quiet) { answer(false) }
        }
        .transition(.opacity.combined(with: .scale(scale: 0.98)))
    }
}

/// Who is downloading a Turbo or live transfer right now. Polls gently and backs off when idle.
struct DownloadActivity: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let checkpointID: String
    let live: Bool
    @State private var records: [TransferActivityRecord] = []

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack {
                    Text(live ? "Recipient activity" : "Download activity").fbText(.bodyStrong).frame(maxWidth: .infinity, alignment: .leading)
                    Text("Anonymous").fbText(.caption, color: theme.textMuted)
                }
                if records.isEmpty {
                    Text("Download activity will appear here.").fbText(.small, color: theme.textMuted)
                } else {
                    ForEach(Array(records.enumerated()), id: \.element.id) { index, record in
                        VStack(alignment: .leading, spacing: Space.xs) {
                            HStack(spacing: Space.sm) {
                                Text("Download \(record.number ?? UInt64(index + 1))").fbText(.smallStrong).frame(maxWidth: .infinity, alignment: .leading)
                                if record.allFiles == true { Text("All files").fbText(.caption, color: theme.textMuted) }
                                else if let count = record.selectionCount { Text(count == 1 ? "1 file" : "\(count) files").fbText(.caption, color: theme.textMuted) }
                                StatusPill(label(record.state), tone: tone(record.state))
                            }
                            if let progress = record.progress { SmoothProgress(progress > 1 ? progress / 100 : progress, height: 4) }
                        }
                    }
                }
            }
            .padding(Space.lg)
        }
        .task(id: checkpointID) {
            var wait: Double = 2
            while !Task.isCancelled {
                if let activity = try? await model.service.transferActivity(checkpointID: checkpointID) {
                    wait = activity.records == records ? min(wait * 1.5, 15) : 2
                    records = activity.records
                }
                try? await Task.sleep(for: .seconds(wait))
            }
        }
    }

    private func label(_ state: String) -> String {
        switch state {
        case "downloading": "Downloading"; case "waiting": "Waiting to resume"; case "verifying": "Verifying"; case "completed": "Completed"
        case "cancelled": "Cancelled"; case "failed": "Could not finish"; default: "Connection lost"
        }
    }
    private func tone(_ state: String) -> Tone {
        switch state { case "completed": .success; case "failed", "cancelled": .danger; case "downloading", "verifying": .accent; default: .neutral }
    }
}
