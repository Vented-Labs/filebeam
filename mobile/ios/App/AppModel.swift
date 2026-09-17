import Foundation
import Observation
import FilebeamDomain

@MainActor @Observable
final class AppModel {
    enum Tab: Hashable { case send, receive, transfers, inbox, settings }

    let service: any FilebeamService
    private(set) var instance: FilebeamInstance
    private let outputDirectory: () throws -> URL
    private let loadDrafts: () async throws -> ComposerDrafts?
    private let saveDrafts: (ComposerDrafts) async throws -> Void
    private let saveSettings: (AppSettings) async throws -> Void
    private var settings: AppSettings
    private var draftsByOrigin: [String: ComposerDrafts] = [:]
    var selectedTab: Tab = .send
    var drafts = ComposerDrafts()
    var policy: InstancePolicy?
    var session: AccountSession?
    var transfers: [TransferSnapshot] = []
    var records: [TransferRecord] = []
    var inboxItems: [InboxItem] = []
    var inboxUnreadCount: UInt64 = 0
    var inboxError: String?
    var operationError: String?
    var receiveInput = ""
    var receiveError: String?
    var receiveJob: TransferSnapshot?
    var activePrompt: PromptContext?
    var pendingReceiveRoute: ReceiveRouteConfirmation?
    var pendingNoteInspection: NoteInspection?
    var pendingNoteInput: String?
    var pendingNoteInstance: FilebeamInstance?
    var externalRequests = ExternalRequestCoordinator()
    var stagedSharedFiles: [FileSource]?
    var stagedSharedText: (text: String, links: [String])?
    var pendingInviteLink: String?
    var pendingResetLink: String?
    var pendingVerificationLink: String?
    var receipt: ShareReceipt?
    var downloadCommand: String?
    var isLoading = false
    var isStartingFileTransfer = false
    var isStartingNote = false
    var pendingInstance: FilebeamInstance?
    var instanceChangeError: String?
    var pollingTask: Task<Void, Never>?
    private var shareKeyChoices: [TransferJobID: Bool] = [:]
    private var promptsByJob: [TransferJobID: TransferPrompt] = [:]
    private var verifiedNotes: [TransferJobID: VerifiedNote] = [:]

    init(service: any FilebeamService, instance: FilebeamInstance, outputDirectory: @escaping () throws -> URL, loadDrafts: @escaping () async throws -> ComposerDrafts? = { nil }, saveDrafts: @escaping (ComposerDrafts) async throws -> Void = { _ in }, settings: AppSettings? = nil, saveSettings: @escaping (AppSettings) async throws -> Void = { _ in }) {
        self.service = service
        self.instance = instance
        self.outputDirectory = outputDirectory
        self.loadDrafts = loadDrafts
        self.saveDrafts = saveDrafts
        self.settings = settings ?? AppSettings(instanceOrigin: instance.origin)
        self.saveSettings = saveSettings
    }

    deinit {}

    func load() async {
        isLoading = true
        defer { isLoading = false }
        do { if let stored = try await loadDrafts() { drafts = stored; draftsByOrigin[instance.origin] = stored } }
        catch { operationError = message(error) }
        await refreshPolicy()
        await refreshSession()
        await refreshTransfers()
    }

    func persistDrafts(_ value: ComposerDrafts) async {
        draftsByOrigin[instance.origin] = value
        do { try await saveDrafts(value) }
        catch { operationError = message(error) }
    }

    func refreshPolicy() async {
        let origin = instance
        do {
            let discovered = try await service.discover(instance: origin)
            if instance == origin { policy = discovered }
        } catch { if instance == origin { operationError = message(error) } }
    }

    func stageInstance(origin: String) {
        instanceChangeError = nil
        guard let candidate = FilebeamInstance(origin: origin) else {
            instanceChangeError = "Enter a canonical HTTPS instance URL."
            return
        }
        guard candidate != instance else { pendingInstance = nil; return }
        pendingInstance = candidate
    }

    func confirmInstanceChange() async -> Bool {
        guard let candidate = pendingInstance else { return false }
        draftsByOrigin[instance.origin] = drafts
        do {
            var updated = settings
            updated.instanceOrigin = candidate.origin
            try await saveSettings(updated)
            settings = updated
            instance = candidate
            pendingInstance = nil
            session = nil
            inboxItems = []
            inboxUnreadCount = 0
            inboxError = nil
            policy = nil
            drafts = draftsByOrigin[candidate.origin] ?? ComposerDrafts()
            await refreshPolicy()
            await refreshSession()
            return true
        } catch {
            instanceChangeError = message(error)
            return false
        }
    }

