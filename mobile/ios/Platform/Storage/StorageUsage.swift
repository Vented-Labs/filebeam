import Foundation

struct StorageUsageBucket: Hashable, Sendable {
    var files = 0
    var bytes: UInt64 = 0
    mutating func add(_ size: UInt64) { files += 1; bytes += size }
}

/// Metadata-only accounting for app-private transfer files. File contents and secrets are never opened.
struct StorageUsage: Hashable, Sendable {
    var snapshots = StorageUsageBucket()
    var recovery = StorageUsageBucket()
    var verified = StorageUsageBucket()
    var free: UInt64 = 0
    var bytes: UInt64 { snapshots.bytes + recovery.bytes + verified.bytes }

    static func scan(paths: AppPaths) async -> StorageUsage {
        await Task.detached(priority: .utility) {
            var usage = StorageUsage()
            let outputs = paths.sources.appendingPathComponent("outputs", isDirectory: true).standardizedFileURL.path
            for (bucket, file) in files(under: paths.sources) {
                if file.standardizedFileURL.path.hasPrefix(outputs + "/") { usage.verified.add(bucket) } else { usage.snapshots.add(bucket) }
            }
            for (size, _) in files(under: paths.transfers) { usage.recovery.add(size) }
            for (size, _) in files(under: paths.drafts) { usage.recovery.add(size) }
            if let values = try? paths.transfers.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]), let free = values.volumeAvailableCapacityForImportantUsage {
                usage.free = UInt64(max(0, free))
            }
            return usage
        }.value
    }

    private static func files(under root: URL) -> [(UInt64, URL)] {
        let keys: Set<URLResourceKey> = [.isRegularFileKey, .fileSizeKey, .isSymbolicLinkKey]
        guard let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: Array(keys), options: [.skipsPackageDescendants]) else { return [] }
        var result: [(UInt64, URL)] = []
        for case let url as URL in enumerator {
            guard let values = try? url.resourceValues(forKeys: keys), values.isSymbolicLink != true, values.isRegularFile == true else { continue }
            result.append((UInt64(max(0, values.fileSize ?? 0)), url))
        }
        return result
    }
}
