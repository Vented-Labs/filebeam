import Foundation
#if canImport(FilebeamCore) && canImport(UIKit)
@preconcurrency import FilebeamCore
import UIKit

enum BackgroundHTTPDriverError: LocalizedError { case unavailable, invalidDescriptor, inboxCredentialsRequired
    var errorDescription: String? { switch self { case .unavailable: return "Background state is unavailable until the device is unlocked."; case .invalidDescriptor: return "The native background request descriptor is invalid."; case .inboxCredentialsRequired: return "Sign in again before resuming this inbox download." } }
}

public final class BackgroundHTTPDriver: NSObject, @unchecked Sendable {
    public static let maximumUploadResponseBytes = 256 * 1024
    public static let stableSessionIdentifier = "io.filebeam.ios.background-http-v1"
    private let engine: FilebeamCore.BackgroundTransfer
    private let directory: URL
    private let onEvent: @Sendable (BackgroundHTTPEvent) -> Void
    private let stateQueue = DispatchQueue(label: "io.filebeam.background.state")
    private let ffiQueue = DispatchQueue(label: "io.filebeam.background.ffi")
    private let delegateQueue: OperationQueue
    private var journal: BackgroundJournal?
    private var session: URLSession!
    private var inboxCookies: [String: String] = [:]
    private var pendingIngestions = 0
    private var ingestingOperationIDs: Set<String> = []
    private var eventsFinished = false
    private var enqueuing: Set<String> = []

    public init(engine: FilebeamCore.BackgroundTransfer, stateDirectory: URL, sessionIdentifier: String = BackgroundHTTPDriver.stableSessionIdentifier, onEvent: @escaping @Sendable (BackgroundHTTPEvent) -> Void) {
        self.engine = engine; directory = stateDirectory.standardizedFileURL; self.onEvent = onEvent
        delegateQueue = OperationQueue(); delegateQueue.maxConcurrentOperationCount = 1; delegateQueue.name = "io.filebeam.background.delegate"
        journal = try? BackgroundJournal(directory: directory)
        super.init()
        let configuration = URLSessionConfiguration.background(withIdentifier: sessionIdentifier)
        configuration.isDiscretionary = false; configuration.waitsForConnectivity = true; configuration.sessionSendsLaunchEvents = true
        configuration.httpCookieStorage = nil; configuration.httpShouldSetCookies = false; configuration.urlCache = nil; configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        session = URLSession(configuration: configuration, delegate: self, delegateQueue: delegateQueue)
        BackgroundURLSessionRegistry.attach(self, sessionIdentifier: sessionIdentifier)
    }

    public static func attach(_ driver: BackgroundHTTPDriver, sessionIdentifier: String = stableSessionIdentifier) { BackgroundURLSessionRegistry.attach(driver, sessionIdentifier: sessionIdentifier) }
    public func supplyInboxCookieContext(_ cookie: String, checkpointID: String) throws { guard !cookie.isEmpty, !cookie.contains("\r"), !cookie.contains("\n") else { throw BackgroundHTTPDriverError.inboxCredentialsRequired }; stateQueue.sync { inboxCookies[checkpointID] = cookie } }

    public func enqueue(checkpointID: String, direction: BackgroundDirection) async throws {
        let enqueueKey = "\(direction.rawValue):\(checkpointID)"
        guard stateQueue.sync(execute: { enqueuing.insert(enqueueKey).inserted }) else { return }
        defer { _ = stateQueue.sync { enqueuing.remove(enqueueKey) } }
        try await reconcileOrThrow(replayStaged: false)
        guard let journal = stateQueue.sync(execute: { self.journal }) else { throw BackgroundHTTPDriverError.unavailable }
        let work = try await descriptors(checkpointID: checkpointID, direction: direction)
        try await schedule(checkpointID: checkpointID, direction: direction, work: work, journal: journal)
    }

