import Foundation
import Observation
import SwiftUI
import FilebeamDomain

struct PromptContext: Identifiable, Hashable {
    let jobID: TransferJobID
    let prompt: TransferPrompt
    var id: String { "\(jobID.uuidString):\(prompt.id)" }
}

enum TransferAction { case pause, resume, discardLocal, revokeRemote, endLive, receipt, downloadCommand }

extension AccountSession {
    var displayName: String { username ?? name }
}

/// App-wide state shared by every screen: the connected instance, the signed-in account,
/// transfers, the inbox, navigation, transient messages and work handed over by links.
@MainActor @Observable
final class AppModel {
    let service: any FilebeamService
    let appearance: AppearanceStore
    let paths: AppPaths?
    let importer: ([URL]) async -> [FileSource]
    private(set) var instance: FilebeamInstance
    private let outputDirectory: () throws -> URL
    private let loadDrafts: () async throws -> ComposerDrafts?
    private let saveDrafts: (ComposerDrafts) async throws -> Void
    private let saveSettings: (AppSettings) async throws -> Void
    private let secrets: IOSKeychainStore?
    private var settings: AppSettings
    private var draftsByOrigin: [String: ComposerDrafts] = [:]

    // Navigation
    var selectedTab: Tab = .send
    var path: [Route] = []

    // Feedback
    var toast: ToastMessage?
    var dialog: FbDialogSpec?

    // Instance and account
    var policy: InstancePolicy?
    var policyError: String?
    var policyLoading = false
    var session: AccountSession?

    // Transfers
    var transfers: [TransferSnapshot] = []
    var records: [TransferRecord] = []
    var activePrompt: PromptContext?
    var receipt: ShareReceipt?
    var downloadCommand: String?
    private var promptsByJob: [TransferJobID: TransferPrompt] = [:]
    private var verifiedNotes: [TransferJobID: VerifiedNote] = [:]
    private var shareKeyChoices: [TransferJobID: Bool] = [:]
    /// Jobs whose prompts a feature screen answers itself; the global prompt sheet leaves them alone.
    private var claimedJobs: Set<TransferJobID> = []
    private var pollingTask: Task<Void, Never>?

    // Inbox
    var inboxItems: [InboxItem]?
    var inboxUnreadCount: UInt64 = 0
    var inboxError: String?
    var stagedInbox: [StagedInboxItem] = []
    var automaticReceiving = false

    // Drafts and settings
    var drafts = ComposerDrafts()
    var pendingInstance: FilebeamInstance?
    var instanceChangeError: String?

    // Work handed over by links and shares, claimed once by the screen that understands it.
    var pendingReceiveLink: String?
    var pendingRecipient: String?
    var pendingInvitationLink: String?
    var pendingVerificationLink: String?
    var pendingResetLink: String?
    var stagedSharedFiles: [FileSource] = []
    var stagedSharedText: String?

    private var sendStorage: SendModel!
    private var receiveStorage: ReceiveModel!
    var send: SendModel { sendStorage }
    var receive: ReceiveModel { receiveStorage }

    init(service: any FilebeamService, instance: FilebeamInstance, outputDirectory: @escaping () throws -> URL, paths: AppPaths? = nil, appearance: AppearanceStore = AppearanceStore(defaults: UserDefaults(suiteName: "preview") ?? .standard), secrets: IOSKeychainStore? = nil, importer: @escaping ([URL]) async -> [FileSource] = { _ in [] }, loadDrafts: @escaping () async throws -> ComposerDrafts? = { nil }, saveDrafts: @escaping (ComposerDrafts) async throws -> Void = { _ in }, settings: AppSettings? = nil, saveSettings: @escaping (AppSettings) async throws -> Void = { _ in }) {
        self.service = service
        self.instance = instance
        self.outputDirectory = outputDirectory
        self.paths = paths
        self.appearance = appearance
        self.secrets = secrets
        self.importer = importer
        self.loadDrafts = loadDrafts
        self.saveDrafts = saveDrafts
        self.settings = settings ?? AppSettings(instanceOrigin: instance.origin)
        self.saveSettings = saveSettings
        sendStorage = SendModel(app: self)
        receiveStorage = ReceiveModel(app: self)
    }

    // MARK: Navigation and feedback

