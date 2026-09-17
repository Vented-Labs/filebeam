import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

final class BackgroundJournal {
    static let maximumEntries = 256
    static let maximumScheduledPerCheckpoint = 64
    static let maximumCiphertextBytes: UInt64 = 25 * 1024 * 1024

    enum State: String, Codable, Sendable { case scheduled, staged, failed, paused }
    struct Entry: Codable, Equatable, Sendable {
        var descriptor: BackgroundTaskDescriptor
        var taskIdentifier: Int?
        var expectedResponseBytes: UInt64
        var spoolName: String?
        var responseStatus: Int?
        var responseHeaders: [String: String]
        var state: State
        var networkDone: UInt64
    }
    struct TaskIdentity: Sendable { let taskIdentifier: Int; let descriptor: BackgroundTaskDescriptor? }
    private struct Disk: Codable { var entries: [Entry] }
    private let file: URL
    private let spool: URL
    private var entries: [Entry]

    init(directory: URL) throws {
        let manager = FileManager.default
        try manager.createDirectory(at: directory, withIntermediateDirectories: true)
        file = directory.appendingPathComponent("background-http-v1.json")
        spool = directory.appendingPathComponent("background-ciphertext", isDirectory: true)
        try manager.createDirectory(at: spool, withIntermediateDirectories: true)
        let decoded = manager.fileExists(atPath: file.path) ? try JSONDecoder().decode(Disk.self, from: Data(contentsOf: file)).entries : []
        guard Self.valid(decoded) else { throw BackgroundJournalError.invalidState }
        entries = decoded
        try protect(spool)
        if manager.fileExists(atPath: file.path) { try protect(file) }
        cleanupOrphans()
    }

    func entry(forTaskIdentifier id: Int) -> Entry? { entries.first { $0.taskIdentifier == id && $0.state == .scheduled } }
    func entry(operationID: String) -> Entry? { entries.first { $0.descriptor.operationID == operationID } }
    func staged() -> [Entry] { entries.filter { $0.state == .staged } }
    func scheduled(checkpointID: String) -> [Entry] { entries.filter { $0.descriptor.checkpointID == checkpointID && $0.state == .scheduled } }
    func wireEntries(checkpointID: String) -> [Entry] { entries.filter { $0.descriptor.checkpointID == checkpointID && ($0.state == .scheduled || $0.state == .staged) } }
    func entries(checkpointID: String, direction: BackgroundDirection) -> [Entry] { entries.filter { $0.descriptor.checkpointID == checkpointID && $0.descriptor.direction == direction } }
    func scopes() -> Set<BackgroundScope> { Set(entries.map { .init(checkpointID: $0.descriptor.checkpointID, direction: $0.descriptor.direction) }) }

    func queuePlan(workOperationIDs: [String], checkpointID: String, direction: BackgroundDirection) -> [String] {
        BackgroundQueuePlan.operationIDs(workOperationIDs: workOperationIDs, entries: entries, checkpointID: checkpointID, direction: direction)
    }

    func addBatch(_ additions: [Entry]) throws {
        guard additions.count + entries.count <= Self.maximumEntries, Self.valid(additions), Set(additions.map(\.descriptor.operationID)).isDisjoint(with: Set(entries.map(\.descriptor.operationID))) else { throw BackgroundJournalError.invalidState }
        try replace(entries + additions)
    }

    func scheduleBatch(_ additions: [Entry]) throws {
        let operationIDs = Set(additions.map(\.descriptor.operationID))
        let replaced = entries.filter { operationIDs.contains($0.descriptor.operationID) && ($0.state == .failed || $0.state == .paused) }
        let retained = entries.filter { !operationIDs.contains($0.descriptor.operationID) }
        guard additions.count + retained.count <= Self.maximumEntries, Self.valid(additions) else { throw BackgroundJournalError.invalidState }
        try replace(retained + additions)
        for entry in replaced { if let name = entry.spoolName, let url = try? spoolURL(name) { try? FileManager.default.removeItem(at: url) } }
    }

    func bind(_ entry: Entry) throws {
        if let index = entries.firstIndex(where: { $0.descriptor.operationID == entry.descriptor.operationID }) {
            var candidate = entries; candidate[index] = entry; try replace(candidate)
        } else { try addBatch([entry]) }
    }

    func reconcile(_ tasks: [TaskIdentity]) throws {
        var candidate = entries
        for index in candidate.indices where candidate[index].state == .scheduled {
            candidate[index].taskIdentifier = tasks.first(where: { $0.descriptor == candidate[index].descriptor })?.taskIdentifier
        }
        try replace(candidate)
    }