    private func schedule(checkpointID: String, direction: BackgroundDirection, work: [FilebeamCore.BackgroundWork], journal: BackgroundJournal, includePaused: Bool = true) async throws {
        let existing = await session.allTasks
        var created: [(URLSessionTask, BackgroundJournal.Entry)] = []
        let paused = includePaused ? Set<String>() : Set(stateQueue.sync { journal.entries(checkpointID: checkpointID, direction: direction).filter { $0.state == .paused }.map { $0.descriptor.operationID } })
        let planned = Set(stateQueue.sync { journal.queuePlan(workOperationIDs: work.map(\.operationId).filter { !paused.contains($0) }, checkpointID: checkpointID, direction: direction) })
        for value in work where planned.contains(value.operationId) {
            try validate(value, direction: direction)
            let descriptor = try BackgroundTaskDescriptor(operationID: value.operationId, checkpointID: checkpointID, direction: direction)
            let task = try makeTask(value, descriptor: descriptor, direction: direction)
            created.append((task, .init(descriptor: descriptor, taskIdentifier: task.taskIdentifier, expectedResponseBytes: value.expectedResponseBytes, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)))
        }
        // Persist every suspended task in one transaction before any can complete.
        if !created.isEmpty {
            let descriptors = created.map { $0.1.descriptor }
            do {
                for task in existing where BackgroundTaskDescriptor.parse(task.taskDescription).map(descriptors.contains) == true { task.cancel() }
                try stateQueue.sync { try journal.scheduleBatch(created.map { $0.1 }) }
            } catch {
                created.forEach { $0.0.cancel() }
                throw error
            }
            created.forEach { $0.0.resume() }
        }
        if work.isEmpty && stateQueue.sync(execute: { journal.entries(checkpointID: checkpointID, direction: direction).isEmpty }) {
            onEvent(.readyToFinalize(checkpointID: checkpointID, direction: direction))
        }
    }

    public func pause(checkpointID: String) async throws {
        guard let journal = stateQueue.sync(execute: { openJournal() }) else { throw BackgroundHTTPDriverError.unavailable }
        let tasks = await session.allTasks
        let direction = try stateQueue.sync { try journal.markPaused(checkpointID: checkpointID) }
        for task in tasks where BackgroundTaskDescriptor.parse(task.taskDescription)?.checkpointID == checkpointID { task.cancel() }
        if let direction { onEvent(.paused(checkpointID: checkpointID, direction: direction)) }
    }
    public func resume(checkpointID: String, direction: BackgroundDirection) async throws { try await enqueue(checkpointID: checkpointID, direction: direction) }
    public func reconcile() async { try? await reconcileOrThrow() }

    private func reconcileOrThrow(replayStaged: Bool = true) async throws {
        guard let journal = stateQueue.sync(execute: { openJournal() }) else { throw BackgroundHTTPDriverError.unavailable }
        let tasks = await session.allTasks
        let parsed = tasks.map { ($0, BackgroundTaskDescriptor.parse($0.taskDescription)) }
        for (task, descriptor) in parsed where descriptor == nil { task.cancel() }
        let managed = parsed.compactMap { task, descriptor in descriptor.map { (task, $0) } }
        // Include failed, staged, and taskless records: Rust's complete pending set is the acknowledgement authority.
        let taskScopes = Set(managed.map { BackgroundScope(checkpointID: $0.1.checkpointID, direction: $0.1.direction) })
        let scopes = stateQueue.sync { journal.scopes().union(taskScopes) }
        var workByScope: [BackgroundScope: [FilebeamCore.BackgroundWork]] = [:]
        for scope in scopes {
            let work: [FilebeamCore.BackgroundWork]
            do { work = try await descriptors(checkpointID: scope.checkpointID, direction: scope.direction) }
            catch { continue } // Inbox credentials or protected keychain can be retried after unlock.
            workByScope[scope] = work
            let byOperation = Dictionary(uniqueKeysWithValues: work.map { ($0.operationId, $0) })
            for (task, descriptor) in managed where descriptor.direction == scope.direction && descriptor.checkpointID == scope.checkpointID {
                    guard let value = byOperation[descriptor.operationID] else { task.cancel(); continue }
                    if stateQueue.sync(execute: { journal.entry(operationID: descriptor.operationID) }) == nil {
                        try validate(value, direction: scope.direction)
                        try stateQueue.sync { try journal.bind(.init(descriptor: descriptor, taskIdentifier: task.taskIdentifier, expectedResponseBytes: value.expectedResponseBytes, spoolName: nil, responseStatus: nil, responseHeaders: [:], state: .scheduled, networkDone: 0)) }
                    }
                }
            _ = try stateQueue.sync { try journal.reconcilePending(checkpointID: scope.checkpointID, direction: scope.direction, pendingOperationIDs: Set(byOperation.keys)) }
        }
        try stateQueue.sync { try journal.reconcile(managed.map { .init(taskIdentifier: $0.0.taskIdentifier, descriptor: $0.1) }) }
        for (task, _) in managed where task.state == .suspended && stateQueue.sync(execute: { journal.entry(forTaskIdentifier: task.taskIdentifier) != nil }) { task.resume() }
        if replayStaged { for entry in stateQueue.sync(execute: { journal.staged() }) { scheduleIngest(entry) } }
        for (scope, work) in workByScope {
            try await schedule(checkpointID: scope.checkpointID, direction: scope.direction, work: work, journal: journal, includePaused: false)
        }
    }

