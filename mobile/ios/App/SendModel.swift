import Foundation
import Observation
import FilebeamDomain

enum SendMode: String, CaseIterable, Hashable { case files, notes }
enum ShareKind: Hashable { case files, liveFiles, note, liveNote, inbox }

/// What the Send tab is showing.
enum SendStage: Hashable {
    case composing
    /// A transfer started from this tab is running or finished; the share view is shown.
    case sharing(kind: ShareKind, turbo: Bool, startedAt: Date)
}

enum RecipientLookup: Hashable { case idle, searching, failed(String) }

/// Why sending is currently blocked, in the order a person should fix things. Quiet blockers show no message.
enum SendBlocker: Hashable {
    case loadingPolicy, policyUnavailable(String?), signInRequired, transportDisabled, nothingSelected, importing
    case tooManyFiles(UInt64), tooLarge(UInt64, webrtcFits: Bool), noteTooLarge(UInt64), attachedNoteInvalid, passwordTooShort, sourceError

    var message: String? {
        switch self {
        case .loadingPolicy, .signInRequired, .nothingSelected, .importing: nil
        case let .policyUnavailable(detail): detail ?? "This instance could not be reached. Check the connection in Settings."
        case .transportDisabled: "This instance has turned off the selected transfer method."
        case let .tooManyFiles(maximum): maximum == 1 ? "This transfer method accepts 1 file at a time." : "This transfer method accepts up to \(maximum) files at a time."
        case let .tooLarge(limit, _): "This encrypted transfer exceeds the \(formatBytes(limit)) limit."
        case let .noteTooLarge(limit): "This note exceeds the \(formatBytes(limit)) limit."
        case .attachedNoteInvalid: "The attached note needs between 1 byte and 64 KiB of text."
        case .passwordTooShort: "Use at least 8 characters."
        case .sourceError: "One of the files can no longer be read."
        }
    }
    var offersWebRTC: Bool { if case .tooLarge(_, true) = self { true } else { false } }
}

let passwordMinimumLength = 8
let passwordMaximumLength = 128
let attachedNoteMaximumBytes = 64 * 1024
let noteTitleMaximumCharacters = 160

/// Owns both composers (files and notes), their protected drafts, and the hand-off to the
/// native engine. Passwords live here in memory only and answer the native prompt.
@MainActor @Observable
final class SendModel {
    unowned let app: AppModel
    var mode: SendMode = .files
    var stage: SendStage = .composing
    /// A one-off line about the last add, e.g. skipped duplicates.
    var queueMessage: String?
    var recipientLookup: RecipientLookup = .idle
    var filePassword: String?
    var notePassword: String?
    var noteBusy = false
    var importing = 0
    private(set) var jobID: TransferJobID?
    private(set) var receipt: ShareReceipt?
    private var answered: Set<UInt64> = []
    private var lookupRevision = 0

    init(app: AppModel) { self.app = app }

    var files: FileDraft {
        get { app.drafts.files }
        set { app.drafts.files = newValue }
    }
    var note: NoteDraft {
        get { app.drafts.note }
        set { app.drafts.note = newValue }
    }
    var snapshot: TransferSnapshot? { jobID.flatMap { app.snapshot($0) } }

    // MARK: Files

    /// Adds picked or shared documents. Exact duplicates are skipped and the selection is capped
    /// at the instance's file-count limit, each with a short explanation.
    func addFiles(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        importing += 1
        Task {
            let imported = await app.importer(urls)
            importing -= 1
            addSources(imported)
        }
    }

    func addSources(_ sources: [FileSource]) {
        let existing = files.sources
        let fresh = sources.filter { candidate in !existing.contains { $0.name == candidate.name && $0.sizeBytes == candidate.sizeBytes } }
        let duplicates = sources.count - fresh.count
        let maximum = app.policy?.driver(for: files.options.transport)?.maximumFileCount.finiteValue
        let room = maximum.map { Int(max(0, Int64($0) - Int64(existing.count))) } ?? fresh.count
        let accepted = Array(fresh.prefix(room))
        if accepted.count < fresh.count, let maximum {
            queueMessage = room == 1 ? "Only 1 more file can be added (maximum \(maximum))." : "Only \(room) more files can be added (maximum \(maximum))."
        } else if duplicates > 0 {
            queueMessage = duplicates == 1 ? "1 duplicate file was already in the queue." : "\(duplicates) duplicate files were already in the queue."
        } else { queueMessage = nil }
        mode = .files
        files.sources += accepted
    }

    func removeSource(_ id: UUID) {
        files.sources.removeAll { $0.id == id }
        queueMessage = nil
    }