    /// A successful pending-work read is the durable acknowledgement authority.
    @discardableResult
    func reconcilePending(checkpointID: String, direction: BackgroundDirection, pendingOperationIDs: Set<String>?) throws -> [Entry] {
        guard let pendingOperationIDs else { return [] }
        let removed = entries.filter {
            $0.descriptor.checkpointID == checkpointID &&
            $0.descriptor.direction == direction &&
            !pendingOperationIDs.contains($0.descriptor.operationID)
        }
        guard !removed.isEmpty else { return [] }
        try replace(entries.filter { entry in !removed.contains(entry) })
        for entry in removed { removeSpool(named: entry.spoolName) }
        return removed
    }

    func markPaused(checkpointID: String) throws -> BackgroundDirection? {
        guard let first = entries.first(where: { $0.descriptor.checkpointID == checkpointID }) else { return nil }
        var candidate = entries
        for index in candidate.indices where candidate[index].descriptor.checkpointID == checkpointID && candidate[index].state == .scheduled { candidate[index].state = .paused; candidate[index].taskIdentifier = nil }
        try replace(candidate)
        return first.descriptor.direction
    }

    func markFailed(taskIdentifier: Int, message: String) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.taskIdentifier == taskIdentifier && $0.state == .scheduled }) else { return nil }
        let spoolName = entries[index].spoolName
        var candidate = entries; candidate[index].state = .failed; candidate[index].taskIdentifier = nil; candidate[index].spoolName = nil
        // The error text is deliberately not persisted: URL errors may echo an untrusted URL.
        _ = message
        try replace(candidate)
        removeSpool(named: spoolName)
        return candidate[index]
    }

    func markIngestionFailed(operationID: String) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.descriptor.operationID == operationID && $0.state == .staged }) else { return nil }
        let spoolName = entries[index].spoolName
        var candidate = entries; candidate[index].state = .failed; candidate[index].taskIdentifier = nil; candidate[index].spoolName = nil
        try replace(candidate)
        removeSpool(named: spoolName)
        return candidate[index]
    }

    func recordResponse(_ response: HTTPURLResponse?, taskIdentifier: Int) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.taskIdentifier == taskIdentifier && $0.state == .scheduled }) else { return nil }
        var candidate = entries; candidate[index].responseStatus = response?.statusCode; candidate[index].responseHeaders = allowedHeaders(response)
        try replace(candidate); return candidate[index]
    }

    func appendUploadResponse(_ data: Data, taskIdentifier: Int, response: HTTPURLResponse?, maximumBytes: Int) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.taskIdentifier == taskIdentifier && $0.state == .scheduled }) else { return nil }
        let name = entries[index].spoolName ?? Self.newSpoolName()
        let url = try spoolURL(name)
        let old = (try? Data(contentsOf: url)) ?? Data()
        guard old.count + data.count <= maximumBytes else { throw BackgroundJournalError.limitExceeded }
        var merged = old; merged.append(data)
        try merged.write(to: url, options: .atomic); try protect(url)
        var candidate = entries; candidate[index].spoolName = name; candidate[index].responseStatus = response?.statusCode; candidate[index].responseHeaders = allowedHeaders(response)
        do { try replace(candidate) } catch { throw error }
        return candidate[index]
    }

    func stageDownload(from source: URL, taskIdentifier: Int, response: HTTPURLResponse?) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.taskIdentifier == taskIdentifier && $0.state == .scheduled }) else { return nil }
        let size = try source.resourceValues(forKeys: [.fileSizeKey]).fileSize.map(UInt64.init) ?? 0
        guard size <= entries[index].expectedResponseBytes, size <= Self.maximumCiphertextBytes else { throw BackgroundJournalError.limitExceeded }
        let name = Self.newSpoolName(); let destination = try spoolURL(name)
        try FileManager.default.moveItem(at: source, to: destination); try protect(destination)
        var candidate = entries; candidate[index].spoolName = name; candidate[index].responseStatus = response?.statusCode; candidate[index].responseHeaders = allowedHeaders(response); candidate[index].state = .staged; candidate[index].taskIdentifier = nil; candidate[index].networkDone = size
        try replace(candidate)
        return candidate[index]
    }

    func stageUpload(taskIdentifier: Int) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.taskIdentifier == taskIdentifier && $0.state == .scheduled }) else { return nil }
        var candidate = entries; candidate[index].state = .staged; candidate[index].taskIdentifier = nil
        try replace(candidate); return candidate[index]
    }

    func updateNetworkBytes(taskIdentifier: Int, done: UInt64) throws -> Entry? {
        guard let index = entries.firstIndex(where: { $0.taskIdentifier == taskIdentifier && $0.state == .scheduled }) else { return nil }
        var candidate = entries; candidate[index].networkDone = max(candidate[index].networkDone, done); try replace(candidate); return candidate[index]
    }

    func remove(operationID: String) throws {
        guard let removed = entries.first(where: { $0.descriptor.operationID == operationID }) else { return }
        try replace(entries.filter { $0.descriptor.operationID != operationID })
        removeSpool(named: removed.spoolName)
    }

    private func replace(_ candidate: [Entry]) throws {
        guard candidate.count <= Self.maximumEntries, Self.valid(candidate) else { throw BackgroundJournalError.invalidState }
        let data = try JSONEncoder().encode(Disk(entries: candidate))
        try data.write(to: file, options: .atomic); try protect(file)
        entries = candidate
    }

    private func cleanupOrphans() {
        let referenced = Set(entries.compactMap(\.spoolName))
        guard let files = try? FileManager.default.contentsOfDirectory(at: spool, includingPropertiesForKeys: nil) else { return }
        for url in files where Self.validSpoolName(url.lastPathComponent) && !referenced.contains(url.lastPathComponent) { try? FileManager.default.removeItem(at: url) }
    }
    private func removeSpool(named name: String?) { if let name, let url = try? spoolURL(name) { try? FileManager.default.removeItem(at: url) } }

    private static func valid(_ entries: [Entry]) -> Bool {
        guard entries.count <= maximumEntries, Set(entries.map(\.descriptor.operationID)).count == entries.count else { return false }
        return entries.allSatisfy { entry in
            entry.expectedResponseBytes <= maximumCiphertextBytes && entry.responseHeaders.count <= 2 && entry.responseHeaders.allSatisfy { ($0.key.caseInsensitiveCompare("Content-Length") == .orderedSame || $0.key.caseInsensitiveCompare("ETag") == .orderedSame) && $0.value.utf8.count <= 8 * 1024 } && (entry.spoolName == nil || validSpoolName(entry.spoolName!))
        }
    }
    private static func newSpoolName() -> String { UUID().uuidString.lowercased() + ".ciphertext" }
    private static func validSpoolName(_ name: String) -> Bool { name.hasSuffix(".ciphertext") && UUID(uuidString: String(name.dropLast(".ciphertext".count))) != nil && !name.contains("/") && !name.contains("\\") }
    private func spoolURL(_ name: String) throws -> URL { guard Self.validSpoolName(name) else { throw BackgroundJournalError.invalidState }; return spool.appendingPathComponent(name, isDirectory: false) }
    private func allowedHeaders(_ response: HTTPURLResponse?) -> [String: String] { guard let response else { return [:] }; var result: [String: String] = [:]; for (key, value) in response.allHeaderFields { let name = String(describing: key); let text = String(describing: value); if (name.caseInsensitiveCompare("Content-Length") == .orderedSame || name.caseInsensitiveCompare("ETag") == .orderedSame) && text.utf8.count <= 8 * 1024 { result[name] = text } }; return result }
    private func protect(_ url: URL) throws {
        #if os(iOS) || os(tvOS) || os(watchOS)
        var values = URLResourceValues()
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: url.path)
        values.isExcludedFromBackup = true
        var mutable = url
        try mutable.setResourceValues(values)
        #endif
    }
}