    func push(_ route: Route) { if path.last != route { path.append(route) } }
    func pop() { _ = path.popLast() }
    /// Replaces the top route, e.g. sign-in giving way to the account screen.
    func replace(_ route: Route) { if !path.isEmpty { path.removeLast() }; push(route) }
    func select(_ tab: Tab) { path.removeAll(); selectedTab = tab }

    func toast(_ text: String, tone: Tone = .neutral) { toast = ToastMessage(text: tone == .danger ? readableMessage(text) : text, tone: tone) }
    func toastDismissed(_ message: ToastMessage) { if toast?.id == message.id { toast = nil } }
    func fail(_ error: Error) { toast(readableMessage(error), tone: .danger) }
    func present(_ spec: FbDialogSpec) { dialog = spec }
    func dismissDialog() { dialog = nil }

    var branding: InstanceBranding { policy?.branding ?? InstanceBranding() }
    var relayOnly: Bool { settings.relayOnly }
    var instanceTitle: String { instance.origin.replacingOccurrences(of: "https://", with: "") }
    func outputFolder() throws -> URL { try outputDirectory() }

    // MARK: Lifecycle

    func load() async {
        do { if let stored = try await loadDrafts() { drafts = stored; draftsByOrigin[instance.origin] = stored } }
        catch { fail(error) }
        await refreshPolicy()
        await refreshSession()
        await refreshTransfers()
        pollLiveJobs()
    }

    func persistDrafts(_ value: ComposerDrafts) async {
        draftsByOrigin[instance.origin] = value
        do { try await saveDrafts(value) } catch { fail(error) }
    }

    func refreshPolicy() async {
        let origin = instance
        policyLoading = true
        defer { if instance == origin { policyLoading = false } }
        do {
            let discovered = try await service.discover(instance: origin)
            if instance == origin { policy = discovered; policyError = nil }
        } catch { if instance == origin { policyError = readableMessage(error) } }
    }

    func refreshSession() async {
        let origin = instance
        do {
            let restored = try await service.session(instance: origin)
            if instance == origin { session = restored }
        } catch { if instance == origin { session = nil } }
        await refreshUnread()
    }

    /// Keeps the session, automatic receiving and the unread badge fresh while the app is open.
    func heartbeat() async {
        while !Task.isCancelled {
            await refreshSession()
            await refreshAutomaticInbox()
            try? await Task.sleep(for: .seconds(60))
        }
    }

    // MARK: Instance settings

    func stageInstance(origin: String) {
        instanceChangeError = nil
        guard let candidate = FilebeamInstance(origin: origin) else { instanceChangeError = "Enter a full address such as https://filebeam.io."; return }
        guard candidate != instance else { pendingInstance = nil; return }
        pendingInstance = candidate
    }

    /// Switches instance after it answered, so a typo can never strand the app.
    func confirmInstanceChange() async -> Bool {
        guard let candidate = pendingInstance else { return false }
        draftsByOrigin[instance.origin] = drafts
        do {
            _ = try await service.discover(instance: candidate)
            var updated = settings
            updated.instanceOrigin = candidate.origin
            try await saveSettings(updated)
            settings = updated
            instance = candidate
            pendingInstance = nil
            session = nil
            inboxItems = nil
            stagedInbox = []
            inboxUnreadCount = 0
            inboxError = nil
            policy = nil
            drafts = draftsByOrigin[candidate.origin] ?? ComposerDrafts()
            await refreshPolicy()
            await refreshSession()
            return true
        } catch {
            instanceChangeError = readableMessage(error)
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
        } catch { instanceChangeError = readableMessage(error); return false }
    }

    // MARK: Account

    func login(identity: String, password: String, remember: Bool) async throws {
        let origin = instance
        let signedIn = try await service.login(.init(instance: origin, email: identity, password: password, remember: remember))
        guard instance == origin else { return }
        session = signedIn
        await recoverPasswordCustody(password: password, userID: signedIn.id)
        await refreshUnread()
    }

    func register(username: String, name: String?, email: String, password: String) async throws {
        let origin = instance
        let signedIn = try await service.register(.init(instance: origin, username: username, name: name, email: email, password: password))
        guard instance == origin else { return }
        session = signedIn
        await recoverPasswordCustody(password: password, userID: signedIn.id)
    }