    func cancelInstanceChange() { pendingInstance = nil }

    func setRelayOnly(_ enabled: Bool) async -> Bool {
        guard settings.relayOnly != enabled else { return true }
        do {
            var updated = settings
            updated.relayOnly = enabled
            try await saveSettings(updated)
            settings = updated
            return true
        } catch {
            instanceChangeError = message(error)
            return false
        }
    }

    var relayOnly: Bool { settings.relayOnly }

    func refreshSession() async {
        let origin = instance
        do {
            let restored = try await service.session(instance: origin)
            if instance == origin { session = restored }
        } catch { if instance == origin { session = nil } }
    }

    func refreshTransfers() async {
        do { records = try await service.savedTransfers() }
        catch { operationError = message(error) }
    }

    func importSources(_ urls: [URL], importer: @escaping ([URL]) async -> [FileSource]) {
        Task {
            let sources = await importer(urls)
            drafts.files.sources.append(contentsOf: sources)
        }
    }

    func retrySource(_ source: FileSource, importer: @escaping ([URL]) async -> [FileSource]) {
        Task {
            let origin = instance
            guard drafts.files.sources.contains(where: { $0.id == source.id }) else { return }
            let url = URL(string: source.location) ?? URL(fileURLWithPath: source.location)
            let replacements = await importer([url])
            guard instance == origin, let index = drafts.files.sources.firstIndex(where: { $0.id == source.id }) else { return }
            drafts.files.sources.replaceSubrange(index...index, with: replacements)
        }
    }

    func enqueueExternal(_ request: ExternalRequest) { externalRequests.enqueue(request) }

    func processPendingSharedDrafts(root: URL, importer: @escaping ([URL]) async -> [FileSource]) async {
        do {
            let inbox = try SharedDraftInbox(groupURL: root)
            let pending = try inbox.pending()
            let drafts = try pending.map { draft in
                guard let current = try inbox.peek(draft.id) else { throw FilebeamDomainError.storage("A shared draft disappeared before it could be imported.") }
                return current
            }
            guard drafts.allSatisfy({ $0.failure == nil }) else {
                operationError = "A shared item could not be staged. Re-select it in the originating app."
                return
            }
            var imported: [[FileSource]] = []
            for draft in drafts {
                let sources = await importer(draft.files)
                guard sources.count == draft.files.count, sources.allSatisfy({ if case .ready = $0.state { return true }; return false }) else {
                    operationError = "Shared files are still unavailable. They were not removed from the share inbox."
                    return
                }
                imported.append(sources)
            }
            for (index, draft) in drafts.enumerated() {
                if !imported[index].isEmpty { enqueueExternal(.sharedFiles(imported[index])) }
                let text = [draft.text, draft.links.isEmpty ? nil : draft.links.joined(separator: "\n")].compactMap { $0 }.joined(separator: draft.text?.isEmpty == false ? "\n\n" : "")
                if !text.isEmpty { enqueueExternal(.sharedText(text: text, links: draft.links)) }
            }
            for draft in drafts { try inbox.acknowledge(draft.id) }
        } catch { operationError = message(error) }
    }

    func acceptStagedSharedFiles() {
        guard let files = stagedSharedFiles else { return }
        drafts.files.sources.append(contentsOf: files)
        stagedSharedFiles = nil
        externalRequests.completeActive()
        selectedTab = .send
    }

    func mergeStagedSharedText(replacing: Bool) {
        guard let stagedSharedText else { return }
        if replacing || drafts.note.text.isEmpty { drafts.note.text = stagedSharedText.text }
        else { drafts.note.text += "\n\n\(stagedSharedText.text)" }
        self.stagedSharedText = nil
        externalRequests.completeActive()
        selectedTab = .send
    }

    func stageExternalRequest() async {
        guard let request = externalRequests.active else { return }
        switch request {
        case let .receive(input, _): receiveInput = input; selectedTab = .receive; externalRequests.completeActive()
        case let .sharedFiles(files): stagedSharedFiles = files; selectedTab = .send
        case let .sharedText(text, links): stagedSharedText = (text, links); selectedTab = .send
        case let .profile(url): await prepareRecipient(profileURL: url)
        case let .invite(url): pendingInviteLink = url; selectedTab = .settings; externalRequests.completeActive()
        case let .verifyEmail(url): pendingVerificationLink = url; selectedTab = .settings; externalRequests.completeActive()
        case let .resetPassword(url): pendingResetLink = url; selectedTab = .settings; externalRequests.completeActive()
        }
    }