    private func descriptors(checkpointID: String, direction: BackgroundDirection) async throws -> [FilebeamCore.BackgroundWork] { try await ffi { [self] in switch direction { case .upload: return try engine.pendingUploadWork(transferId: checkpointID); case .download: return try engine.pendingDownloadWork(transferId: checkpointID); case .inboxDownload: guard let cookie = stateQueue.sync(execute: { inboxCookies[checkpointID] }) else { throw BackgroundHTTPDriverError.inboxCredentialsRequired }; return try engine.pendingInboxDownloadWork(transferId: checkpointID, cookieContext: cookie) } } }
    private func makeTask(_ work: FilebeamCore.BackgroundWork, descriptor: BackgroundTaskDescriptor, direction: BackgroundDirection) throws -> URLSessionTask { guard let url = URL(string: work.url) else { throw BackgroundHTTPDriverError.invalidDescriptor }; var request = URLRequest(url: url); request.httpMethod = work.method; request.cachePolicy = .reloadIgnoringLocalCacheData; work.headers.forEach { request.setValue($0.value, forHTTPHeaderField: $0.name) }; let task: URLSessionTask; if direction == .upload { task = session.uploadTask(with: request, fromFile: URL(fileURLWithPath: work.bodyPath)) } else { task = session.downloadTask(with: request) }; task.taskDescription = descriptor.taskDescription; return task }
    private func validate(_ work: FilebeamCore.BackgroundWork, direction: BackgroundDirection) throws {
        guard let c = URLComponents(string: work.url), c.scheme?.lowercased() == "https", c.host != nil, c.user == nil, c.password == nil, c.fragment == nil, work.url.utf8.count <= 4096, work.expectedResponseBytes <= BackgroundJournal.maximumCiphertextBytes, work.headers.count <= 32, work.headers.allSatisfy({ $0.name.utf8.count <= 128 && $0.value.utf8.count <= 8192 && !$0.name.contains("\r") && !$0.name.contains("\n") && !$0.value.contains("\r") && !$0.value.contains("\n") }) else { throw BackgroundHTTPDriverError.invalidDescriptor }
        guard direction == .upload ? ["PUT", "POST"].contains(work.method) : work.method == "GET" else { throw BackgroundHTTPDriverError.invalidDescriptor }
        if direction == .upload { let body = URL(fileURLWithPath: work.bodyPath).standardizedFileURL; guard body.path.hasPrefix(directory.path + "/"), let values = try? body.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]), values.isRegularFile == true, values.isSymbolicLink != true else { throw BackgroundHTTPDriverError.invalidDescriptor } }
    }
    private func ffi<T: Sendable>(_ operation: @escaping @Sendable () throws -> T) async throws -> T { try await withCheckedThrowingContinuation { continuation in ffiQueue.async { continuation.resume(with: Result { try operation() }) } } }
    private func openJournal() -> BackgroundJournal? { if journal == nil { journal = try? BackgroundJournal(directory: directory) }; return journal }

    private func scheduleIngest(_ entry: BackgroundJournal.Entry) {
        let scheduled = stateQueue.sync { () -> Bool in
            guard ingestingOperationIDs.insert(entry.descriptor.operationID).inserted else { return false }
            pendingIngestions += 1
            return true
        }
        if scheduled { Task { [weak self] in await self?.ingest(entry) } }
    }
    private func ingest(_ entry: BackgroundJournal.Entry) async { defer { finishIngestion(operationID: entry.descriptor.operationID) }; guard let journal = stateQueue.sync(execute: { openJournal() }) else { return }; do { let status = UInt16(clamping: entry.responseStatus ?? 0); if entry.descriptor.direction == .upload { let path = entry.spoolName.map { directory.appendingPathComponent("background-ciphertext").appendingPathComponent($0).path }; try await ffi { [engine] in try engine.ingestUploadCompletion(transferId: entry.descriptor.checkpointID, operationId: entry.descriptor.operationID, status: status, responseFile: path) } } else { guard let name = entry.spoolName else { throw BackgroundHTTPDriverError.invalidDescriptor }; let path = directory.appendingPathComponent("background-ciphertext").appendingPathComponent(name).path; try await ffi { [engine] in try engine.ingestDownloadCompletion(transferId: entry.descriptor.checkpointID, operationId: entry.descriptor.operationID, status: status, responseHeaders: entry.responseHeaders.map { FilebeamCore.BackgroundHeader(name: $0.key, value: $0.value) }, responseFile: path) } }; try stateQueue.sync { try journal.remove(operationID: entry.descriptor.operationID) }; await reportProgress(checkpointID: entry.descriptor.checkpointID); try await enqueue(checkpointID: entry.descriptor.checkpointID, direction: entry.descriptor.direction) } catch { _ = try? stateQueue.sync { try journal.markIngestionFailed(operationID: entry.descriptor.operationID) }; onEvent(.failed(checkpointID: entry.descriptor.checkpointID, message: "Background operation requires recovery.")) } }
    private func finish(_ task: URLSessionTask, error: Error?) { guard let journal = stateQueue.sync(execute: { openJournal() }), let entry = stateQueue.sync(execute: { journal.entry(forTaskIdentifier: task.taskIdentifier) }) else { return }; do { let completedEntry = stateQueue.sync { journal.entry(forTaskIdentifier: task.taskIdentifier) } ?? entry; if error != nil || (completedEntry.descriptor.direction == .upload && completedEntry.responseStatus != 201) { let failed = try stateQueue.sync { try journal.markFailed(taskIdentifier: task.taskIdentifier, message: "network") }; if let failed { onEvent(.failed(checkpointID: failed.descriptor.checkpointID, message: "Background network operation requires retry.")) }; return }; guard let staged = try stateQueue.sync(execute: { try journal.stageUpload(taskIdentifier: task.taskIdentifier) }) else { return }; scheduleIngest(staged) } catch { onEvent(.failed(checkpointID: entry.descriptor.checkpointID, message: "Background state requires recovery.")) } }
    private func finishIngestion(operationID: String) { let complete = stateQueue.sync { ingestingOperationIDs.remove(operationID); pendingIngestions -= 1; if pendingIngestions == 0 && eventsFinished { eventsFinished = false; return true }; return false }; if complete { BackgroundURLSessionRegistry.finishEvents(sessionIdentifier: session.configuration.identifier) } }
    private func didFinishEvents() { let complete = stateQueue.sync { eventsFinished = true; if pendingIngestions == 0 { eventsFinished = false; return true }; return false }; if complete { BackgroundURLSessionRegistry.finishEvents(sessionIdentifier: session.configuration.identifier) } }
}

