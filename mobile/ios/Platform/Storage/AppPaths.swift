import Foundation

struct AppPaths: Sendable {
    let transfers: URL
    let drafts: URL
    let sources: URL
    let group: URL?

    init() throws {
        let manager = FileManager.default
        let support = try manager.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true).appendingPathComponent("Filebeam", isDirectory: true)
        transfers = support.appendingPathComponent("transfers", isDirectory: true)
        drafts = support.appendingPathComponent("drafts", isDirectory: true)
        sources = support.appendingPathComponent("sources", isDirectory: true)
        let identifier = Bundle.main.object(forInfoDictionaryKey: "AppGroupIdentifier") as? String
        group = identifier.flatMap(manager.containerURL(forSecurityApplicationGroupIdentifier:))
        try [support, transfers, drafts, sources].forEach(Self.prepare)
        if let group { try Self.prepare(group) }
    }

    private static func prepare(_ url: URL) throws {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: url.path)
        var values = URLResourceValues(); values.isExcludedFromBackup = true
        var mutable = url; try mutable.setResourceValues(values)
    }
}