struct BackgroundScope: Hashable, Sendable {
    let checkpointID: String
    let direction: BackgroundDirection
}

/// Selects missing/retry work first, then fills a bounded URLSession window.
struct BackgroundQueuePlan {
    static func operationIDs(workOperationIDs: [String], entries: [BackgroundJournal.Entry], checkpointID: String, direction: BackgroundDirection) -> [String] {
        let relevant = entries.filter { $0.descriptor.checkpointID == checkpointID && $0.descriptor.direction == direction }
        let byOperation = Dictionary(uniqueKeysWithValues: relevant.map { ($0.descriptor.operationID, $0) })
        let running = relevant.filter { $0.state == .scheduled && $0.taskIdentifier != nil }.count
        var remainingJournalSlots = BackgroundJournal.maximumEntries - entries.count
        var remainingWindowSlots = BackgroundJournal.maximumScheduledPerCheckpoint - running
        var result: [String] = []
        var selected: Set<String> = []
        for operationID in workOperationIDs where remainingWindowSlots > 0 {
            guard selected.insert(operationID).inserted else { continue }
            if let entry = byOperation[operationID] {
                guard entry.state != .staged, !(entry.state == .scheduled && entry.taskIdentifier != nil) else { continue }
            } else {
                guard remainingJournalSlots > 0 else { break }
                remainingJournalSlots -= 1
            }
            result.append(operationID)
            remainingWindowSlots -= 1
        }
        return result
    }
}