extension BackgroundHTTPDriver: URLSessionDataDelegate, URLSessionDownloadDelegate {
    public func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void) { do { guard let journal = stateQueue.sync(execute: { openJournal() }) else { throw BackgroundHTTPDriverError.unavailable }; _ = try stateQueue.sync { try journal.recordResponse(response as? HTTPURLResponse, taskIdentifier: dataTask.taskIdentifier) }; completionHandler(.allow) } catch { dataTask.cancel(); completionHandler(.cancel); onEvent(.failed(checkpointID: "unknown", message: "Background state requires recovery.")) } }
    public func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) { do { guard let journal = stateQueue.sync(execute: { openJournal() }) else { throw BackgroundHTTPDriverError.unavailable }; _ = try stateQueue.sync { try journal.appendUploadResponse(data, taskIdentifier: dataTask.taskIdentifier, response: dataTask.response as? HTTPURLResponse, maximumBytes: Self.maximumUploadResponseBytes) } } catch { dataTask.cancel() } }
    public func urlSession(_ session: URLSession, task: URLSessionTask, didSendBodyData bytesSent: Int64, totalBytesSent: Int64, totalBytesExpectedToSend: Int64) { progress(task: task, done: totalBytesSent, total: totalBytesExpectedToSend) }
    public func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) { progress(task: downloadTask, done: totalBytesWritten, total: totalBytesExpectedToWrite); if let entry = stateQueue.sync(execute: { openJournal()?.entry(forTaskIdentifier: downloadTask.taskIdentifier) }), UInt64(max(0, totalBytesWritten)) > entry.expectedResponseBytes { downloadTask.cancel() } }
    public func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) { do { guard let journal = stateQueue.sync(execute: { openJournal() }), let entry = try stateQueue.sync(execute: { try journal.stageDownload(from: location, taskIdentifier: downloadTask.taskIdentifier, response: downloadTask.response as? HTTPURLResponse) }) else { return }; scheduleIngest(entry) } catch { downloadTask.cancel() } }
    public func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) { finish(task, error: error) }
    public func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) { didFinishEvents() }
    private func progress(task: URLSessionTask, done: Int64, total: Int64) {
        guard let journal = stateQueue.sync(execute: { openJournal() }) else { return }
        let entry: BackgroundJournal.Entry?
        do { entry = try stateQueue.sync { try journal.updateNetworkBytes(taskIdentifier: task.taskIdentifier, done: UInt64(max(0, done))) } }
        catch { return }
        guard let entry else { return }
        let checkpoint = entry.descriptor.checkpointID
        _ = total
        Task { [weak self] in await self?.reportProgress(checkpointID: checkpoint) }
    }
    private func reportProgress(checkpointID: String) async { guard let status = try? await ffi({ [engine] in try engine.status(transferId: checkpointID) }) else { return }; onEvent(.progress(checkpointID: checkpointID, done: status.done, total: status.total)) }
}

