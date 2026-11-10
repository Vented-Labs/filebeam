import Foundation
import Observation
import FilebeamDomain

/// Where an incoming link is on its way from pasted text to saved files or an open note.
enum ReceiveStage: Hashable {
    case idle
    case inspecting
    /// The link carries no key; the sender shared it separately.
    case needKey(error: Bool)
    case needPassword(error: Bool)
    /// A live transfer wants to connect peers directly, which reveals network addresses.
    case needConsent(peer: String?)
    case unlocking(TransferPhase)
    case noteLocked(burnOnRead: Bool)
    case files([TransferSelectionItem])
    case downloading(items: [TransferSelectionItem], selection: Set<String>?)
    case saved([URL])
    case note(VerifiedNote, burnPending: Bool)
    case unavailable(String?)

    var kind: String {
        switch self {
        case .idle: "idle"; case .inspecting: "inspecting"; case .needKey: "key"; case .needPassword: "password"; case .needConsent: "consent"
        case .unlocking: "unlocking"; case .noteLocked: "note-locked"; case .files: "files"; case .downloading: "downloading"; case .saved: "saved"; case .note: "note"; case .unavailable: "unavailable"
        }
    }
}

/// Drives receiving. Files are unlocked first (key, password, consent) so their names and sizes
/// can be shown and chosen before any payload is fetched; notes are decrypted only on request.
/// Keys and passwords stay in memory for the life of one attempt.
@MainActor @Observable
final class ReceiveModel {
    unowned let app: AppModel
    var input = ""
    var stage: ReceiveStage = .idle
    var live = false
    var attachedNote: AttachedNoteDraft?
    var reportSent = false
    var reportBusy = false
    var reportError: String?
    private(set) var jobID: TransferJobID?
    private var link = ""
    private var routeInstance: FilebeamInstance?
    private var transferID: TransferID?
    private var inspection: TransferInspection?
    private var password: String?
    private var passwordAttempts = 0
    private var consent = false
    private var answered: Set<UInt64> = []
    private var work: Task<Void, Never>?

    init(app: AppModel) { self.app = app }

    var canEdit: Bool { if case .idle = stage { true } else if case .unavailable = stage { true } else { false } }
    var snapshot: TransferSnapshot? { jobID.flatMap { app.snapshot($0) } }

    /// Looks the link up on its instance and picks the files or the note path.
    func open(_ value: String? = nil) {
        let trimmed = (value ?? input).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        reset(keepInput: false)
        input = trimmed
        link = trimmed
        stage = .inspecting
        work = Task { [weak self] in
            guard let self else { return }
            do {
                let route = try await app.service.inspectReceive(instance: app.instance, input: trimmed)
                guard case let .transfer(id, instance, normalized) = route else { stage = .unavailable("Enter a transfer link or ID."); return }
                let inspection = try await app.service.inspectPayload(instance: instance, input: normalized)
                guard !Task.isCancelled else { return }
                self.inspection = inspection
                routeInstance = instance
                transferID = id
                link = normalized
                live = inspection.transport == .webRTC
                if inspection.isUnavailable { stage = .unavailable(nil); return }
                guard inspection.kind == .files || inspection.kind == .note else { stage = .unavailable("This transfer type is not supported."); return }
                if !hasKey(link) { stage = .needKey(error: false) }
                else if inspection.passwordRequired && password == nil { stage = .needPassword(error: false) }
                else { await proceed() }
            } catch { stage = .unavailable(readableMessage(error)) }
        }
    }

    private static let keyPattern = /^(?:v1\.)?[A-Za-z0-9_-]{43}$/

    func submitKey(_ key: String) async {
        guard let inspection, let routeInstance else { return }
        let value = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard value.wholeMatch(of: Self.keyPattern) != nil, let combined = try? await app.service.combineKey(instance: routeInstance, link: link, key: value) else {
            stage = .needKey(error: true)
            return
        }
        link = combined
        if inspection.passwordRequired && password == nil { stage = .needPassword(error: false) } else { await proceed() }
    }

    /// Returns to the key step, forgetting the key that was entered.
    func changeKey() {
        cancelWork()
        link = link.split(separator: "#", maxSplits: 1).first.map(String.init) ?? link
        password = nil
        stage = .needKey(error: false)
    }