    private func prepareRecipient(profileURL: String) async {
        guard let components = URLComponents(string: profileURL), let host = components.host,
              let profileInstance = FilebeamInstance(origin: "https://\(host)\(components.port.map { ":\($0)" } ?? "")"),
              let username = components.path.split(separator: "/").last.map(String.init), !username.isEmpty else {
            operationError = "This profile link is invalid."
            return
        }
        guard profileInstance == instance else {
            operationError = "Select \(profileInstance.origin) before using a recipient from that instance."
            return
        }
        do {
            drafts.files.options.recipient = try await service.recipient(instance: instance, username: username)
            selectedTab = .send
            externalRequests.completeActive()
        } catch { operationError = message(error) }
    }

    func startFileTransfer(password: String?) async -> Bool {
        guard !isStartingFileTransfer else { return false }
        guard let policy else { operationError = "Connection policy is still loading."; return false }
        let validation = SendPolicy.validate(drafts.files, policy: policy, isAuthenticated: session != nil).submissionStatus
        guard case .ready = validation else { operationError = validation.message; return false }
        isStartingFileTransfer = true
        defer { isStartingFileTransfer = false }
        do {
            let snapshot = try await service.startUpload(.init(instance: instance, paths: drafts.files.sources.map(\.location), sources: drafts.files.sources, options: drafts.files.options, password: password))
            shareKeyChoices[snapshot.id] = drafts.files.options.includeKeyInLink
            accept(snapshot)
            guard snapshot.lifecycle != .failed else { operationError = snapshot.error ?? "The transfer could not start."; return false }
            drafts.files = FileDraft()
            selectedTab = .transfers
            return true
        } catch { operationError = message(error); return false }
    }

    func startNote(password: String?) async -> Bool {
        guard !isStartingNote else { return false }
        guard let policy else { operationError = "Connection policy is still loading."; return false }
        let validation = SendPolicy.validate(drafts.note, policy: policy, isAuthenticated: session != nil).submissionStatus
        guard case .ready = validation else { operationError = validation.message; return false }
        isStartingNote = true
        defer { isStartingNote = false }
        do {
            let snapshot = try await service.startNote(.init(instance: instance, draft: drafts.note, password: password))
            shareKeyChoices[snapshot.id] = drafts.note.options.includeKeyInLink
            accept(snapshot)
            guard snapshot.lifecycle != .failed else { operationError = snapshot.error ?? "The note could not start."; return false }
            drafts.note = NoteDraft()
            selectedTab = .transfers
            return true
        } catch { operationError = message(error); return false }
    }

    func inspectAndReceive() async {
        receiveError = nil
        do {
            let route = try await service.inspectReceive(instance: instance, input: receiveInput)
            guard case let .transfer(_, routeInstance, input) = route else { receiveError = "Enter a transfer link or ID."; return }
            let inspection = try await service.inspectPayload(instance: routeInstance, input: input)
            guard inspection.kind == .files || inspection.kind == .note else { receiveError = "This transfer type is not supported."; return }
            pendingReceiveRoute = ReceiveRouteConfirmation(instance: routeInstance, input: input, inspection: inspection, isForeignInstance: routeInstance != instance)
        } catch { receiveError = message(error) }
    }

    func beginConfirmedReceive() async {
        guard let route = pendingReceiveRoute else { return }
        receiveError = nil
        do {
            if route.inspection.kind == .note {
                let inspection = try await service.inspectNote(route.input)
                pendingNoteInspection = inspection
                pendingNoteInput = route.input
                pendingNoteInstance = route.instance
            } else {
                let directory = try outputDirectory()
                accept(try await service.receive(.init(instance: route.instance, input: route.input, outputDirectory: directory.path)))
            }
            pendingReceiveRoute = nil
        } catch { receiveError = message(error) }
    }

    func receiveNote(separateKey: String?, password: String?, burnAcknowledged: Bool) async {
        guard let input = pendingNoteInput, let routeInstance = pendingNoteInstance else { return }
        do {
            let snapshot = try await service.startReceiveNote(.init(instance: routeInstance, input: input, separateKey: separateKey, password: password, burnAcknowledged: burnAcknowledged))
            pendingNoteInspection = nil
            pendingNoteInput = nil
            pendingNoteInstance = nil
            accept(snapshot)
        } catch { receiveError = message(error) }
    }

    func takeNote(jobID: TransferJobID) async -> VerifiedNote? {
        do { return try await service.takeVerifiedNote(jobID: jobID) }
        catch { operationError = message(error); return nil }
    }

