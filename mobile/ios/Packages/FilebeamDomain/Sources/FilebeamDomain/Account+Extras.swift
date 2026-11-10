import Foundation

// MARK: Transfer history

public struct HistoryFilter: Hashable, Sendable {
    public var status: String?
    public var kind: String?
    public var driver: String?
    public init(status: String? = nil, kind: String? = nil, driver: String? = nil) { self.status = status; self.kind = kind; self.driver = driver }
}

/// One sent transfer in the account's history. Names and keys are never part of it.
public struct HistoryEntry: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let kind: String
    public let driver: String
    public let delivery: String
    public let status: String
    public let itemCount: UInt64
    public let ciphertextBytes: UInt64
    public let createdAt: Date?
    public let expiresAt: Date?
    public let removedAt: Date?
    public let retentionHours: UInt64
    public let maximumRetentionHours: UInt64
    public let burnOnRead: Bool
    public let canDelete: Bool
    public let canExtend: Bool
    public init(id: String, kind: String, driver: String, delivery: String, status: String, itemCount: UInt64, ciphertextBytes: UInt64, createdAt: Date?, expiresAt: Date?, removedAt: Date?, retentionHours: UInt64, maximumRetentionHours: UInt64, burnOnRead: Bool, canDelete: Bool, canExtend: Bool) {
        self.id = id; self.kind = kind; self.driver = driver; self.delivery = delivery; self.status = status; self.itemCount = itemCount; self.ciphertextBytes = ciphertextBytes; self.createdAt = createdAt; self.expiresAt = expiresAt; self.removedAt = removedAt; self.retentionHours = retentionHours; self.maximumRetentionHours = maximumRetentionHours; self.burnOnRead = burnOnRead; self.canDelete = canDelete; self.canExtend = canExtend
    }
    public var isNote: Bool { kind.hasPrefix("note") }
}

public struct HistoryPage: Hashable, Sendable {
    public let entries: [HistoryEntry]
    public let nextCursor: String?
    public init(entries: [HistoryEntry], nextCursor: String?) { self.entries = entries; self.nextCursor = nextCursor }
}

// MARK: Devices

public struct RegisteredDevice: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let displayName: String
    public let platform: String
    public let publicKey: String
    public let keyVersion: UInt32
    public let status: String
    public let lastSeenAt: Date?
    public let presenceExpiresAt: Date?
    public let revokedAt: Date?
    public init(id: String, displayName: String, platform: String, publicKey: String, keyVersion: UInt32, status: String, lastSeenAt: Date?, presenceExpiresAt: Date?, revokedAt: Date?) {
        self.id = id; self.displayName = displayName; self.platform = platform; self.publicKey = publicKey; self.keyVersion = keyVersion; self.status = status; self.lastSeenAt = lastSeenAt; self.presenceExpiresAt = presenceExpiresAt; self.revokedAt = revokedAt
    }
    public var isRevoked: Bool { status == "revoked" }
    public var isOnline: Bool { presenceExpiresAt.map { $0 > Date() } ?? false }
}

public struct DeviceRegistry: Codable, Hashable, Sendable {
    public let revision: UInt64
    public let devices: [RegisteredDevice]
    public init(revision: UInt64, devices: [RegisteredDevice]) { self.revision = revision; self.devices = devices }
}

// MARK: Changelog

/// A sanitised document node from the instance's release notes: text, or an element with a tag, attributes and children.
public indirect enum ReleaseNode: Hashable, Sendable {
    case text(String)
    case element(tag: String, attributes: [String: String], children: [ReleaseNode])

    public var plainText: String {
        switch self {
        case let .text(value): value
        case let .element(tag, _, children): tag == "br" ? "\n" : children.map(\.plainText).joined()
        }
    }
}

public struct ReleaseSummary: Hashable, Sendable, Identifiable {
    public let tag: String
    public let title: String
    public let description: String
    public let publishedAt: Date?
    public let prerelease: Bool
    public let latest: Bool
    public let featured: Bool
    public var id: String { tag }
    public init(tag: String, title: String, description: String, publishedAt: Date?, prerelease: Bool, latest: Bool, featured: Bool) {
        self.tag = tag; self.title = title; self.description = description; self.publishedAt = publishedAt; self.prerelease = prerelease; self.latest = latest; self.featured = featured
    }
}

public struct ChangelogListing: Hashable, Sendable {
    public let releases: [ReleaseSummary]
    public let page: Int
    public let lastPage: Int
    public let versions: [ReleaseSummary]
    public let hasPrereleases: Bool
    /// "ready", "unavailable" or another archive state from the instance.
    public let archiveState: String
    public let stale: Bool
    public init(releases: [ReleaseSummary], page: Int, lastPage: Int, versions: [ReleaseSummary], hasPrereleases: Bool, archiveState: String, stale: Bool) {
        self.releases = releases; self.page = page; self.lastPage = lastPage; self.versions = versions; self.hasPrereleases = hasPrereleases; self.archiveState = archiveState; self.stale = stale
    }
}

public struct ReleaseDetail: Hashable, Sendable {
    public let summary: ReleaseSummary
    public let author: String?
    public let htmlURL: String?
    /// Top-level content blocks, each a list of nodes.
    public let blocks: [[ReleaseNode]]
    public let newer: ReleaseSummary?
    public let older: ReleaseSummary?
    public init(summary: ReleaseSummary, author: String?, htmlURL: String?, blocks: [[ReleaseNode]], newer: ReleaseSummary?, older: ReleaseSummary?) {
        self.summary = summary; self.author = author; self.htmlURL = htmlURL; self.blocks = blocks; self.newer = newer; self.older = older
    }
}
