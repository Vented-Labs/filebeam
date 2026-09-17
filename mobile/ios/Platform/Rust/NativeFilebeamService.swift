import Foundation
import FilebeamCore
import FilebeamDomain

public typealias DTransferSnapshot = FilebeamDomain.TransferSnapshot
public typealias DAccountSession = FilebeamDomain.AccountSession
public typealias DRecipient = FilebeamDomain.Recipient
public typealias DInboxMetadata = FilebeamDomain.InboxMetadata
public typealias DOpenedInbox = FilebeamDomain.OpenedInbox
public typealias DAccountKeyBundle = FilebeamDomain.AccountKeyBundle
public typealias DGeneratedAccountKey = FilebeamDomain.GeneratedAccountKey

public protocol NativeSecretStorage: Sendable {
    func checkpointKey(scope: String) throws -> Data
    func loadSession(origin: String) throws -> String?
    func saveSession(origin: String, cookie: String?) throws
}
public protocol NativeSecretStorageRemoving: NativeSecretStorage { func removeCheckpointKey(scope: String) throws }

/// Storage injected by the app for small sensitive adapter records. Implementations
/// must authenticate and encrypt data; the adapter never writes these records itself.
public protocol NativeRecordStorage: Sendable {
    func load(name: String) throws -> Data?
    func save(name: String, data: Data) throws
}

/// Foundation-only adapter. Rust owns transfer execution; this object owns FFI
/// handles and associates app job IDs with their native job/checkpoint IDs.
public final class NativeFilebeamService: FilebeamService, @unchecked Sendable {
    private final class CheckpointStore: SecretStoreCallback, @unchecked Sendable {
        let secrets: any NativeSecretStorage
        init(_ secrets: any NativeSecretStorage) { self.secrets = secrets }
        func loadOrCreate(scope: String) throws -> Data {
            do { return try secrets.checkpointKey(scope: scope) }
            catch { throw FilebeamCore.ClientError.Operation(detail: "The device checkpoint key is unavailable. Unlock the device and retry.") }
        }
        func remove(scope: String) throws {
            guard let removable = secrets as? any NativeSecretStorageRemoving else { throw FilebeamCore.ClientError.Operation(detail: "Checkpoint key removal is not supported by this custody provider.") }
            do { try removable.removeCheckpointKey(scope: scope) }
            catch { throw FilebeamCore.ClientError.Operation(detail: "The device checkpoint key could not be removed.") }
        }
    }
    private enum Handle { case transfer(TransferJob); case note(ManagedNoteReceive); case creatingNote(ManagedNoteCreate); case completed(ShareReceipt); case backgroundOnly }
    private struct Job {
        var handle: Handle
        var transferID: TransferID?
        var password: String?
        var direction: FilebeamDomain.TransferDirection
        var kind: FilebeamDomain.TransferKind
        var transport: FilebeamDomain.Transport?
        var verifiedNote: FilebeamDomain.VerifiedNote?
        var includeKey = true
        var origin = ""
        var backgroundStage: BackgroundStage? = nil
        var backgroundProgress: (done: UInt64, total: UInt64?)? = nil
        var inboxCredentials: (workingKey: Data, cookie: String)? = nil
        var selectionItems: [TransferSelectionItem] = []
        var selectionPromptID: UInt64? = nil
    }
    private enum BackgroundStage { case preparing, selecting, network, finalizing, failed(String), paused }
    private struct PreparationResult: Sendable { let complete: Bool; let failed: Bool; let paused: Bool; let checkpointID: String?; let error: String? }
    private struct BackgroundAssociation: Codable {
        let jobID: UUID
        let origin: String
        let remoteID: String?
        let checkpointID: String
        let kind: FilebeamDomain.TransferKind
        let direction: NativeBackgroundDirection
        let includeKey: Bool
        let selectionItems: [TransferSelectionItem]?
        let selectionPromptID: UInt64?
    }

    private let queue = DispatchQueue(label: "io.filebeam.native.ffi", qos: .utility)
    private let lock = NSLock()
    private let secrets: any NativeSecretStorage
    private let recordStorage: (any NativeRecordStorage)?
    private let client: TransferClient
    private let runtime: NativeRuntime
    private var jobs: [TransferJobID: Job] = [:]
    private var receipts: [TransferID: ShareReceipt] = [:]
    private var servicesByOrigin: [String: NativeServices] = [:]
    private var rememberedOrigins = Set<String>()
    private var backgroundExecutor: (any NativeBackgroundExecutor)?
    private var backgroundAssociations: [String: BackgroundAssociation] = [:]
    private var finalizingBackgroundCheckpoints = Set<String>()

    private struct StoredReceipt: Codable {
        let receipt: ShareReceipt
        let deleteToken: String?
        let origin: String
        var exportedPaths: [String]
        // Retain the canonical capability separately from its UI presentation.
        let canonicalShareURL: String?
        let canonicalShareKey: String?

        enum CodingKeys: String, CodingKey { case receipt, deleteToken, origin, exportedPaths, canonicalShareURL, canonicalShareKey }
        init(receipt: ShareReceipt, deleteToken: String?, origin: String, exportedPaths: [String], canonicalShareURL: String?, canonicalShareKey: String?) {
            self.receipt = receipt; self.deleteToken = deleteToken; self.origin = origin; self.exportedPaths = exportedPaths; self.canonicalShareURL = canonicalShareURL; self.canonicalShareKey = canonicalShareKey
        }
        init(from decoder: Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            receipt = try values.decode(ShareReceipt.self, forKey: .receipt)
            deleteToken = try values.decodeIfPresent(String.self, forKey: .deleteToken)
            origin = try values.decode(String.self, forKey: .origin)
            exportedPaths = try values.decodeIfPresent([String].self, forKey: .exportedPaths) ?? []
            canonicalShareURL = try values.decodeIfPresent(String.self, forKey: .canonicalShareURL)
            canonicalShareKey = try values.decodeIfPresent(String.self, forKey: .canonicalShareKey)
        }
    }
    private var storedReceipts: [String: StoredReceipt] = [:]
    private var exportedPaths: [String: [String]] = [:]
    private static let receiptRecordName = "native-receipts-v1"
    private static let exportRecordName = "native-exports-v1"
    private static let backgroundRecordName = "native-background-v1"

    public init(stateDirectory: URL, secrets: any NativeSecretStorage, relayOnly: Bool = false, recordStorage: (any NativeRecordStorage)? = nil) throws {
        guard stateDirectory.isFileURL, stateDirectory.path.hasPrefix("/") else {
            throw FilebeamDomainError.invalidInput("The transfer state directory must be absolute.")
        }
        self.secrets = secrets
        self.recordStorage = recordStorage
        let config = ClientConfig(stateDirectory: stateDirectory.path, memoryBudgetMib: 128, maxConcurrency: 2, relayOnly: relayOnly, allowHttp: false)
        let store = CheckpointStore(secrets)
        runtime = try NativeRuntime.newWithSecretStore(config: config, secretStore: store)
        client = try TransferClient.newWithRuntimeSecretStore(config: config, runtime: runtime, secretStore: store)
        if let data = try recordStorage?.load(name: Self.receiptRecordName) {
            storedReceipts = try JSONDecoder().decode([String: StoredReceipt].self, from: data)
            receipts = Dictionary(uniqueKeysWithValues: storedReceipts.map { (TransferID($0.key), $0.value.receipt) })
        }
        if let data = try recordStorage?.load(name: Self.exportRecordName) {
            exportedPaths = try JSONDecoder().decode([String: [String]].self, from: data)
        }
        if let data = try recordStorage?.load(name: Self.backgroundRecordName) {
            backgroundAssociations = try JSONDecoder().decode([String: BackgroundAssociation].self, from: data)
            for association in backgroundAssociations.values {
                do {
                    let status = try client.backgroundTransfer().status(transferId: association.checkpointID)
                    let direction: FilebeamDomain.TransferDirection = association.direction == .upload ? .upload : association.direction == .inboxDownload ? .inboxDownload : .download
                    let stage: BackgroundStage = status.awaitingUnlock ? .failed("Unlock the device and retry this background transfer.") : association.selectionItems == nil ? .network : .selecting
                    let transport: FilebeamDomain.Transport = status.transport == "webrtc" ? .webRTC : .http
                    jobs[association.jobID] = Job(handle: .backgroundOnly, transferID: association.remoteID.map { TransferID($0) }, password: nil, direction: direction, kind: association.kind, transport: transport, verifiedNote: nil, includeKey: association.includeKey, origin: association.origin, backgroundStage: stage, backgroundProgress: (status.done, status.total), inboxCredentials: nil, selectionItems: association.selectionItems ?? [], selectionPromptID: association.selectionPromptID)
                } catch {
                    // Do not claim recoverable background state when its sealed key is unavailable.
                    let direction: FilebeamDomain.TransferDirection = association.direction == .upload ? .upload : association.direction == .inboxDownload ? .inboxDownload : .download
                    jobs[association.jobID] = Job(handle: .backgroundOnly, transferID: association.remoteID.map { TransferID($0) }, password: nil, direction: direction, kind: association.kind, transport: .http, verifiedNote: nil, includeKey: association.includeKey, origin: association.origin, backgroundStage: .failed("Background state is unavailable until the device is unlocked. Retry after unlocking."), backgroundProgress: nil, inboxCredentials: nil)
                }
            }
        }
    }