    func submitPassword(_ value: String) async {
        guard !value.isEmpty else { return }
        password = value
        if let jobID, let prompt = app.snapshot(jobID)?.prompt, prompt.kind == .password {
            answered.insert(prompt.id)
            try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: value)
            stage = .unlocking(.preparing)
            drive()
        } else { await proceed() }
    }

    func submitConsent(_ allowed: Bool) async {
        if !allowed {
            if let jobID, let prompt = app.snapshot(jobID)?.prompt, prompt.kind == .peerConsent {
                try? await app.service.respondToConsent(jobID: jobID, promptID: prompt.id, allowed: false)
            }
            reset(keepInput: true)
            return
        }
        consent = true
        app.appearance.peerExposureAcceptedAt = Date()
        if let jobID, let prompt = app.snapshot(jobID)?.prompt, prompt.kind == .peerConsent {
            answered.insert(prompt.id)
            try? await app.service.respondToConsent(jobID: jobID, promptID: prompt.id, allowed: true)
            stage = .unlocking(.preparing)
            drive()
        } else { await proceed() }
    }

    private func proceed() async {
        guard let inspection else { return }
        if inspection.kind == .note { await inspectNote() } else { await unlockFiles() }
    }

    // MARK: Files

    private func unlockFiles() async {
        guard let routeInstance else { return }
        cancelWork()
        stage = .unlocking(.preparing)
        do {
            let directory = try app.outputFolder()
            let snapshot = try await app.service.receive(.init(instance: routeInstance, input: link, outputDirectory: directory.path))
            jobID = snapshot.id
            app.claim(snapshot.id)
            app.accept(snapshot)
            drive()
        } catch { stage = .unavailable(readableMessage(error)) }
    }

    /// Polls the native job, answering its prompts from what the user already entered.
    private func drive() {
        cancelWork()
        work = Task { [weak self] in
            guard let self, let jobID else { return }
            while !Task.isCancelled {
                await app.refresh(jobID: jobID)
                guard let snapshot = app.snapshot(jobID), !Task.isCancelled else { return }
                if let note = try? await app.service.attachedNote(jobID: jobID) { attachedNote = note }
                if let prompt = snapshot.prompt, !answered.contains(prompt.id) {
                    switch prompt.kind {
                    case .password:
                        if let known = password, passwordAttempts == 0 {
                            answered.insert(prompt.id)
                            passwordAttempts += 1
                            try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: known)
                        } else {
                            let retry = password != nil
                            password = nil
                            passwordAttempts = 0
                            stage = .needPassword(error: retry)
                            return
                        }
                    case .decryptionKey:
                        stage = .needKey(error: true)
                        return
                    case .peerConsent:
                        if consent || app.appearance.recentlyConsentedToPeerExposure {
                            answered.insert(prompt.id)
                            try? await app.service.respondToConsent(jobID: jobID, promptID: prompt.id, allowed: true)
                        } else {
                            stage = .needConsent(peer: prompt.message)
                            return
                        }
                    case .filesSelection:
                        if case .downloading = stage {} else { stage = .files(prompt.selectionItems) }
                        return
                    default:
                        answered.insert(prompt.id)
                        try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: "continue")
                    }
                }
                switch snapshot.lifecycle {
                case .running, .pausing:
                    if case .downloading = stage {} else if case .files = stage {} else { stage = .unlocking(snapshot.phase) }
                case .complete:
                    stage = .saved(snapshot.verifiedFilePaths.map { URL(fileURLWithPath: $0) })
                    Haptics.confirm()
                    await app.refreshTransfers()
                    return
                case .paused:
                    if case .downloading(let items, _) = stage { stage = .files(items) }
                    return
                case .failed:
                    stage = .unavailable(snapshot.error)
                    return
                }
                try? await Task.sleep(for: .milliseconds(400))
            }
        }
    }

    /// Downloads `selection`, or every file when it is nil.
    func download(_ selection: Set<String>?) async {
        guard case let .files(items) = stage, let jobID, let prompt = app.snapshot(jobID)?.prompt, prompt.kind == .filesSelection else { return }
        let ids = selection.map { Array($0).sorted() } ?? items.map(\.id)
        guard let data = try? JSONEncoder().encode(ids), let json = String(data: data, encoding: .utf8) else { return }
        answered.insert(prompt.id)
        stage = .downloading(items: items, selection: selection)
        do {
            try await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: json)
            drive()
        } catch { stage = .unavailable(readableMessage(error)) }
    }

    func cancelDownload() async {
        guard case let .downloading(items, _) = stage, let jobID else { return }
        cancelWork()
        try? await app.service.pause(jobID: jobID)
        await app.refresh(jobID: jobID)
        stage = .files(items)
    }

    // MARK: Notes

    private func inspectNote() async {
        cancelWork()
        stage = .unlocking(.preparing)
        do {
            let note = try await app.service.inspectNote(link)
            stage = .noteLocked(burnOnRead: note.burnOnRead)
        } catch { stage = .unavailable(readableMessage(error)) }
    }

    /// Decrypts the note. For burn-on-read notes this is the read that removes it.
    func decryptNote() async {
        guard let routeInstance else { return }
        let burn: Bool = if case let .noteLocked(value) = stage { value } else { true }
        cancelWork()
        stage = .unlocking(.receiving)
        do {
            let snapshot = try await app.service.startReceiveNote(.init(instance: routeInstance, input: link, separateKey: nil, password: password, burnAcknowledged: burn))
            jobID = snapshot.id
            app.claim(snapshot.id)
            app.accept(snapshot)
            driveNote()
        } catch { stage = .unavailable(readableMessage(error)) }
    }

    private func driveNote() {
        cancelWork()
        work = Task { [weak self] in
            guard let self, let jobID else { return }
            while !Task.isCancelled {
                guard let snapshot = try? await app.service.snapshot(jobID: jobID), !Task.isCancelled else { return }
                app.accept(snapshot)
                if let prompt = snapshot.prompt, !answered.contains(prompt.id) {
                    switch prompt.kind {
                    case .password:
                        if let known = password, passwordAttempts == 0 {
                            answered.insert(prompt.id); passwordAttempts += 1
                            try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: known)
                        } else {
                            let retry = password != nil
                            password = nil; passwordAttempts = 0
                            try? await app.service.pause(jobID: jobID)
                            stage = .needPassword(error: retry)
                            return
                        }
                    case .peerConsent:
                        if consent || app.appearance.recentlyConsentedToPeerExposure {
                            answered.insert(prompt.id)
                            try? await app.service.respondToConsent(jobID: jobID, promptID: prompt.id, allowed: true)
                        } else { stage = .needConsent(peer: prompt.message); return }
                    case .decryptionKey:
                        try? await app.service.pause(jobID: jobID)
                        stage = .needKey(error: true)
                        return
                    default:
                        answered.insert(prompt.id)
                        try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: "continue")
                    }
                }
                switch snapshot.lifecycle {
                case .running, .pausing: stage = .unlocking(snapshot.phase)
                case .complete:
                    if let note = try? await app.service.takeVerifiedNote(jobID: jobID) {
                        let pending = snapshot.actions.retryNoteBurn
                        stage = .note(note, burnPending: pending)
                        Haptics.confirm()
                    } else { stage = .unavailable(nil) }
                    return
                case .paused, .failed:
                    stage = .unavailable(snapshot.error)
                    return
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }

    /// Asks the server again to remove a burn-on-read note whose removal did not go through.
    func retryBurn() async {
        guard let jobID, case let .note(note, _) = stage else { return }
        let removed = (try? await app.service.retryNoteBurn(jobID: jobID)) ?? false
        stage = .note(note, burnPending: !removed)
    }

    // MARK: Reporting

    func report(category: ReportCategory, description: String, email: String?) async {
        guard let routeInstance, let transferID else { return }
        reportBusy = true
        reportError = nil
        do {
            _ = try await app.service.report(.init(instance: routeInstance, transferID: transferID, category: category, description: description, email: email?.nilIfEmpty))
            reportSent = true
        } catch { reportError = readableMessage(error) }
        reportBusy = false
    }

    func clearReport() { reportSent = false; reportError = nil }

    /// Drops everything about the current link, including entered secrets and the opened note.
    func reset(keepInput: Bool = false) {
        cancelWork()
        if let jobID { app.release(jobID) }
        jobID = nil
        link = ""
        routeInstance = nil
        transferID = nil
        inspection = nil
        password = nil
        passwordAttempts = 0
        consent = false
        answered = []
        attachedNote = nil
        live = false
        reportSent = false
        reportError = nil
        if !keepInput { input = "" }
        stage = .idle
    }

    private func cancelWork() { work?.cancel(); work = nil }
    private func hasKey(_ value: String) -> Bool { value.split(separator: "#", maxSplits: 1).dropFirst().first.map { $0.contains("k=") } ?? false }
}