    func acceptInvitation(link: String, username: String, name: String?, email: String, password: String) async throws {
        session = try await service.acceptInvite(.init(link: link, username: username, name: name, email: email, password: password))
    }

    func logout() async {
        do {
            try await service.logout(instance: instance)
            clearAccountState()
        } catch { fail(error) }
    }

    func deleteAccount(password: String) async throws {
        _ = try await service.requestAccountDeletion(.init(instance: instance, currentPassword: password, confirmation: "DELETE"))
        clearAccountState()
    }

    private func clearAccountState() {
        session = nil
        inboxItems = nil
        inboxUnreadCount = 0
        stagedInbox = []
        receipt = nil
        downloadCommand = nil
    }

    func updatePreferences(inboxEnabled: Bool? = nil, notificationChannel: String? = nil) async throws {
        guard let current = session else { return }
        let preferences = AccountPreferences(inboxEnabled: inboxEnabled ?? current.inboxEnabled, notificationChannel: notificationChannel ?? current.notificationChannel)
        try await service.updatePreferences(instance: instance, preferences: preferences)
        session = AccountSession(id: current.id, name: current.name, username: current.username, email: current.email, emailVerifiedAt: current.emailVerifiedAt, profileURL: current.profileURL, inboxEnabled: preferences.inboxEnabled, usernameRoutingEnabled: current.usernameRoutingEnabled, notificationChannel: preferences.notificationChannel)
    }

    // MARK: Inbox keys

    private func keyName(bundleID: UInt64, userID: UInt64) -> String { "inbox-key.\(instance.origin).\(userID).\(bundleID)" }

    /// A private key this device already holds for the bundle, or nil.
    func rememberedInboxKey(bundleID: UInt64, userID: UInt64) -> Data? {
        guard let key = try? secrets?.loadSecret(keyName(bundleID: bundleID, userID: userID)), key.count == 32 else { return nil }
        return key
    }

    func rememberInboxKey(_ key: Data, bundleID: UInt64, userID: UInt64) {
        guard key.count == 32 else { return }
        try? secrets?.saveSecret(keyName(bundleID: bundleID, userID: userID), key)
    }

    /// After a password sign-in, password-custody keys are unlocked once and kept on this device.
    private func recoverPasswordCustody(password: String, userID: UInt64) async {
        guard let bundles = try? await service.accountKeys(instance: instance) else { return }
        for bundle in bundles where bundle.isActive && bundle.custody == .password && bundle.encryptedPrivateKey != nil {
            if let key = try? await service.unwrapPasswordCustodyKey(envelope: bundle.encryptedPrivateKey!, password: password, userID: userID, publicKey: bundle.publicKey) {
                rememberInboxKey(key, bundleID: bundle.id, userID: userID)
            }
        }
    }

    /// The private key for a recipient bundle: remembered on this device, unwrapped with the
    /// account password, or imported as an fbsk1 export.
    func privateKey(for bundle: AccountKeyBundle, password: String?) async throws -> Data {
        guard let session else { throw FilebeamDomainError.invalidInput("Sign in first.") }
        guard bundle.userID == session.id else { throw FilebeamDomainError.invalidInput("This inbox key belongs to another account.") }
        if let remembered = rememberedInboxKey(bundleID: bundle.id, userID: session.id) { return remembered }
        if bundle.custody == .password, let envelope = bundle.encryptedPrivateKey, let password, !password.isEmpty {
            let key = try await service.unwrapPasswordCustodyKey(envelope: envelope, password: password, userID: session.id, publicKey: bundle.publicKey)
            rememberInboxKey(key, bundleID: bundle.id, userID: session.id)
            return key
        }
        if let imported = try? secrets?.loadSecret("inbox-key-import.\(instance.origin).\(session.id)"), imported.count == 32 { return imported }
        throw FilebeamDomainError.invalidInput(bundle.custody == .password ? "Enter your account password or import the matching fbsk1 key." : "Import the fbsk1 key for this inbox delivery under Secure inbox.")
    }

    func importSelfCustodyKey(_ encoded: String) async throws {
        guard let session else { throw FilebeamDomainError.invalidInput("Sign in first.") }
        let key = try await service.importSelfCustodyKey(encoded.trimmingCharacters(in: .whitespacesAndNewlines))
        try secrets?.saveSecret("inbox-key-import.\(instance.origin).\(session.id)", key)
    }