    func retrySource(_ source: FileSource) {
        let url = URL(string: source.location).flatMap { $0.isFileURL ? $0 : nil } ?? URL(fileURLWithPath: source.location)
        Task {
            let replacements = await app.importer([url])
            guard let index = files.sources.firstIndex(where: { $0.id == source.id }) else { return }
            files.sources.replaceSubrange(index...index, with: replacements)
        }
    }

    /// Inbox delivery is stored-only, so choosing live sending drops the recipient.
    func setTransport(_ transport: Transport) {
        files.options.transport = transport
        if transport == .webRTC { files.options.recipient = nil; files.options.turbo = false }
    }

    func clearRecipient() {
        lookupRevision += 1
        files.options.recipient = nil
        recipientLookup = .idle
    }

    /// Resolves a username to its inbox key. Sending to a friend forces stored HTTP without a password.
    func chooseRecipient(_ username: String) async {
        let name = username.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "@", with: "").lowercased()
        guard !name.isEmpty else { return }
        lookupRevision += 1
        let revision = lookupRevision
        recipientLookup = .searching
        do {
            let recipient = try await app.service.recipient(instance: app.instance, username: name)
            guard revision == lookupRevision else { return }
            files.options.recipient = recipient
            files.options.transport = .http
            files.options.turbo = false
            files.options.includeKeyInLink = true
            filePassword = nil
            recipientLookup = .idle
            mode = .files
        } catch {
            guard revision == lookupRevision else { return }
            recipientLookup = .failed(readableMessage(error))
        }
    }

    func appendText(_ text: String) {
        mode = .notes
        note.text = note.text.isEmpty ? text : note.text + "\n" + text
    }

    // MARK: Blockers

    func fileBlocker() -> SendBlocker? {
        guard let policy = app.policy else { return app.policyError.map { .policyUnavailable($0) } ?? .loadingPolicy }
        if !policy.anonymousUploads && app.session == nil { return .signInRequired }
        let transport = files.options.transport
        guard policy.enabledTransports.contains(transport), let driver = policy.driver(for: transport) else { return .transportDisabled }
        if files.sources.isEmpty { return .nothingSelected }
        if importing > 0 || files.sources.contains(where: { if case .importing = $0.state { true } else { false } }) { return .importing }
        if files.sources.contains(where: { if case .importFailed = $0.state { true } else { false } }) { return .sourceError }
        let count = files.options.archive ? 1 : UInt64(files.sources.count)
        if let maximum = driver.maximumFileCount.finiteValue, count > maximum { return .tooManyFiles(maximum) }
        if let maximum = driver.maximumTransferBytes.finiteValue, !files.options.archive, files.sources.allSatisfy({ $0.sizeBytes != nil }), policy.chunkBytes > 0 {
            if let estimated = SendPolicy.ciphertextBytes(fileSizes: files.sources.compactMap(\.sizeBytes), chunkBytes: policy.chunkBytes), estimated > maximum {
                let other = policy.driver(for: .webRTC)?.maximumTransferBytes
                let fits = transport == .http && policy.enabledTransports.contains(.webRTC) && files.options.recipient == nil && (other?.finiteValue.map { estimated <= $0 } ?? true)
                return .tooLarge(maximum, webrtcFits: fits)
            }
        }
        if let password = filePassword, !(passwordMinimumLength...passwordMaximumLength).contains(password.count) { return .passwordTooShort }
        if let attached = files.options.attachedNote, attached.text.isEmpty || attached.text.utf8.count > attachedNoteMaximumBytes || attached.title.count > noteTitleMaximumCharacters { return .attachedNoteInvalid }
        return nil
    }

    func noteBlocker() -> SendBlocker? {
        guard let policy = app.policy else { return app.policyError.map { .policyUnavailable($0) } ?? .loadingPolicy }
        if !policy.anonymousUploads && app.session == nil { return .signInRequired }
        let transport: Transport = note.options.live ? .webRTC : .http
        guard policy.enabledTransports.contains(transport), let driver = policy.driver(for: transport) else { return .transportDisabled }
        if note.text.isEmpty { return .nothingSelected }
        if let maximum = driver.maximumNoteBytes.finiteValue, UInt64(note.text.utf8.count) > maximum { return .noteTooLarge(maximum) }
        if let password = notePassword, !(passwordMinimumLength...passwordMaximumLength).contains(password.count) { return .passwordTooShort }
        return nil
    }

    // MARK: Sending

    /// Starts the upload and switches the tab to the share view.
    func sendFiles(turbo: Bool) async {
        guard fileBlocker() == nil, let policy = app.policy else { return }
        let live = files.options.transport == .webRTC
        let recipient = files.options.recipient
        var options = files.options
        options.turbo = turbo && !live && recipient == nil
        options.passwordEnabled = filePassword != nil && recipient == nil
        options.retentionHours = options.retentionHours ?? policy.fileDefaultRetentionHours
        options.includeKeyInLink = options.includeKeyInLink || recipient != nil
        if options.attachedNote?.text.isEmpty == true { options.attachedNote = nil }
        answered = []
        receipt = nil
        do {
            let snapshot = try await app.service.startUpload(.init(instance: app.instance, paths: files.sources.map(\.location), sources: files.sources, options: options, password: options.passwordEnabled ? filePassword : nil))
            jobID = snapshot.id
            app.rememberKeyChoice(options.includeKeyInLink, for: snapshot.id)
            app.claim(snapshot.id)
            app.accept(snapshot)
            let kind: ShareKind = recipient != nil ? .inbox : live ? .liveFiles : .files
            stage = .sharing(kind: kind, turbo: options.turbo, startedAt: Date())
            Haptics.tap()
        } catch { app.fail(error) }
    }

    func sendNote() async {
        guard noteBlocker() == nil, let policy = app.policy else { return }
        var draft = note
        draft.options.passwordEnabled = notePassword != nil
        draft.options.retentionHours = draft.options.retentionHours ?? policy.noteDefaultRetentionHours
        let live = draft.options.live
        answered = []
        receipt = nil
        noteBusy = true
        stage = .sharing(kind: live ? .liveNote : .note, turbo: false, startedAt: Date())
        do {
            let snapshot = try await app.service.startNote(.init(instance: app.instance, draft: draft, password: notePassword))
            jobID = snapshot.id
            app.rememberKeyChoice(draft.options.includeKeyInLink, for: snapshot.id)
            app.claim(snapshot.id)
            app.accept(snapshot)
            await sync()
        } catch {
            stage = .composing
            app.fail(error)
        }
        noteBusy = false
    }

    /// Answers prompts this screen already has the answer to, so the user is not asked twice, and
    /// fetches the receipt once the share exists.
    func sync() async {
        guard let jobID, case let .sharing(kind, _, _) = stage else { return }
        await app.refresh(jobID: jobID)
        guard let snapshot = app.snapshot(jobID) else { return }
        if let prompt = snapshot.prompt, !answered.contains(prompt.id) {
            switch prompt.kind {
            case .password:
                if let password = kind == .note || kind == .liveNote ? notePassword : filePassword {
                    answered.insert(prompt.id)
                    try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: password)
                }
            case .directoryChoice:
                answered.insert(prompt.id)
                try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: files.options.archive ? "zip" : "individual")
            case .shareReady:
                answered.insert(prompt.id)
                try? await app.service.respondToPrompt(jobID: jobID, promptID: prompt.id, value: "continue")
            case .peerConsent:
                if app.appearance.recentlyConsentedToPeerExposure {
                    answered.insert(prompt.id)
                    try? await app.service.respondToConsent(jobID: jobID, promptID: prompt.id, allowed: true)
                }
            default: break
            }
        }
        if receipt == nil, snapshot.lifecycle == .complete, let transferID = snapshot.transferID {
            let include = (kind == .note || kind == .liveNote) ? note.options.includeKeyInLink : files.options.includeKeyInLink
            receipt = try? await app.service.receipt(transferID: transferID, includeKey: include)
        }
    }

    func respondConsent(_ allowed: Bool) async {
        guard let jobID, let prompt = app.snapshot(jobID)?.prompt, prompt.kind == .peerConsent else { return }
        answered.insert(prompt.id)
        if allowed { app.appearance.peerExposureAcceptedAt = Date() }
        try? await app.service.respondToConsent(jobID: jobID, promptID: prompt.id, allowed: allowed)
        if !allowed { await cancelUpload() }
    }

    /// Stops the running upload. A cancelled upload returns to the composer with the draft intact.
    func cancelUpload() async {
        if let jobID { try? await app.service.pause(jobID: jobID); await app.refresh(jobID: jobID) }
        stage = .composing
    }

    func endLive() async {
        guard let jobID, let snapshot = app.snapshot(jobID) else { return }
        await app.act(on: snapshot, action: .endLive)
    }

    func deleteShare() async {
        guard let jobID, let snapshot = app.snapshot(jobID) else { return }
        await app.act(on: snapshot, action: .revokeRemote)
    }

    /// Leaves the share view. The sent draft is cleared; options such as retention are kept.
    func newTransfer() {
        if case let .sharing(kind, _, _) = stage {
            switch kind {
            case .note, .liveNote:
                note.title = ""; note.text = ""; notePassword = nil
            default:
                files.sources = []; files.options.attachedNote = nil; files.options.recipient = nil; filePassword = nil; queueMessage = nil
            }
        }
        if let jobID { app.release(jobID) }
        jobID = nil
        receipt = nil
        stage = .composing
    }

    /// Returns to the composer after a failed send, keeping everything that was selected.
    func backToComposer() {
        if let jobID { app.release(jobID) }
        stage = .composing
    }
}