    func retryNoteBurn(jobID: TransferJobID) async -> Bool {
        do { return try await service.retryNoteBurn(jobID: jobID) }
        catch { operationError = message(error); return false }
    }

    func receiveInbox(item: InboxItem, privateKey: Data) async {
        do {
            let directory = try outputDirectory()
            accept(try await service.receiveInbox(instance: instance, transferID: item.id, privateKey: privateKey, outputDirectory: directory.path))
            selectedTab = .transfers
        } catch { inboxError = message(error) }
    }

    func deleteInboxItem(_ item: InboxItem) async {
        do { try await service.deleteInboxItem(instance: instance, transferID: item.id); inboxItems.removeAll { $0.id == item.id } }
        catch { inboxError = message(error) }
    }

    func respond(context: PromptContext, value: String = "", allowed: Bool = true) async -> Bool {
        do {
            if context.prompt.kind == .peerConsent { try await service.respondToConsent(jobID: context.jobID, promptID: context.prompt.id, allowed: allowed) }
            else { try await service.respondToPrompt(jobID: context.jobID, promptID: context.prompt.id, value: value) }
            promptsByJob[context.jobID] = nil
            advancePrompt(after: context)
            await refresh(jobID: context.jobID)
            return true
        } catch { receiveError = message(error); return false }
    }

    func dismissPrompt(_ context: PromptContext) async {
        guard activePrompt?.id == context.id else { return }
        do { try await service.pause(jobID: context.jobID) }
        catch { operationError = message(error) }
        activePrompt = nil
        await refresh(jobID: context.jobID)
    }

    func refresh(jobID: TransferJobID) async {
        do {
            let snapshot = try await service.snapshot(jobID: jobID)
            if snapshot.kind == .note, (snapshot.phase == .verifiedAwaitingExport || snapshot.phase == .complete), let note = try await service.takeVerifiedNote(jobID: jobID) {
                verifiedNotes[jobID] = note
                accept(snapshot.withVerifiedNote(note))
            } else { accept(snapshot) }
        }
        catch { operationError = message(error) }
    }