    func exportActiveKey() async throws -> String {
        guard let session else { throw FilebeamDomainError.invalidInput("Sign in first.") }
        let situation = try await service.keySituation(instance: instance)
        guard let active = situation.activeBundleID, let key = rememberedInboxKey(bundleID: active, userID: session.id) else {
            throw FilebeamDomainError.invalidInput("Import or generate an account key first.")
        }
        return try await service.exportSelfCustodyKey(key)
    }

    // MARK: Inbox

    func refreshInbox() async {
        guard session != nil else { inboxItems = nil; return }
        let origin = instance
        inboxError = nil
        do {
            let items = try await service.inbox(instance: origin)
            guard instance == origin else { return }
            inboxItems = items
        } catch { if instance == origin { inboxError = readableMessage(error) } }
        await refreshUnread()
    }

    func refreshUnread() async {
        guard session != nil else { inboxUnreadCount = 0; return }
        let origin = instance
        if let unread = try? await service.inboxUnreadCount(instance: origin), instance == origin { inboxUnreadCount = unread.count }
    }

    /// Seeing the inbox is what clears its unread badge.
    func markInboxSeen() async {
        guard session != nil, inboxUnreadCount > 0 else { return }
        try? await service.markInboxNotificationsRead(instance: instance)
        inboxUnreadCount = 0
    }

    func refreshAutomaticInbox() async {
        guard session != nil else { return }
        let selected = instance
        if let state = try? await service.receiveAutomatically(instance: selected), instance == selected {
            stagedInbox = state.entries.filter { $0.state != "dismissed" }
            automaticReceiving = state.enabled
        }
    }

    func setAutomaticReceiving(_ enabled: Bool) async throws {
        let state = try await service.inboxReceiver(instance: instance, enabled: enabled)
        automaticReceiving = state.enabled
        stagedInbox = state.entries.filter { $0.state != "dismissed" }
        if enabled { await refreshAutomaticInbox() }
    }

    func receiveInbox(item: InboxItem, password: String?) async throws {
        let metadata = try await service.inboxMetadata(instance: instance, transferID: item.id)
        let key = try await privateKey(for: metadata.recipientKeyBundle, password: password)
        let directory = try outputDirectory()
        accept(try await service.receiveInbox(instance: instance, transferID: item.id, privateKey: key, outputDirectory: directory.path))
        select(.transfers)
    }

    func deleteInboxItem(_ item: InboxItem) async throws {
        try await service.deleteInboxItem(instance: instance, transferID: item.id)
        inboxItems?.removeAll { $0.id == item.id }
    }

    func verifyStagedInbox(_ item: StagedInboxItem, password: String?) async throws -> [URL] {
        let bundles = try await service.accountKeys(instance: instance)
        guard let bundle = bundles.first(where: { $0.id == item.keyBundleID }) else { throw FilebeamDomainError.invalidInput("The receiving key is unavailable.") }
        let key = try await privateKey(for: bundle, password: password)
        let destination = try outputDirectory().appendingPathComponent("inbox-\(item.id)", isDirectory: true)
        let paths = try await service.exportStagedInbox(instance: instance, id: item.id, privateKey: key, destination: destination.path)
        await refreshAutomaticInbox()
        return paths.map { URL(fileURLWithPath: $0) }
    }

    func dismissStagedInbox(_ item: StagedInboxItem) async throws {
        try await service.dismissStagedInbox(instance: instance, id: item.id)
        await refreshAutomaticInbox()
    }

    // MARK: Transfers

    func refreshTransfers() async {
        do { records = try await service.savedTransfers() } catch { fail(error) }
    }

    func claim(_ jobID: TransferJobID) { claimedJobs.insert(jobID); if activePrompt?.jobID == jobID { activePrompt = nextPrompt(excluding: jobID) } }
    func release(_ jobID: TransferJobID) { claimedJobs.remove(jobID) }
    func snapshot(_ jobID: TransferJobID) -> TransferSnapshot? { transfers.first { $0.id == jobID } }

    func respond(context: PromptContext, value: String = "", allowed: Bool = true) async -> Bool {
        do {
            if context.prompt.kind == .peerConsent { try await service.respondToConsent(jobID: context.jobID, promptID: context.prompt.id, allowed: allowed) }
            else { try await service.respondToPrompt(jobID: context.jobID, promptID: context.prompt.id, value: value) }
            promptsByJob[context.jobID] = nil
            activePrompt = nextPrompt(excluding: context.jobID)
            await refresh(jobID: context.jobID)
            return true
        } catch { fail(error); return false }
    }