enum BackgroundURLSessionRegistry { private static let lock = NSLock(); nonisolated(unsafe) private static var drivers: [String: BackgroundHTTPDriver] = [:]; nonisolated(unsafe) private static var completions: [String: @Sendable () -> Void] = [:]; nonisolated(unsafe) private static var finished: Set<String> = []; static func attach(_ driver: BackgroundHTTPDriver, sessionIdentifier: String) { lock.lock(); drivers[sessionIdentifier] = driver; let completion = finished.remove(sessionIdentifier).flatMap { _ in completions.removeValue(forKey: sessionIdentifier) }; lock.unlock(); if let completion { DispatchQueue.main.async(execute: completion) } }; static func handleEvents(sessionIdentifier: String, completion: @escaping @Sendable () -> Void) { lock.lock(); let ready = finished.remove(sessionIdentifier) != nil; if !ready { completions[sessionIdentifier] = completion }; lock.unlock(); if ready { DispatchQueue.main.async(execute: completion) } }; static func finishEvents(sessionIdentifier: String?) { guard let id = sessionIdentifier else { return }; lock.lock(); let completion = completions.removeValue(forKey: id); if completion == nil { finished.insert(id) }; lock.unlock(); if let completion { DispatchQueue.main.async(execute: completion) } } }
@MainActor final class FilebeamAppDelegate: NSObject, UIApplicationDelegate { func application(_ application: UIApplication, handleEventsForBackgroundURLSession identifier: String, completionHandler: @escaping @Sendable () -> Void) { BackgroundURLSessionRegistry.handleEvents(sessionIdentifier: identifier, completion: completionHandler) } }
#endif