    func pollLiveJobs() {
        pollingTask?.cancel()
        pollingTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                for job in self.transfers where job.lifecycle == .running || job.lifecycle == .pausing { await self.refresh(jobID: job.id) }
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    func act(on snapshot: TransferSnapshot, action: TransferAction) async {
        do {
            switch action {
            case .pause: try await service.pause(jobID: snapshot.id)
            case .resume: guard let id = snapshot.checkpointID.map({ TransferID($0) }) else { throw FilebeamDomainError.invalidInput("This transfer has no resumable checkpoint.") }; accept(try await service.resume(transferID: id))
            case .discardLocal: guard let id = snapshot.checkpointID.map({ TransferID($0) }) else { throw FilebeamDomainError.invalidInput("This transfer has no local recovery state.") }; try await service.discardLocal(transferID: id)
            case .revokeRemote: guard let id = snapshot.checkpointID.map({ TransferID($0) }) else { throw FilebeamDomainError.invalidInput("This transfer has no remote recovery reference.") }; accept(try await service.revokeRemote(transferID: id))
            case .endLive: guard let id = snapshot.transferID ?? snapshot.checkpointID.map({ TransferID($0) }) else { throw FilebeamDomainError.invalidInput("Live transfer identity is unavailable.") }; accept(try await service.endLive(transferID: id))
            case .receipt: guard let id = snapshot.transferID else { throw FilebeamDomainError.invalidInput("Share receipt is unavailable.") }; receipt = try await service.receipt(transferID: id, includeKey: shareKeyChoices[snapshot.id] ?? false)
            case .downloadCommand: guard let link = snapshot.shareURL else { throw FilebeamDomainError.invalidInput("A share link is unavailable.") }; downloadCommand = try await service.downloadCommand(link: link)
            }
            await refreshTransfers()
        } catch { operationError = message(error) }
    }

    // Saved records retain the native recovery identifier, so recovery never depends on a
    // transient in-memory job or a synthesized checkpoint.
    func resume(record: TransferRecord) async {
        do { accept(try await service.resume(transferID: record.id)); await refreshTransfers() }
        catch { operationError = message(error) }
    }

    func discard(record: TransferRecord) async {
        do { try await service.discardLocal(transferID: record.id); await refreshTransfers() }
        catch { operationError = message(error) }
    }

    func revoke(record: TransferRecord) async {
        do { accept(try await service.revokeRemote(transferID: record.id)); await refreshTransfers() }
        catch { operationError = message(error) }
    }

    func end(record: TransferRecord) async {
        do { accept(try await service.endLive(transferID: record.id)); await refreshTransfers() }
        catch { operationError = message(error) }
    }

    func refreshInbox() async {
        let origin = instance
        inboxError = nil
        do {
            async let items = service.inbox(instance: origin)
            async let unread = service.inboxUnreadCount(instance: origin)
            let result = try await (items, unread)
            guard instance == origin else { return }
            inboxItems = result.0
            inboxUnreadCount = result.1.count
        }
        catch { if instance == origin { inboxError = message(error) } }
    }

    func markInboxNotificationsRead() async {
        do { try await service.markInboxNotificationsRead(instance: instance); inboxUnreadCount = 0 }
        catch { inboxError = message(error) }
    }

    func recordExport(jobID: TransferJobID, paths: [String]) async {
        do {
            try await service.recordExport(jobID: jobID, paths: paths)
            await refresh(jobID: jobID)
            await refreshTransfers()
        } catch { operationError = message(error) }
    }

    func login(email: String, password: String, remember: Bool) async -> Bool {
        let origin = instance
        do {
            let signedIn = try await service.login(.init(instance: origin, email: email, password: password, remember: remember))
            guard instance == origin else { return false }
            session = signedIn
            return true
        } catch { if instance == origin { operationError = message(error) }; return false }
    }

    func logout() async {
        do {
            try await service.logout(instance: instance)
            session = nil
            inboxItems = []
            inboxUnreadCount = 0
            receipt = nil
            downloadCommand = nil
            pendingNoteInspection = nil
            pendingNoteInput = nil
            pendingNoteInstance = nil
            receiveJob = nil
            verifiedNotes = [:]
        }
        catch { operationError = message(error) }
    }

    private func accept(_ snapshot: TransferSnapshot) {
        var snapshot = snapshot
        if let note = verifiedNotes[snapshot.id], snapshot.verifiedNote == nil { snapshot = snapshot.withVerifiedNote(note) }
        if let index = transfers.firstIndex(where: { $0.id == snapshot.id }) { transfers[index] = snapshot }
        else { transfers.append(snapshot) }
        if snapshot.direction == .download || snapshot.direction == .inboxDownload || snapshot.kind == .note { receiveJob = snapshot }
        if let prompt = snapshot.prompt {
            promptsByJob[snapshot.id] = prompt
            if activePrompt == nil { activePrompt = PromptContext(jobID: snapshot.id, prompt: prompt) }
        } else {
            promptsByJob[snapshot.id] = nil
            if activePrompt?.jobID == snapshot.id { activePrompt = nextPrompt(excluding: snapshot.id) }
        }
    }

    private func advancePrompt(after context: PromptContext) { activePrompt = nextPrompt(excluding: context.jobID) }
    private func nextPrompt(excluding jobID: TransferJobID? = nil) -> PromptContext? { promptsByJob.first(where: { $0.key != jobID }).map { PromptContext(jobID: $0.key, prompt: $0.value) } }

    func message(_ error: Error) -> String {
        if case let FilebeamDomainError.invalidInput(value) = error { return value }
        if case let FilebeamDomainError.rejected(value) = error { return value }
        if case let FilebeamDomainError.remote(value) = error { return value }
        return error.localizedDescription
    }
}

private extension TransferSnapshot {
    func withVerifiedNote(_ note: VerifiedNote) -> TransferSnapshot {
        TransferSnapshot(id: id, transferID: transferID, checkpointID: checkpointID, lifecycle: lifecycle, phase: phase, direction: direction, kind: kind, transport: transport, completedBytes: completedBytes, totalBytes: totalBytes, prompt: prompt, verifiedFilePaths: verifiedFilePaths, verifiedNote: note, shareURL: shareURL, serverAvailability: serverAvailability, exportStatus: exportStatus, error: error, actions: actions)
    }
}

enum TransferAction { case pause, resume, discardLocal, revokeRemote, endLive, receipt, downloadCommand }

struct PromptContext: Identifiable, Hashable {
    let jobID: TransferJobID
    let prompt: TransferPrompt
    var id: String { "\(jobID.uuidString):\(prompt.id)" }
}

struct ReceiveRouteConfirmation: Identifiable, Hashable {
    let instance: FilebeamInstance
    let input: String
    let inspection: TransferInspection
    let isForeignInstance: Bool
    var id: String { input }
}

private extension DraftValidation {
    var message: String { switch self { case .ready: return ""; case let .blocked(value), let .unknown(value): return value } }
}