    func dismissPrompt(_ context: PromptContext) async {
        guard activePrompt?.id == context.id else { return }
        try? await service.pause(jobID: context.jobID)
        activePrompt = nil
        await refresh(jobID: context.jobID)
    }

    func refresh(jobID: TransferJobID) async {
        do {
            let snapshot = try await service.snapshot(jobID: jobID)
            if snapshot.kind == .note, snapshot.phase == .verifiedAwaitingExport || snapshot.phase == .complete, let note = try await service.takeVerifiedNote(jobID: jobID) {
                verifiedNotes[jobID] = note
                accept(snapshot.withVerifiedNote(note))
            } else { accept(snapshot) }
        } catch { fail(error) }
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
            case .discardLocal: guard let id = snapshot.checkpointID.map({ TransferID($0) }) else { throw FilebeamDomainError.invalidInput("This transfer has no local recovery state.") }; try await service.discardLocal(transferID: id); transfers.removeAll { $0.id == snapshot.id }
            case .revokeRemote: guard let id = snapshot.checkpointID.map({ TransferID($0) }) ?? snapshot.transferID else { throw FilebeamDomainError.invalidInput("This transfer has no remote recovery reference.") }; accept(try await service.revokeRemote(transferID: id))
            case .endLive: guard let id = snapshot.transferID ?? snapshot.checkpointID.map({ TransferID($0) }) else { throw FilebeamDomainError.invalidInput("Live transfer identity is unavailable.") }; accept(try await service.endLive(transferID: id))
            case .receipt: guard let id = snapshot.transferID else { throw FilebeamDomainError.invalidInput("Share receipt is unavailable.") }; receipt = try await service.receipt(transferID: id, includeKey: shareKeyChoices[snapshot.id] ?? true)
            case .downloadCommand: guard let link = snapshot.shareURL else { throw FilebeamDomainError.invalidInput("A share link is unavailable.") }; downloadCommand = try await service.downloadCommand(link: link)
            }
            await refresh(jobID: snapshot.id)
            await refreshTransfers()
        } catch { fail(error) }
    }

    // Saved records retain the native recovery identifier, so recovery never depends on a
    // transient in-memory job or a synthesized checkpoint.
    func resume(record: TransferRecord) async {
        do { accept(try await service.resume(transferID: record.id)); await refreshTransfers() } catch { fail(error) }
    }
    func discard(record: TransferRecord) async {
        do { try await service.discardLocal(transferID: record.id); await refreshTransfers() } catch { fail(error) }
    }
    func revoke(record: TransferRecord) async {
        do { accept(try await service.revokeRemote(transferID: record.id)); await refreshTransfers() } catch { fail(error) }
    }
    func end(record: TransferRecord) async {
        do { accept(try await service.endLive(transferID: record.id)); await refreshTransfers() } catch { fail(error) }
    }

    func recordExport(jobID: TransferJobID, paths: [String]) async {
        do { try await service.recordExport(jobID: jobID, paths: paths); await refresh(jobID: jobID); await refreshTransfers() } catch { fail(error) }
    }

    func rememberKeyChoice(_ include: Bool, for jobID: TransferJobID) { shareKeyChoices[jobID] = include }

    func accept(_ snapshot: TransferSnapshot) {
        var snapshot = snapshot
        if let note = verifiedNotes[snapshot.id], snapshot.verifiedNote == nil { snapshot = snapshot.withVerifiedNote(note) }
        if let index = transfers.firstIndex(where: { $0.id == snapshot.id }) { transfers[index] = snapshot } else { transfers.append(snapshot) }
        guard !claimedJobs.contains(snapshot.id) else { return }
        if let prompt = snapshot.prompt {
            promptsByJob[snapshot.id] = prompt
            if activePrompt == nil { activePrompt = PromptContext(jobID: snapshot.id, prompt: prompt) }
        } else {
            promptsByJob[snapshot.id] = nil
            if activePrompt?.jobID == snapshot.id { activePrompt = nextPrompt(excluding: snapshot.id) }
        }
    }

    private func nextPrompt(excluding jobID: TransferJobID? = nil) -> PromptContext? {
        promptsByJob.first { $0.key != jobID && !claimedJobs.contains($0.key) }.map { PromptContext(jobID: $0.key, prompt: $0.value) }
    }

    // MARK: Links and shares

    /// Routes an https link on the app-link host, or a transfer link, to the screen that understands it.
    func handle(url: URL) {
        let link = url.absoluteString
        switch InputRouter.route(link, selectedInstance: instance) {
        case let .profile(value):
            if let username = URL(string: value)?.pathComponents.last?.lowercased(), !username.isEmpty, username != "/" { push(.recipient(username)) }
        case let .invite(value): pendingInvitationLink = value; push(.auth(.invitation))
        case let .verifyEmail(value): pendingVerificationLink = value; push(.auth(.verify))
        case let .resetPassword(value): pendingResetLink = value; push(.auth(.reset))
        case .transfer:
            let path = url.path.lowercased()
            if path.hasPrefix("/features") { push(.tour) }
            else if path.hasPrefix("/changelog") { if let tag = url.pathComponents.dropFirst(2).first { push(.release(tag)) } else { push(.changelog) } }
            else if path.hasPrefix("/account") { push(session != nil ? .account : .auth(.login)) }
            else { pendingReceiveLink = link; select(.receive) }
        case .invalid: break
        }
    }

    func processPendingSharedDrafts(root: URL) async {
        do {
            let inbox = try SharedDraftInbox(groupURL: root)
            let pending = try inbox.pending()
            guard !pending.isEmpty else { return }
            var files: [FileSource] = []
            var texts: [String] = []
            var failures = 0
            for draft in pending {
                guard let current = try inbox.peek(draft.id) else { continue }
                if current.failure != nil { failures += 1; try inbox.acknowledge(draft.id); continue }
                let imported = await importer(current.files)
                guard imported.count == current.files.count, imported.allSatisfy({ if case .ready = $0.state { return true }; return false }) else {
                    toast("Shared files are still unavailable. Share them again from the originating app.", tone: .danger)
                    continue
                }
                files += imported
                let text = [current.text, current.links.isEmpty ? nil : current.links.joined(separator: "\n")].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: "\n\n")
                if !text.isEmpty { texts.append(text) }
                try inbox.acknowledge(draft.id)
            }
            if !files.isEmpty { send.addSources(files); select(.send) }
            if !texts.isEmpty { send.appendText(texts.joined(separator: "\n\n")); select(.send) }
            if failures > 0 { toast("A shared item could not be staged. Re-select it in the originating app.", tone: .danger) }
        } catch { fail(error) }
    }
}