    private func saveReceipts() throws {
        guard let recordStorage else { return }
        try recordStorage.save(name: Self.receiptRecordName, data: JSONEncoder().encode(storedReceipts))
    }

    private func saveExports() throws {
        guard let recordStorage else { return }
        try recordStorage.save(name: Self.exportRecordName, data: JSONEncoder().encode(exportedPaths))
    }
    private func saveBackgroundAssociations() throws {
        guard let recordStorage else { return }
        try recordStorage.save(name: Self.backgroundRecordName, data: JSONEncoder().encode(backgroundAssociations))
    }

    public func backgroundEngine() async throws -> BackgroundTransfer { try await ffi { self.client.backgroundTransfer() } }

    /// May be called after constructing the service, before the app model starts jobs.
    public func configureBackgroundExecutor(_ executor: (any NativeBackgroundExecutor)?) {
        lock.lock(); backgroundExecutor = executor; lock.unlock()
        guard let executor else { return }
        Task { await executor.reconcile() }
    }

    public func handleBackgroundEvent(_ event: NativeBackgroundEvent) {
        Task { [weak self] in await self?.processBackgroundEvent(event) }
    }

    private func startBackgroundPreparation(jobID: TransferJobID) {
        Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                do {
                    let result = try await self.ffi { () -> PreparationResult in
                        let job = try self.job(jobID)
                        guard case let .transfer(native) = job.handle else { throw FilebeamDomainError.invalidInput("Background preparation is no longer active.") }
                        let snapshot = native.snapshot()
                        return PreparationResult(complete: snapshot.state == .complete, failed: snapshot.state == .failed, paused: snapshot.state == .paused, checkpointID: snapshot.results.first, error: snapshot.error)
                    }
                    if result.complete {
                        guard let checkpointID = result.checkpointID, !checkpointID.isEmpty else {
                            await self.failBackground(jobID: jobID, message: "Native preparation did not produce a checkpoint.")
                            return
                        }
                        await self.prepareBackgroundDownloadSelection(jobID: jobID, checkpointID: checkpointID)
                        return
                    }
                    if result.paused {
                        self.lock.withLock { if var job = self.jobs[jobID] { job.backgroundStage = .paused; self.jobs[jobID] = job } }
                        return
                    }
                    if result.failed {
                        await self.failBackground(jobID: jobID, message: result.error ?? "Native background preparation failed.")
                        return
                    }
                    try await Task.sleep(for: .milliseconds(200))
                } catch {
                    await self.failBackground(jobID: jobID, message: error.localizedDescription)
                    return
                }
            }
        }
    }

    private func activateBackground(jobID: TransferJobID, checkpointID: String) async {
        do {
            let association = try await ffi { () -> BackgroundAssociation in
                self.lock.lock(); defer { self.lock.unlock() }
                guard var job = self.jobs[jobID] else { throw FilebeamDomainError.invalidInput("Background preparation has already been handled.") }
                guard ({ if case .preparing? = job.backgroundStage { return true }; if case .selecting? = job.backgroundStage { return true }; return false }()) else { throw FilebeamDomainError.invalidInput("Background preparation has already been handled.") }
                let direction: NativeBackgroundDirection = job.direction == .upload ? .upload : job.direction == .inboxDownload ? .inboxDownload : .download
                let status = try self.client.backgroundTransfer().status(transferId: checkpointID)
                let association = BackgroundAssociation(jobID: jobID, origin: job.origin, remoteID: status.remoteId ?? job.transferID?.rawValue, checkpointID: checkpointID, kind: job.kind, direction: direction, includeKey: job.includeKey, selectionItems: nil, selectionPromptID: nil)
                self.backgroundAssociations[checkpointID] = association
                try self.saveBackgroundAssociations()
                job.backgroundStage = .network
                job.selectionItems = []
                job.selectionPromptID = nil
                job.transferID = status.remoteId.map { TransferID($0) } ?? job.transferID
                self.jobs[jobID] = job
                return association
            }
            if lock.withLock({ jobs[jobID]?.transport == .webRTC }) {
                let native = try await ffi { self.client.resume(id: checkpointID) }
                try await ffi {
                    self.lock.lock(); defer { self.lock.unlock() }
                    guard var job = self.jobs[jobID] else { throw FilebeamDomainError.invalidInput("WebRTC preparation is no longer active.") }
                    job.handle = .transfer(native)
                    job.backgroundStage = nil
                    self.jobs[jobID] = job
                    self.backgroundAssociations.removeValue(forKey: checkpointID)
                    try self.saveBackgroundAssociations()
                }
                return
            }
            guard let executor = lock.withLock({ backgroundExecutor }) else { throw FilebeamDomainError.remote("Background HTTP executor is unavailable.") }
            if association.direction == .inboxDownload {
                guard let cookie = lock.withLock({ jobs[jobID]?.inboxCredentials?.cookie }),
                      let authenticatedExecutor = executor as? any NativeInboxBackgroundExecutor else {
                    throw FilebeamDomainError.invalidInput("Sign in and unlock this Inbox item to continue.")
                }
                try authenticatedExecutor.supplyInboxCookieContext(cookie, checkpointID: checkpointID)
            }
            try await executor.enqueue(checkpointID: association.checkpointID, direction: association.direction)
        } catch {
            await failBackground(jobID: jobID, message: error.localizedDescription)
        }
    }

    /// Manifest metadata is authenticated during preparation. Selection must finish before
    /// the executor asks Rust for descriptors, because that request freezes the selection.
    private func prepareBackgroundDownloadSelection(jobID: TransferJobID, checkpointID: String) async {
        do {
            let items = try await ffi { () -> [TransferSelectionItem] in
                let job = try self.job(jobID)
                guard job.direction != .upload else { return [] }
                return try self.client.backgroundTransfer().downloadItems(checkpointId: checkpointID).map { TransferSelectionItem(id: $0.id, name: $0.name, size: $0.size) }
            }
            if items.count <= 1 {
                if let item = items.first { try await ffi { try self.client.backgroundTransfer().selectDownloadItems(checkpointId: checkpointID, itemIds: [item.id]) } }
                await activateBackground(jobID: jobID, checkpointID: checkpointID)
                return
            }
            try await ffi {
                self.lock.lock(); defer { self.lock.unlock() }
                guard var job = self.jobs[jobID], case .preparing? = job.backgroundStage else { throw FilebeamDomainError.invalidInput("Background preparation is no longer active.") }
                let status = try self.client.backgroundTransfer().status(transferId: checkpointID)
                let promptID = self.backgroundPromptID(jobID: jobID)
                let association = BackgroundAssociation(jobID: jobID, origin: job.origin, remoteID: status.remoteId ?? job.transferID?.rawValue, checkpointID: checkpointID, kind: job.kind, direction: job.direction == .inboxDownload ? .inboxDownload : .download, includeKey: job.includeKey, selectionItems: items, selectionPromptID: promptID)
                self.backgroundAssociations[checkpointID] = association
                try self.saveBackgroundAssociations()
                job.backgroundStage = .selecting
                job.selectionItems = items
                job.selectionPromptID = promptID
                self.jobs[jobID] = job
            }
        } catch { await failBackground(jobID: jobID, message: error.localizedDescription) }
    }

    private func backgroundPromptID(jobID: TransferJobID) -> UInt64 {
        // Reserve a high-bit namespace so native prompt IDs cannot collide with this bridge prompt.
        0x8000_0000_0000_0000 | UInt64(bitPattern: Int64(jobID.uuidString.hashValue)) & 0x7fff_ffff_ffff_ffff
    }

    private func failBackground(jobID: TransferJobID, message: String) async {
        lock.withLock { guard var job = jobs[jobID] else { return }; job.backgroundStage = .failed(message); jobs[jobID] = job }
    }

    private func processBackgroundEvent(_ event: NativeBackgroundEvent) async {
        switch event {
        case let .progress(checkpointID, done, total):
            lock.withLock { guard let association = backgroundAssociations[checkpointID], var job = jobs[association.jobID] else { return }; job.backgroundProgress = (done, total); jobs[association.jobID] = job }
        case let .paused(checkpointID, _):
            lock.withLock { guard let association = backgroundAssociations[checkpointID], var job = jobs[association.jobID] else { return }; job.backgroundStage = .paused; jobs[association.jobID] = job }
        case let .failed(checkpointID, message):
            guard let association = lock.withLock({ backgroundAssociations[checkpointID] }) else { return }
            if association.direction == .upload { await reconcileBackgroundUpload(association) }
            else { await failBackground(jobID: association.jobID, message: message) }
        case let .readyToFinalize(checkpointID, direction):
            await finalizeBackground(checkpointID: checkpointID, direction: direction)
        }
    }

    private func reconcileBackgroundUpload(_ association: BackgroundAssociation) async {
        do {
            let job = try await ffi { self.client.backgroundTransfer().reconcileUpload(transferId: association.checkpointID) }
            lock.withLock { if var local = jobs[association.jobID] { local.handle = .transfer(job); local.backgroundStage = .network; jobs[association.jobID] = local } }
            let snapshot = try await ffi { job.snapshot() }
            if snapshot.state == .complete, let executor = lock.withLock({ backgroundExecutor }) { try await executor.resume(checkpointID: association.checkpointID, direction: .upload) }
        } catch { await failBackground(jobID: association.jobID, message: error.localizedDescription) }
    }

    private func finalizeBackground(checkpointID: String, direction: NativeBackgroundDirection) async {
        do {
            guard let association = lock.withLock({ () -> BackgroundAssociation? in
                guard let association = backgroundAssociations[checkpointID], finalizingBackgroundCheckpoints.insert(checkpointID).inserted else { return nil }
                return association
            }) else { return }
            let native = try await ffi { () throws -> TransferJob in
                switch direction {
                case .upload: return self.client.backgroundTransfer().finalizeUpload(transferId: checkpointID)
                case .download: return self.client.backgroundTransfer().finalizeDownload(transferId: checkpointID)
                case .inboxDownload:
                    guard let credentials = self.lock.withLock({ self.jobs[association.jobID]?.inboxCredentials }) else { throw FilebeamDomainError.invalidInput("Unlock and authorize this inbox download before finalizing it.") }
                    return try self.client.backgroundTransfer().finalizeInboxDownload(transferId: checkpointID, instance: association.origin, workingKey: credentials.workingKey, cookieContext: credentials.cookie)
                }
            }
            lock.withLock { if var job = jobs[association.jobID] { job.handle = .transfer(native); job.backgroundStage = .finalizing; jobs[association.jobID] = job } }
        } catch {
            let jobID = lock.withLock { () -> TransferJobID in
                finalizingBackgroundCheckpoints.remove(checkpointID)
                return backgroundAssociations[checkpointID]?.jobID ?? UUID()
            }
            await failBackground(jobID: jobID, message: error.localizedDescription)
        }
    }

    func adopt(job: TransferJob, transferID: TransferID? = nil, password: String? = nil, direction: FilebeamDomain.TransferDirection = .unknown, kind: FilebeamDomain.TransferKind = .unknown, transport: FilebeamDomain.Transport? = nil, origin: String = "", background: Bool = false, inboxCredentials: (workingKey: Data, cookie: String)? = nil, includeKey: Bool = true) -> TransferJobID {
        let id = UUID(); lock.lock(); jobs[id] = Job(handle: .transfer(job), transferID: transferID, password: password, direction: direction, kind: kind, transport: transport, verifiedNote: nil, includeKey: includeKey, origin: origin, backgroundStage: background ? .preparing : nil, backgroundProgress: nil, inboxCredentials: inboxCredentials); lock.unlock(); if background { startBackgroundPreparation(jobID: id) }; return id
    }
    private func adopt(note: ManagedNoteReceive, transferID: TransferID? = nil) -> TransferJobID {
        let id = UUID(); lock.lock(); jobs[id] = Job(handle: .note(note), transferID: transferID, password: nil, direction: .download, kind: .note, transport: nil, verifiedNote: nil); lock.unlock(); return id
    }
    private func ffi<T: Sendable>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { do { continuation.resume(returning: try body()) } catch { continuation.resume(throwing: Self.error(error)) } }
        }
    }
    private static func error(_ error: Error) -> FilebeamDomainError {
        if let domain = error as? FilebeamDomainError { return domain }
        if let native = error as? FilebeamCore.ClientError {
            switch native {
            case let .InvalidInput(detail): return .invalidInput(detail)
            case let .Operation(detail): return .remote(detail)
            }
        }
        return .remote(error.localizedDescription)
    }
    private func services(_ instance: FilebeamInstance) throws -> NativeServices {
        lock.lock(); defer { lock.unlock() }
        if let service = servicesByOrigin[instance.origin] { return service }
        let service: NativeServices
        if let cookie = try secrets.loadSession(origin: instance.origin) { service = try NativeServices.newWithRuntimeCookieContext(instance: instance.origin, allowHttp: false, cookieContext: cookie, runtime: runtime); rememberedOrigins.insert(instance.origin) }
        else { service = try NativeServices.newWithRuntime(instance: instance.origin, allowHttp: false, runtime: runtime) }
        servicesByOrigin[instance.origin] = service; return service
    }
    private func persist(_ service: NativeServices, origin: String) throws { guard rememberedOrigins.contains(origin) else { return }; try secrets.saveSession(origin: origin, cookie: try service.accountCookieContext()) }
    private func cryptoServices() throws -> NativeServices {
        try NativeServices.newWithRuntime(instance: "https://localhost", allowHttp: false, runtime: runtime)
    }
    private func job(_ id: TransferJobID) throws -> Job { lock.lock(); defer { lock.unlock() }; guard let job = jobs[id] else { throw FilebeamDomainError.invalidInput("Transfer job no longer exists.") }; return job }

    public func discover(instance: FilebeamInstance) async throws -> InstancePolicy {
        try await ffi {
            let service = try self.services(instance)
            let policy = try NativePolicyDecoder.decode(service.accountPolicyJson(), instance: instance)
            try self.persist(service, origin: instance.origin)
            return policy
        }
    }
    public func inspectReceive(instance: FilebeamInstance, input: String) async throws -> InputRoute {
        try await ffi {
            let value = try self.client.inspectLink(instance: instance.origin, input: input)
            let resolved = FilebeamInstance(origin: value.instance) ?? instance
            return .transfer(
                transferID: TransferID(value.id),
                instance: resolved,
                input: self.normalizedTransferInput(input, instance: resolved, id: value.id)
            )
        }
    }
    public func inspectPayload(instance: FilebeamInstance, input: String) async throws -> TransferInspection {
        try await ffi {
            let inspection = try self.client.inspectLink(instance: instance.origin, input: input)
            guard let origin = FilebeamInstance(origin: inspection.instance) else {
                throw FilebeamDomainError.invalidInput("The transfer has an invalid instance origin.")
            }
            let kind: FilebeamDomain.TransferKind = inspection.kind == "files" ? .files : inspection.kind == "note" ? .note : .unknown
            var burnOnRead = false
            if kind == .note {
                let link = self.normalizedTransferInput(input, instance: origin, id: inspection.id)
                burnOnRead = try self.services(origin).inspectNote(link: link).burnOnRead
            }
            return TransferInspection(kind: kind, transport: inspection.driver == "http" ? .http : inspection.driver == "webrtc" ? .webRTC : nil, passwordRequired: inspection.passwordRequired, burnOnRead: burnOnRead)
        }
    }
    public func downloadCommand(link: String) async throws -> String {
        try await ffi { try formatDownloadWithCli(target: link) }
    }
    public func startUpload(_ request: UploadStartRequest) async throws -> DTransferSnapshot {
        let id = try await ffi { () -> TransferJobID in
            let service = try self.services(request.instance); let cookie = try? service.accountCookieContext()
            let recipient = request.options.recipient.map { UploadRecipient(username: $0.username, userId: $0.userID, accountKeyBundleId: $0.keyBundleID, publicKey: $0.publicKey) }
            let options = UploadOptions(transport: request.options.transport == .http ? .http : .webRtc, turbo: request.options.turbo, archive: request.options.archive, password: request.options.passwordEnabled, retentionHours: request.options.retentionHours, authentication: UploadAuthentication(bearerToken: nil, sessionCookie: cookie), recipient: recipient)
            let background = self.lock.withLock { self.backgroundExecutor != nil } && request.options.transport == .http
            let sources = try request.sources.map { source -> UploadSource in
                let attributes = try FileManager.default.attributesOfItem(atPath: source.location)
                guard let size = attributes[.size] as? NSNumber, size.int64Value >= 0 else {
                    throw FilebeamDomainError.invalidInput("The selected source has no valid size.")
                }
                return UploadSource(kind: .path, name: source.name, pathOrIdentity: source.location, offset: 0, length: UInt64(size.int64Value), mutationToken: nil)
            }
            let job: TransferJob
            if background {
                job = sources.isEmpty ? try self.client.backgroundTransfer().prepareUpload(instance: request.instance.origin, paths: request.paths, options: options) : try self.client.backgroundTransfer().prepareUploadSources(instance: request.instance.origin, sources: sources, options: options)
            } else {
                job = sources.isEmpty ? try self.client.startUploadWithOptions(instance: request.instance.origin, paths: request.paths, options: options) : try self.client.startUploadSources(instance: request.instance.origin, sources: sources, options: options)
            }
            return self.adopt(job: job, password: request.password, direction: .upload, kind: .files, transport: request.options.transport, origin: request.instance.origin, background: background, includeKey: request.options.includeKeyInLink)
        }; return try await snapshot(jobID: id)
    }
    public func startNote(_ request: NoteStartRequest) async throws -> DTransferSnapshot {
        let id = UUID()
        let receipt = try await ffi { () -> ShareReceipt? in
            let service = try self.services(request.instance); let draft = request.draft
            if draft.options.live { let job = try service.startLiveNote(request: NoteRequest(text: draft.text, title: draft.title.isEmpty ? nil : draft.title, language: draft.language.rawValue, password: request.password, burnOnRead: draft.options.burnOnRead, retentionHours: draft.options.retentionHours, transport: .webRtc)); self.lock.lock(); self.jobs[id] = Job(handle: .transfer(job), transferID: nil, password: request.password, direction: .upload, kind: .note, transport: .webRTC, verifiedNote: nil, includeKey: draft.options.includeKeyInLink, origin: request.instance.origin); self.lock.unlock(); return ShareReceipt(transferID: TransferID(""), link: "", separateKey: nil, expiresAt: nil, isLive: true, serverAvailability: .waitingForRecipient) }
            let note = try service.startCreateNote(request: NoteRequest(text: draft.text, title: draft.title.isEmpty ? nil : draft.title, language: draft.language.rawValue, password: request.password, burnOnRead: draft.options.burnOnRead, retentionHours: draft.options.retentionHours, transport: .http))
            self.lock.lock(); self.jobs[id] = Job(handle: .creatingNote(note), transferID: nil, password: request.password, direction: .upload, kind: .note, transport: .http, verifiedNote: nil, includeKey: draft.options.includeKeyInLink, origin: request.instance.origin); self.lock.unlock()
            return nil
        }
        if let receipt, !receipt.transferID.rawValue.isEmpty { return TransferSnapshot(id: id, transferID: receipt.transferID, checkpointID: nil, lifecycle: .complete, phase: .complete, direction: .upload, kind: .note, transport: .http, completedBytes: 0, totalBytes: nil, shareURL: receipt.link, serverAvailability: .available) }
        return try await snapshot(jobID: id)
    }
    public func inspectNote(_ input: String) async throws -> FilebeamDomain.NoteInspection { try await ffi { guard let instance = self.instanceFromFullLink(input) else { throw FilebeamDomainError.invalidInput("Enter a full HTTPS note link.") }; let n = try self.services(instance).inspectNote(link: input); return FilebeamDomain.NoteInspection(id: TransferID(n.id), status: n.status, burnOnRead: n.burnOnRead, transport: n.transport == .http ? .http : .webRTC, passwordRequired: n.passwordRequired) } }
    public func startReceiveNote(_ request: FilebeamDomain.NoteReceiveRequest) async throws -> DTransferSnapshot { let id = try await ffi { self.adopt(note: try self.services(request.instance).startReceiveNote(request: FilebeamCore.NoteReceiveRequest(link: self.noteLink(request.input, separateKey: request.separateKey), password: request.password, burnAcknowledged: request.burnAcknowledged))) }; return try await snapshot(jobID: id) }
    public func takeVerifiedNote(jobID: TransferJobID) async throws -> VerifiedNote? { try await ffi { self.lock.lock(); defer { self.lock.unlock() }; guard var job = self.jobs[jobID] else { throw FilebeamDomainError.invalidInput("Transfer job no longer exists.") }; if let note = job.verifiedNote { return note }; guard case let .note(native) = job.handle, let value = native.takeNote() else { return nil }; let note = VerifiedNote(id: TransferID(value.id), text: value.text, title: value.title, language: NoteLanguage(rawValue: value.language) ?? .plain, consumed: value.consumed); job.verifiedNote = note; self.jobs[jobID] = job; return note } }
    public func retryNoteBurn(jobID: TransferJobID) async throws -> Bool { try await ffi { self.lock.lock(); defer { self.lock.unlock() }; guard var job = self.jobs[jobID], case let .note(note) = job.handle else { return false }; let burned = try note.retryBurn(); if burned, var verified = job.verifiedNote { verified = VerifiedNote(id: verified.id, text: verified.text, title: verified.title, language: verified.language, consumed: true); job.verifiedNote = verified; self.jobs[jobID] = job }; return burned } }
    public func receive(_ request: ReceiveRequest) async throws -> DTransferSnapshot { let id = try await ffi { let inspection = try self.client.inspectLink(instance: request.instance.origin, input: request.input); let isHTTP = inspection.driver == "http"; let prepareOnly = inspection.driver == "webrtc"; let background = prepareOnly || (isHTTP && self.lock.withLock { self.backgroundExecutor != nil }); let job = background ? try self.client.backgroundTransfer().prepareDownload(instance: request.instance.origin, link: request.input, outputDirectory: request.outputDirectory) : try self.client.startDownload(instance: request.instance.origin, link: request.input, outputDirectory: request.outputDirectory); return self.adopt(job: job, transferID: TransferID(inspection.id), direction: .download, kind: .files, transport: isHTTP ? .http : .webRTC, origin: request.instance.origin, background: background) }; return try await snapshot(jobID: id) }
    public func startInboxReceive(_ request: InboxReceiveRequest) async throws -> DTransferSnapshot { let id = try await ffi { let background = self.lock.withLock { self.backgroundExecutor != nil }; let job = background ? try self.client.backgroundTransfer().prepareInboxDownload(instance: request.instance.origin, transferId: request.transferID.rawValue, workingKey: request.workingKey, cookieContext: request.cookieContext, outputDirectory: request.outputDirectory) : try self.client.startInboxDownload(instance: request.instance.origin, transferId: request.transferID.rawValue, workingKey: request.workingKey, cookieContext: request.cookieContext, outputDirectory: request.outputDirectory); return self.adopt(job: job, transferID: request.transferID, direction: .inboxDownload, kind: .files, transport: .http, origin: request.instance.origin, background: background, inboxCredentials: (request.workingKey, request.cookieContext)) }; return try await snapshot(jobID: id) }
    public func resumeInboxReceive(_ request: InboxResumeRequest) async throws -> DTransferSnapshot { let id = try await ffi { self.adopt(job: try self.client.resumeInboxDownload(checkpointId: request.checkpointID, instance: request.instance.origin, workingKey: request.workingKey, cookieContext: request.cookieContext), direction: .inboxDownload, kind: .files, transport: .http) }; return try await snapshot(jobID: id) }
    public func receiveInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data, outputDirectory: String) async throws -> DTransferSnapshot {
        let credentials = try await ffi { () -> (Data, String) in
            let service = try self.services(instance)
            return (try service.accountOpenInboxKey(transferId: transferID.rawValue, privateKey: privateKey), try service.accountCookieContext())
        }
        if let association = lock.withLock({ backgroundAssociations.values.first { $0.origin == instance.origin && $0.remoteID == transferID.rawValue && $0.direction == .inboxDownload } }) {
            lock.withLock {
                if var job = jobs[association.jobID] {
                    job.inboxCredentials = (credentials.0, credentials.1)
                    jobs[association.jobID] = job
                }
            }
            if let executor = lock.withLock({ backgroundExecutor }) as? any NativeInboxBackgroundExecutor {
                try executor.supplyInboxCookieContext(credentials.1, checkpointID: association.checkpointID)
            }
            try await resumeBackground(checkpointID: association.checkpointID)
            return try await snapshot(jobID: association.jobID)
        }
        return try await startInboxReceive(InboxReceiveRequest(instance: instance, transferID: transferID, workingKey: credentials.0, cookieContext: credentials.1, outputDirectory: outputDirectory))
    }
    public func snapshot(jobID: TransferJobID) async throws -> DTransferSnapshot {
        try await ffi {
            var job = try self.job(jobID)
            if case let .completed(receipt) = job.handle {
                return self.completedSnapshot(jobID: jobID, receipt: receipt)
            }
            let native: FilebeamCore.TransferSnapshot
            switch job.handle {
            case let .transfer(value): native = value.snapshot()
            case let .note(value): native = value.snapshot()
            case let .creatingNote(value): native = value.snapshot()
            case .backgroundOnly: return try self.backgroundOnlySnapshot(jobID: jobID, job: job)
            case .completed: throw FilebeamDomainError.invalidInput("Completed work has no native handle.")
            }
            if case let .creatingNote(value) = job.handle, let created = value.takeReceipt() {
                let receipt = try self.storeReceipt(transferID: TransferID(created.id), shareLink: created.link, origin: job.origin, includeKey: job.includeKey, deleteToken: nil, isLive: false)
                job.handle = .completed(receipt)
                job.transferID = receipt.transferID
                self.lock.withLock { self.jobs[jobID] = job }
                return self.completedSnapshot(jobID: jobID, receipt: receipt, completedBytes: native.done, totalBytes: native.total)
            }
            if native.state == .complete, let shareLink = native.shareUrl, job.direction == .upload, ({ if job.backgroundStage == nil { return true }; if case .finalizing? = job.backgroundStage { return true }; return false }()) {
                let inspected = try self.client.inspectLink(instance: job.origin, input: shareLink)
                let receipt = try self.storeReceipt(transferID: TransferID(inspected.id), shareLink: shareLink, origin: job.origin, includeKey: job.includeKey, deleteToken: nil, isLive: job.kind == .note && job.transport == .webRTC)
                job.transferID = receipt.transferID
                if case .finalizing? = job.backgroundStage, let checkpointID = self.backgroundAssociations.values.first(where: { $0.jobID == jobID })?.checkpointID {
                    self.backgroundAssociations.removeValue(forKey: checkpointID)
                    self.finalizingBackgroundCheckpoints.remove(checkpointID)
                    try self.saveBackgroundAssociations()
                }
                job.backgroundStage = nil
                self.lock.withLock { self.jobs[jobID] = job }
                return self.completedSnapshot(jobID: jobID, receipt: receipt, completedBytes: native.done, totalBytes: native.total, kind: job.kind, transport: job.transport)
            }
            if let password = job.password, native.prompt?.kind == .password, case let .transfer(value) = job.handle {
                try value.respond(promptId: native.prompt!.id, value: password)
                job.password = nil
                self.lock.withLock { self.jobs[jobID] = job }
            }
            return self.map(native, id: jobID, job: job)
        }
    }
    public func savedTransfers() async throws -> [TransferRecord] {
        try await ffi {
            var records = try self.client.savedTransfers().map { saved -> TransferRecord in
                let value = try self.client.savedTransferDetails(id: saved.id)
                let direction = self.direction(value.direction)
                let export: ExportStatus = direction == .download ? (self.exportedPaths[value.id]?.isEmpty == false ? .exported : .notExported) : .notAvailable
                return TransferRecord(id: TransferID(value.id), direction: direction, kind: TransferKind(rawValue: value.kind) ?? .unknown, transport: value.transport == "http" ? .http : value.transport == "webrtc" ? .webRTC : nil, phase: self.phase(value.state), verifiedPrivately: value.verifiedPrivately, exportStatus: export, expiresAt: self.date(value.expiresAt), serverAvailability: .unknown, actions: TransferActions(resume: value.canResume, retryExport: value.canRetrySave && export != .exported, discardLocal: value.canRemoveLocal, revokeRemote: value.canRevokeRemote, endLive: value.canEndLive))
            }
            let known = Set(records.map { $0.id.rawValue })
            records += self.storedReceipts.values.compactMap { stored in
                guard !known.contains(stored.receipt.transferID.rawValue) else { return nil }
                return TransferRecord(id: stored.receipt.transferID, direction: .upload, kind: stored.receipt.isLive ? .note : .files, transport: .http, phase: .complete, verifiedPrivately: true, exportStatus: .notAvailable, expiresAt: stored.receipt.expiresAt, serverAvailability: stored.receipt.serverAvailability, actions: TransferActions(revokeRemote: stored.deleteToken != nil, endLive: stored.receipt.isLive))
            }
            return records
        }
    }
    public func pause(jobID: TransferJobID) async throws {
        try await ffi {
            if case let .transfer(native) = try self.job(jobID).handle, native.snapshot().state == .running {
                native.pause()
            }
        }
        if let association = lock.withLock({ backgroundAssociations.values.first { $0.jobID == jobID } }), let executor = lock.withLock({ backgroundExecutor }) {
            try await executor.pause(checkpointID: association.checkpointID)
            lock.withLock { if var job = jobs[jobID] { job.backgroundStage = .paused; jobs[jobID] = job } }
            return
        }
        try await ffi { switch try self.job(jobID).handle { case let .transfer(value): value.pause(); case let .note(value): value.cancel(); case .creatingNote: throw FilebeamDomainError.invalidInput("Note creation cannot be paused."); case .completed, .backgroundOnly: throw FilebeamDomainError.invalidInput("Completed work cannot be paused.") } }
    }

    /// Restored background work is resumed only through this explicit action.
    public func resumeBackground(checkpointID: String) async throws {
        let association = try lock.withLock { () throws -> BackgroundAssociation in
            guard let value = backgroundAssociations[checkpointID] else { throw FilebeamDomainError.invalidInput("Unknown background checkpoint.") }
            return value
        }
        if lock.withLock({ jobs[association.jobID]?.selectionItems.isEmpty == false }) {
            lock.withLock { if var job = jobs[association.jobID] { job.backgroundStage = .selecting; jobs[association.jobID] = job } }
            return
        }
        if lock.withLock({ jobs[association.jobID]?.transport == .webRTC }) {
            let native = try await ffi { self.client.resume(id: checkpointID) }
            try await ffi {
                self.lock.lock(); defer { self.lock.unlock() }
                guard var job = self.jobs[association.jobID] else { throw FilebeamDomainError.invalidInput("WebRTC checkpoint is no longer active.") }
                job.handle = .transfer(native)
                job.backgroundStage = nil
                self.jobs[association.jobID] = job
                self.backgroundAssociations.removeValue(forKey: checkpointID)
                try self.saveBackgroundAssociations()
            }
            return
        }
        guard let executor = lock.withLock({ backgroundExecutor }) else { throw FilebeamDomainError.remote("Background HTTP executor is unavailable.") }
        try await executor.resume(checkpointID: checkpointID, direction: association.direction)
        lock.withLock { if var job = jobs[association.jobID] { job.backgroundStage = .network; jobs[association.jobID] = job } }
    }
    /// Uses the app's stable job ID rather than exposing a checkpoint as UI identity.
    public func resumeBackground(jobID: TransferJobID) async throws {
        let checkpointID = try lock.withLock { () throws -> String in
            guard let checkpointID = backgroundAssociations.values.first(where: { $0.jobID == jobID })?.checkpointID else { throw FilebeamDomainError.invalidInput("This job has no background checkpoint.") }
            return checkpointID
        }
        try await resumeBackground(checkpointID: checkpointID)
    }
    public func resume(transferID: TransferID) async throws -> DTransferSnapshot {
        if let association = lock.withLock({ backgroundAssociations[transferID.rawValue] ?? backgroundAssociations.values.first(where: { $0.remoteID == transferID.rawValue }) }) {
            try await resumeBackground(jobID: association.jobID)
            return try await snapshot(jobID: association.jobID)
        }
        let id = try await ffi { self.adopt(job: self.client.resume(id: transferID.rawValue), transferID: transferID) }
        return try await snapshot(jobID: id)
    }
    public func discardLocal(transferID: TransferID) async throws { try await ffi { try self.client.discard(id: transferID.rawValue) } }
    public func recordExport(jobID: TransferJobID, paths: [String]) async throws {
        try await ffi {
            guard !paths.isEmpty, paths.allSatisfy({ !$0.isEmpty }) else {
                throw FilebeamDomainError.invalidInput("An export must contain at least one output path.")
            }
            let job = try self.job(jobID)
            let native: FilebeamCore.TransferSnapshot
            guard case let .transfer(value) = job.handle else {
                throw FilebeamDomainError.invalidInput("Only a verified download can be recorded as exported.")
            }
            native = value.snapshot()
            guard job.direction == .download || job.direction == .inboxDownload, native.state == .complete, !native.results.filter({ $0.hasPrefix("/") }).isEmpty else {
                throw FilebeamDomainError.invalidInput("Export is available only after native verification produced local outputs.")
            }
            let stableIDs = [job.transferID?.rawValue, native.checkpointId, jobID.uuidString].compactMap { $0 }
            self.lock.lock()
            stableIDs.forEach { self.exportedPaths[$0] = paths }
            self.lock.unlock()
            try self.saveExports()
        }
    }
    public func revokeRemote(transferID: TransferID) async throws -> DTransferSnapshot {
        if let stored = lock.withLock({ storedReceipts[transferID.rawValue] }), let deleteToken = stored.deleteToken {
            guard let instance = FilebeamInstance(origin: stored.origin) else { throw FilebeamDomainError.storage("The retained receipt has an invalid instance origin.") }
            try await ffi { try self.services(instance).revokeNote(transferId: transferID.rawValue, deleteToken: deleteToken) }
            return DTransferSnapshot(id: UUID(), transferID: transferID, checkpointID: nil, lifecycle: .complete, phase: .revoked, direction: .upload, kind: .note, transport: .http, completedBytes: 0, totalBytes: nil, serverAvailability: .unavailable)
        }
        let id = try await ffi { self.adopt(job: self.client.revokeUpload(id: transferID.rawValue), transferID: transferID) }
        return try await snapshot(jobID: id)
    }
    public func endLive(transferID: TransferID) async throws -> DTransferSnapshot {
        if let jobID = lock.withLock({ jobs.first(where: { $0.value.transferID == transferID && $0.value.kind == .note && $0.value.transport == .webRTC })?.key }) {
            try await ffi { guard case let .transfer(job) = try self.job(jobID).handle else { throw FilebeamDomainError.invalidInput("Live note is no longer active.") }; job.endLive() }
            return try await snapshot(jobID: jobID)
        }
        let id = try await ffi { self.adopt(job: self.client.endLive(id: transferID.rawValue), transferID: transferID) }
        return try await snapshot(jobID: id)
    }
    public func respondToConsent(jobID: TransferJobID, promptID: UInt64, allowed: Bool) async throws { try await ffi { switch try self.job(jobID).handle { case let .transfer(value): try value.respondConsent(promptId: promptID, allowed: allowed); case let .note(value): try value.respondConsent(promptId: promptID, allowed: allowed); case .creatingNote, .completed, .backgroundOnly: throw FilebeamDomainError.invalidInput("Completed work has no prompt.") } } }
    public func respondToPrompt(jobID: TransferJobID, promptID: UInt64, value: String) async throws {
        let selection = try await ffi { () -> (checkpointID: String, ids: [String])? in
            let job = try self.job(jobID)
            guard ({ if case .selecting? = job.backgroundStage { return true }; return false }()), job.selectionPromptID == promptID else { return nil }
            guard let data = value.data(using: .utf8), let ids = try? JSONDecoder().decode([String].self, from: data), !ids.isEmpty else { throw FilebeamDomainError.invalidInput("Select at least one file.") }
            let authorized = Set(job.selectionItems.map(\.id))
            guard ids.count == Set(ids).count, Set(ids) == Set(ids.filter { authorized.contains($0) }) else { throw FilebeamDomainError.invalidInput("The selected files no longer match this transfer.") }
            guard let checkpointID = self.backgroundAssociations.values.first(where: { $0.jobID == jobID })?.checkpointID else { throw FilebeamDomainError.invalidInput("Background selection is unavailable.") }
            return (checkpointID, ids)
        }
        if let selection {
            try await ffi { try self.client.backgroundTransfer().selectDownloadItems(checkpointId: selection.checkpointID, itemIds: selection.ids) }
            await activateBackground(jobID: jobID, checkpointID: selection.checkpointID)
            return
        }
        try await ffi { let job = try self.job(jobID); switch job.handle { case let .transfer(native): let prompt = native.snapshot().prompt; if prompt?.kind == .directory { try native.respondDirectory(promptId: promptID, choice: value == "zip" ? .zip : .individualFiles) } else { try native.respond(promptId: promptID, value: value) }; case let .note(native): try native.respond(promptId: promptID, value: value); case .creatingNote, .completed, .backgroundOnly: throw FilebeamDomainError.invalidInput("Completed work has no prompt.") } }
    }
    public func transferActivity(checkpointID: String) async throws -> FilebeamDomain.TransferActivity {
        try await ffi { let value = try self.client.transferActivity(checkpointId: checkpointID); return FilebeamDomain.TransferActivity(records: value.records.map { TransferActivityRecord(id: $0.id, number: $0.number, progress: $0.progress, state: $0.state, selectionCount: $0.selectionCount, allFiles: $0.allFiles) }, unavailable: value.unavailable, retry: value.retry) }
    }
    public func login(_ request: LoginRequest) async throws -> DAccountSession { try await ffi { let s = try self.services(request.instance); let value = try s.accountLogin(email: request.email, password: request.password, remember: request.remember); if request.remember { self.rememberedOrigins.insert(request.instance.origin); try self.persist(s, origin: request.instance.origin) } else { self.rememberedOrigins.remove(request.instance.origin); try self.secrets.saveSession(origin: request.instance.origin, cookie: nil) }; return self.session(value) } }
    public func register(_ request: RegistrationRequest) async throws -> DAccountSession { try await ffi { let s = try self.services(request.instance); let value = try s.accountRegister(username: request.username, name: request.name, email: request.email, password: request.password); self.rememberedOrigins.remove(request.instance.origin); try self.secrets.saveSession(origin: request.instance.origin, cookie: nil); return self.session(value) } }
    public func logout(instance: FilebeamInstance) async throws { try await ffi { let s = try self.services(instance); try s.accountLogout(); self.lock.lock(); self.servicesByOrigin.removeValue(forKey: instance.origin); self.rememberedOrigins.remove(instance.origin); self.lock.unlock(); try self.secrets.saveSession(origin: instance.origin, cookie: nil) } }
    public func session(instance: FilebeamInstance) async throws -> DAccountSession { try await ffi { self.session(try self.services(instance).accountSession()) } }
    public func requestPasswordReset(instance: FilebeamInstance, email: String) async throws { try await ffi { try self.services(instance).accountRequestPasswordReset(email: email) } }
    public func resetPassword(instance: FilebeamInstance, email: String, token: String, password: String) async throws { try await ffi { let service = try self.services(instance); try service.accountResetPassword(email: email, token: token, password: password); try self.persist(service, origin: instance.origin) } }
    public func resendVerification(instance: FilebeamInstance) async throws { try await ffi { let service = try self.services(instance); try service.accountResendVerification(); try self.persist(service, origin: instance.origin) } }
    public func verifyEmail(link: String) async throws { try await ffi { let route = InputRouter.route(link, selectedInstance: FilebeamInstance(origin: "https://invalid.invalid")!); guard case .verifyEmail = route, let url = URL(string: link), let scheme = url.scheme, let host = url.host, let instance = FilebeamInstance(origin: "\(scheme)://\(host)\(url.port.map { ":\($0)" } ?? "")") else { throw FilebeamDomainError.invalidInput("Invalid verification link.") }; let service = try self.services(instance); try service.accountVerifyEmailLink(link: link); try self.persist(service, origin: instance.origin) } }
    public func recipient(instance: FilebeamInstance, username: String) async throws -> DRecipient { try await ffi { let r = try self.services(instance).accountRecipient(username: username); return DRecipient(userID: r.id, username: r.username, keyBundleID: r.accountKeyBundleId, publicKey: r.publicKey, fingerprint: r.fingerprint) } }
    public func inbox(instance: FilebeamInstance) async throws -> [InboxItem] { try await ffi { try self.services(instance).accountInbox().map { InboxItem(id: TransferID($0.id), ciphertextBytes: $0.ciphertextBytes, itemCount: $0.itemCount, completedAt: self.date($0.completedAt), expiresAt: self.date($0.expiresAt)) } } }
    public func inboxUnreadCount(instance: FilebeamInstance) async throws -> InboxUnreadCount { try await ffi { InboxUnreadCount(count: try self.services(instance).accountInboxUnreadCount()) } }
    public func markInboxNotificationsRead(instance: FilebeamInstance) async throws { try await ffi { let service = try self.services(instance); try service.accountMarkInboxNotificationsRead(); try self.persist(service, origin: instance.origin) } }
    public func deleteInboxItem(instance: FilebeamInstance, transferID: TransferID) async throws { try await ffi { let service = try self.services(instance); try service.accountDeleteInboxItem(transferId: transferID.rawValue); try self.persist(service, origin: instance.origin) } }
    public func inboxMetadata(instance: FilebeamInstance, transferID: TransferID) async throws -> DInboxMetadata { try await ffi { let m = try self.services(instance).accountInboxMetadata(transferId: transferID.rawValue); return DInboxMetadata(transferID: TransferID(m.id), encryptedManifest: m.encryptedManifest, recipientKeyBundle: self.bundle(m.recipientKey.bundle), encryptedRecipientKey: m.recipientKey.encryptedKey) } }
    public func openInbox(instance: FilebeamInstance, transferID: TransferID, privateKey: Data) async throws -> DOpenedInbox { try await ffi { let value = try self.services(instance).accountOpenInbox(transferId: transferID.rawValue, privateKey: privateKey); return DOpenedInbox(transferID: TransferID(value.transferId), keyBundleID: value.keyBundleId, filenames: value.filenames) } }
    public func accountKeys(instance: FilebeamInstance) async throws -> [DAccountKeyBundle] { try await ffi { try self.services(instance).accountKeys().map(self.bundle) } }
    public func keySituation(instance: FilebeamInstance) async throws -> KeySituation { try await ffi { let s = try self.services(instance).accountKeySituation(); return KeySituation(activeBundleID: s.activeBundleId, historicalBundleIDs: s.historicalBundleIds, replacementAcknowledgementRequired: s.replacementAcknowledgementRequired) } }
    public func generateSelfCustodyKey() async throws -> DGeneratedAccountKey { try await ffi { let k = try self.cryptoServices().accountGenerateSelfKey(); return DGeneratedAccountKey(privateKey: k.privateKey, publicKey: k.publicKey, fingerprint: k.fingerprint) } }
    public func wrapPasswordCustodyKey(privateKey: Data, password: String, userID: UInt64, publicKey: String) async throws -> String { try await ffi { try self.cryptoServices().accountWrapPasswordKey(privateKey: privateKey, password: password, userId: userID, publicKey: publicKey) } }
    public func unwrapPasswordCustodyKey(envelope: String, password: String, userID: UInt64, publicKey: String) async throws -> Data { try await ffi { try self.cryptoServices().accountUnwrapPasswordKey(envelope: envelope, password: password, userId: userID, publicKey: publicKey) } }
    public func validateAccountKeyUpload(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> AccountKeyUploadValidation { try await ffi { let keys = try self.services(instance).accountKeys(); let replacement = keys.contains { $0.isActive }; guard request.custody == .password || request.encryptedPrivateKey == nil else { return AccountKeyUploadValidation(isValid: false, reason: "Self-custody keys must not upload private material.", replacementAcknowledgementRequired: replacement) }; return AccountKeyUploadValidation(isValid: !request.publicKey.isEmpty && !request.fingerprint.isEmpty, reason: request.publicKey.isEmpty || request.fingerprint.isEmpty ? "Public key and fingerprint are required." : nil, replacementAcknowledgementRequired: replacement) } }
    public func uploadAccountKey(instance: FilebeamInstance, request: AccountKeyUploadRequest) async throws -> DAccountKeyBundle { try await ffi { let service = try self.services(instance); let bundle = try service.accountUploadKey(key: FilebeamCore.AccountKeyUpload(publicKey: request.publicKey, fingerprint: request.fingerprint, custodyMode: request.custody == .password ? "password" : "self", encryptedPrivateKey: request.encryptedPrivateKey, currentPassword: request.currentPassword, replace: request.replace)); try self.persist(service, origin: instance.origin); return self.bundle(bundle) } }
    public func exportSelfCustodyKey(_ privateKey: Data) async throws -> String { try await ffi { try self.cryptoServices().accountExportSelfKey(privateKey: privateKey) } }
    public func importSelfCustodyKey(_ encoded: String) async throws -> Data { try await ffi { try self.cryptoServices().accountImportSelfKey(value: encoded) } }
    public func preferences(instance: FilebeamInstance) async throws -> AccountPreferences { try await ffi { let s = try self.services(instance).accountSession(); return AccountPreferences(inboxEnabled: s.inboxEnabled, notificationChannel: s.notificationChannel) } }
    public func updatePreferences(instance: FilebeamInstance, preferences: AccountPreferences) async throws { try await ffi { let s = try self.services(instance); try s.accountSetInboxEnabled(enabled: preferences.inboxEnabled); try s.accountSetNotificationChannel(channel: preferences.notificationChannel); try self.persist(s, origin: instance.origin) } }
    public func receipt(transferID: TransferID, includeKey: Bool) async throws -> ShareReceipt {
        try await ffi {
            guard let stored = self.lock.withLock({ self.storedReceipts[transferID.rawValue] }) else {
                throw FilebeamDomainError.storage("This receipt was not retained on this device.")
            }
            return try self.presentReceipt(stored, includeKey: includeKey)
        }
    }
    public func inspectInvite(link: String) async throws -> InviteInspection {
        try await ffi {
            guard let instance = self.instanceFromFullLink(link),
                  case .invite = InputRouter.route(link, selectedInstance: instance),
                  let token = URL(string: link)?.lastPathComponent else {
                throw FilebeamDomainError.invalidInput("Invalid invitation link.")
            }
            let value = try self.services(instance).accountInspectInvitation(token: token)
            return InviteInspection(instance: instance, invitationID: token, email: value.email, expiresAt: self.date(value.expiresAt))
        }
    }
    public func acceptInvite(_ request: InviteAcceptanceRequest) async throws -> DAccountSession {
        try await ffi {
            guard let instance = self.instanceFromFullLink(request.link),
                  case .invite = InputRouter.route(request.link, selectedInstance: instance),
                  let token = URL(string: request.link)?.lastPathComponent else {
                throw FilebeamDomainError.invalidInput("Invalid invitation link.")
            }
            let service = try self.services(instance)
            let value = try service.accountAcceptInvitation(token: token, username: request.username, name: request.name, email: request.email, password: request.password)
            try self.persist(service, origin: instance.origin)
            return self.session(value)
        }
    }
    public func report(_ request: ReportRequest) async throws -> ReportReceipt { try await ffi { try self.services(request.instance).accountReport(transferId: request.transferID.rawValue, category: request.category.rawValue, description: request.description, email: request.email); return ReportReceipt(reportID: nil, submittedAt: nil) } }
    public func requestAccountDeletion(_ request: AccountDeletionRequest) async throws -> AccountDeletionResult { try await ffi { let s = try self.services(request.instance); let status = try s.accountDelete(currentPassword: request.currentPassword, confirmation: request.confirmation); self.lock.withLock { self.servicesByOrigin.removeValue(forKey: request.instance.origin); self.rememberedOrigins.remove(request.instance.origin) }; try self.secrets.saveSession(origin: request.instance.origin, cookie: nil); return AccountDeletionResult(accepted: status == "deletion_scheduled", effectiveAt: nil) } }

    private func normalizedTransferInput(_ input: String, instance: FilebeamInstance, id: String) -> String {
        let base = "\(instance.origin)/\(id)"
        guard let fragment = URLComponents(string: input)?.fragment, !fragment.isEmpty else { return base }
        return "\(base)#\(fragment)"
    }
    private func instanceFromFullLink(_ input: String) -> FilebeamInstance? {
        guard var components = URLComponents(string: input), components.scheme?.lowercased() == "https", components.host != nil,
              components.user == nil, components.password == nil else { return nil }
        components.path = ""
        components.query = nil
        components.fragment = nil
        return components.string.flatMap { FilebeamInstance(origin: $0) }
    }
    private func noteLink(_ input: String, separateKey: String?) -> String {
        guard let separateKey, !separateKey.isEmpty, URLComponents(string: input)?.fragment == nil else { return input }
        return "\(input)#\(separateKey)"
    }
    private func storeReceipt(transferID: TransferID, shareLink: String, origin: String, includeKey: Bool, deleteToken: String?, isLive: Bool) throws -> ShareReceipt {
        let split = try splitShareLink(instance: origin, link: shareLink)
        let presentation = try presentShareLink(instance: origin, shareUrl: split.link, shareKey: split.separateKey, includeKey: includeKey)
        let receipt = ShareReceipt(transferID: transferID, link: presentation.link, separateKey: includeKey ? nil : presentation.separateKey, expiresAt: nil, isLive: isLive, serverAvailability: .available)
        let stored = StoredReceipt(receipt: receipt, deleteToken: deleteToken, origin: origin, exportedPaths: [], canonicalShareURL: split.link, canonicalShareKey: split.separateKey)
        lock.withLock { receipts[transferID] = receipt; storedReceipts[transferID.rawValue] = stored }
        try saveReceipts()
        return receipt
    }
    private func completedSnapshot(jobID: TransferJobID, receipt: ShareReceipt, completedBytes: UInt64 = 0, totalBytes: UInt64? = nil, kind: TransferKind = .note, transport: FilebeamDomain.Transport? = .http) -> DTransferSnapshot {
        DTransferSnapshot(id: jobID, transferID: receipt.transferID, checkpointID: nil, lifecycle: .complete, phase: .complete, direction: .upload, kind: kind, transport: transport, completedBytes: completedBytes, totalBytes: totalBytes, shareURL: receipt.link, serverAvailability: receipt.serverAvailability)
    }
    private func presentReceipt(_ stored: StoredReceipt, includeKey: Bool) throws -> ShareReceipt {
        guard let shareURL = stored.canonicalShareURL, let shareKey = stored.canonicalShareKey else {
            if includeKey || stored.receipt.separateKey != nil { return stored.receipt }
            return stored.receipt
        }
        let presentation = try presentShareLink(instance: stored.origin, shareUrl: shareURL, shareKey: shareKey, includeKey: includeKey)
        return ShareReceipt(transferID: stored.receipt.transferID, link: presentation.link, separateKey: includeKey ? nil : presentation.separateKey, expiresAt: stored.receipt.expiresAt, isLive: stored.receipt.isLive, serverAvailability: stored.receipt.serverAvailability)
    }
    private func presentedShareURL(_ link: String?, job: Job) -> String? {
        guard let link, let split = try? splitShareLink(instance: job.origin, link: link), let presentation = try? presentShareLink(instance: job.origin, shareUrl: split.link, shareKey: split.separateKey, includeKey: job.includeKey) else { return link }
        return presentation.link
    }
    private func session(_ value: FilebeamCore.AccountSession) -> DAccountSession { DAccountSession(id: value.id, name: value.name, username: value.username, email: value.email, emailVerifiedAt: date(value.emailVerifiedAt), profileURL: value.profileUrl, inboxEnabled: value.inboxEnabled, usernameRoutingEnabled: value.usernameRoutingEnabled, notificationChannel: value.notificationChannel) }
    private func bundle(_ value: FilebeamCore.AccountKeyBundle) -> DAccountKeyBundle { DAccountKeyBundle(id: value.id, userID: value.userId, version: value.version, publicKey: value.publicKey, fingerprint: value.fingerprint, custody: value.custodyMode == "password" ? .password : .selfCustody, encryptedPrivateKey: value.encryptedPrivateKey, isActive: value.isActive) }
    private func date(_ value: String?) -> Date? { value.flatMap { ISO8601DateFormatter().date(from: $0) } }
    private func direction(_ value: String) -> TransferDirection { TransferDirection(rawValue: value) ?? .unknown }
    private func phase(_ value: String) -> TransferPhase {
        switch value {
        case "sending": return .uploading
        case "receiving": return .receiving
        case "complete": return .complete
        case "verifying": return .verifying
        case "paused": return .paused
        case "preparing": return .preparing
        default: return TransferPhase(rawValue: value) ?? .unknown
        }
    }
    private func map(_ value: FilebeamCore.TransferSnapshot, id: UUID, job: Job) -> DTransferSnapshot {
        let prompt = value.prompt.map { TransferPrompt(id: $0.id, kind: $0.kind == .shareKey ? .decryptionKey : $0.kind == .password ? .password : $0.kind == .peerConsent ? .peerConsent : $0.kind == .directory ? .directoryChoice : .shareReady, message: $0.peer) }
        if let stage = job.backgroundStage {
            let checkpoint = backgroundAssociations.values.first(where: { $0.jobID == id })?.checkpointID ?? value.results.first ?? value.checkpointId
            let progress = job.backgroundProgress
            switch stage {
            case .preparing:
                return DTransferSnapshot(id: id, transferID: job.transferID, checkpointID: checkpoint, lifecycle: value.state == .failed ? .failed : .running, phase: value.state == .failed ? .failed : prompt == nil ? .preparing : .awaitingInput, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: value.done, totalBytes: value.total, prompt: prompt, shareURL: presentedShareURL(value.shareUrl, job: job), error: value.error, actions: TransferActions(pause: value.canPause))
            case .selecting:
                let prompt = job.selectionPromptID.map { TransferPrompt(id: $0, kind: .filesSelection, message: "Choose the files to download.", selectionItems: job.selectionItems) }
                return DTransferSnapshot(id: id, transferID: job.transferID, checkpointID: checkpoint, lifecycle: .running, phase: .awaitingInput, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: value.done, totalBytes: value.total, prompt: prompt, actions: TransferActions(pause: true))
            case .network:
                return DTransferSnapshot(id: id, transferID: job.transferID, checkpointID: checkpoint, lifecycle: .running, phase: job.direction == .upload ? .uploading : .receiving, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: progress?.done ?? 0, totalBytes: progress?.total, actions: TransferActions(pause: true))
            case .finalizing:
                return DTransferSnapshot(id: id, transferID: job.transferID, checkpointID: checkpoint, lifecycle: value.state == .failed ? .failed : .running, phase: value.state == .failed ? .failed : prompt != nil ? .awaitingInput : job.direction == .upload ? .uploading : .verifying, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: progress?.done ?? value.done, totalBytes: progress?.total ?? value.total, prompt: prompt, error: value.error, actions: TransferActions(pause: value.canPause, resume: value.state == .failed))
            case let .failed(message):
                return DTransferSnapshot(id: id, transferID: job.transferID, checkpointID: checkpoint, lifecycle: .failed, phase: .failed, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: progress?.done ?? value.done, totalBytes: progress?.total ?? value.total, error: message, actions: TransferActions(resume: checkpoint != nil))
            case .paused:
                return DTransferSnapshot(id: id, transferID: job.transferID, checkpointID: checkpoint, lifecycle: .paused, phase: .paused, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: progress?.done ?? 0, totalBytes: progress?.total, actions: TransferActions(resume: checkpoint != nil))
            }
        }
        let isFileDownload = job.kind == .files && (job.direction == .download || job.direction == .inboxDownload)
        let export: ExportStatus = isFileDownload && self.exportedPaths[job.transferID?.rawValue ?? value.checkpointId ?? id.uuidString]?.isEmpty == false ? .exported : isFileDownload ? .notExported : .notAvailable
        let canRetryBurn: Bool
        if case let .note(note) = job.handle { canRetryBurn = note.canRetryBurn() } else { canRetryBurn = false }
        let canPause: Bool
        if case .transfer = job.handle { canPause = value.canPause } else { canPause = false }
        let currentPhase: TransferPhase
        switch value.state {
        case .complete: currentPhase = isFileDownload && export != .exported ? .verifiedAwaitingExport : .complete
        case .failed: currentPhase = .failed
        case .paused: currentPhase = .paused
        case .pausing: currentPhase = .pausing
        case .running:
            currentPhase = prompt != nil ? .awaitingInput : value.phase == "waiting" ? (job.direction == .upload ? .waitingForRecipient : .waitingForSender) : phase(value.phase)
        }
        return DTransferSnapshot(
            id: id, transferID: job.transferID, checkpointID: value.checkpointId,
            lifecycle: value.state == .running ? .running : value.state == .pausing ? .pausing : value.state == .paused ? .paused : value.state == .complete ? .complete : .failed,
            phase: currentPhase, direction: job.direction, kind: job.kind, transport: job.transport,
            completedBytes: value.done, totalBytes: value.total, prompt: prompt,
            verifiedFilePaths: isFileDownload && value.state == .complete ? value.results.filter { $0.hasPrefix("/") } : [],
            verifiedNote: job.verifiedNote, shareURL: presentedShareURL(value.shareUrl, job: job),
            serverAvailability: value.shareUrl == nil ? .unknown : job.transport == .webRTC ? .waitingForRecipient : .available,
            exportStatus: export, error: value.error,
            actions: TransferActions(pause: canPause, resume: value.state == .paused && value.checkpointId != nil, retryNoteBurn: canRetryBurn, discardLocal: value.checkpointId != nil, revokeRemote: value.canRevokeRemote, endLive: value.canEndLive)
        )
    }

    private func backgroundOnlySnapshot(jobID: TransferJobID, job: Job) throws -> DTransferSnapshot {
        guard let checkpointID = backgroundAssociations.values.first(where: { $0.jobID == jobID })?.checkpointID else { throw FilebeamDomainError.invalidInput("Background checkpoint is unavailable.") }
        let status = try client.backgroundTransfer().status(transferId: checkpointID)
        let stage = job.backgroundStage
        if case .selecting? = stage, let promptID = job.selectionPromptID {
            return DTransferSnapshot(id: jobID, transferID: job.transferID, checkpointID: checkpointID, lifecycle: .running, phase: .awaitingInput, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: status.done, totalBytes: status.total, prompt: TransferPrompt(id: promptID, kind: .filesSelection, message: "Choose the files to download.", selectionItems: job.selectionItems), actions: TransferActions(pause: true))
        }
        let failure: String? = { if case let .failed(message)? = stage { return message }; return nil }()
        let paused: Bool = { if case .paused? = stage { return true }; return false }()
        return DTransferSnapshot(id: jobID, transferID: job.transferID, checkpointID: checkpointID, lifecycle: failure == nil ? (paused ? .paused : .running) : .failed, phase: failure == nil ? (paused ? .paused : phase(status.state)) : .failed, direction: job.direction, kind: job.kind, transport: job.transport, completedBytes: status.done, totalBytes: status.total, serverAvailability: .unknown, error: failure, actions: TransferActions(pause: failure == nil && !paused, resume: paused || failure != nil))
    }
}