extension TransferSnapshot {
    func withVerifiedNote(_ note: VerifiedNote) -> TransferSnapshot {
        TransferSnapshot(id: id, transferID: transferID, checkpointID: checkpointID, lifecycle: lifecycle, phase: phase, direction: direction, kind: kind, transport: transport, completedBytes: completedBytes, totalBytes: totalBytes, prompt: prompt, verifiedFilePaths: verifiedFilePaths, verifiedNote: note, shareURL: shareURL, serverAvailability: serverAvailability, exportStatus: exportStatus, error: error, actions: actions)
    }
    var fraction: Double? {
        guard let totalBytes, totalBytes > 0 else { return nil }
        return Double(completedBytes) / Double(totalBytes)
    }
    var isActive: Bool { lifecycle == .running || lifecycle == .pausing || lifecycle == .paused }
}

extension TransferPhase {
    /// The phase as a sentence, e.g. "Encrypting and uploading".
    var label: String {
        switch self {
        case .preparing: "Preparing encryption"
        case .encrypting: "Encrypting"
        case .uploading: "Encrypting and uploading"
        case .receiving: "Decrypting download"
        case .waitingForRecipient: "Waiting for a recipient"
        case .waitingForSender: "Waiting for the sender"
        case .awaitingInput: "Waiting for you"
        case .pausing: "Pausing"
        case .paused: "Paused"
        case .verifying: "Verifying integrity"
        case .verifiedAwaitingExport: "Verified"
        case .exporting: "Saving file"
        case .complete: "Complete"
        case .failed: "Could not finish"
        case .expired: "Expired"
        case .revoked: "Deleted"
        case .ended: "Ended"
        case .unknown: "Working"
        }
    }
}
