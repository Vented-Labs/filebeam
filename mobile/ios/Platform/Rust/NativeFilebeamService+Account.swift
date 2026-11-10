import Foundation
import FilebeamCore
import FilebeamDomain

/// Account history, release notes and devices. JSON documents from the core are mapped to
/// domain models here so screens never see wire shapes.
extension NativeFilebeamService {
    public func accountHistory(instance: FilebeamInstance, filter: FilebeamDomain.HistoryFilter, cursor: String?, limit: UInt32) async throws -> FilebeamDomain.HistoryPage {
        try await ffi {
            let page = try self.services(instance).accountHistory(filter: FilebeamCore.HistoryFilter(status: filter.status, kind: filter.kind, driver: filter.driver), cursor: cursor, limit: limit)
            return FilebeamDomain.HistoryPage(entries: page.entries.map { entry in
                FilebeamDomain.HistoryEntry(id: entry.id, kind: entry.kind, driver: entry.driver, delivery: entry.delivery, status: entry.status, itemCount: entry.itemCount, ciphertextBytes: entry.ciphertextBytes, createdAt: Self.instant(entry.createdAt), expiresAt: Self.instant(entry.expiresAt), removedAt: Self.instant(entry.removedAt), retentionHours: entry.retentionHours, maximumRetentionHours: entry.maximumRetentionHours, burnOnRead: entry.burnOnRead, canDelete: entry.canDelete, canExtend: entry.canExtend)
            }, nextCursor: page.nextCursor)
        }
    }

    public func deleteHistory(instance: FilebeamInstance, transferID: TransferID) async throws {
        try await ffi { try self.services(instance).accountHistoryDelete(transferId: transferID.rawValue) }
    }

    public func extendHistory(instance: FilebeamInstance, transferID: TransferID, retentionHours: UInt64) async throws -> Date? {
        try await ffi { Self.instant(try self.services(instance).accountHistoryExtend(transferId: transferID.rawValue, retentionHours: retentionHours)) }
    }

    public func devices(instance: FilebeamInstance) async throws -> FilebeamDomain.DeviceRegistry {
        try await ffi { Self.registry(try self.services(instance).accountDevices()) }
    }

    public func renameDevice(instance: FilebeamInstance, deviceID: String, name: String) async throws {
        try await ffi { _ = try self.services(instance).accountDeviceRename(deviceId: deviceID, displayName: name) }
    }

    public func revokeDevice(instance: FilebeamInstance, deviceID: String, currentPassword: String) async throws {
        try await ffi { _ = try self.services(instance).accountDeviceRevoke(deviceId: deviceID, currentPassword: currentPassword) }
    }

    public func resetDevices(instance: FilebeamInstance, currentPassword: String) async throws {
        try await ffi { _ = try self.services(instance).accountDevicesReset(currentPassword: currentPassword) }
    }

    public func deviceFingerprint(publicKey: String) async throws -> String {
        try await ffi { try deviceKeyFingerprint(publicKey: publicKey) }
    }

    public func changelog(instance: FilebeamInstance, page: Int, term: String?, prereleases: Bool) async throws -> ChangelogListing {
        try await ffi {
            let json = try self.services(instance).changelogJson(page: UInt32(max(1, page)), term: term, prereleases: prereleases)
            let root = try Self.object(json)
            let listing = root["releases"] as? [String: Any] ?? [:]
            let rows = (listing["data"] as? [[String: Any]] ?? []).map(Self.summary)
            let versions = (root["versions"] as? [[String: Any]] ?? []).map(Self.summary)
            let archive = root["archive"] as? [String: Any] ?? [:]
            return ChangelogListing(
                releases: rows,
                page: listing["current_page"] as? Int ?? page,
                lastPage: listing["last_page"] as? Int ?? 1,
                versions: versions,
                hasPrereleases: root["has_prereleases"] as? Bool ?? false,
                archiveState: archive["state"] as? String ?? "ready",
                stale: archive["stale"] as? Bool ?? false
            )
        }
    }

    public func changelogRelease(instance: FilebeamInstance, tag: String) async throws -> ReleaseDetail {
        try await ffi {
            let root = try Self.object(try self.services(instance).changelogReleaseJson(tag: tag))
            guard let release = root["release"] as? [String: Any] else { throw FilebeamDomainError.remote("The release is unavailable.") }
            let document = release["document"] as? [String: Any] ?? [:]
            let blocks = (document["blocks"] as? [[String: Any]] ?? []).compactMap { block -> [ReleaseNode]? in
                guard block["type"] as? String == "content", let nodes = block["nodes"] as? [Any] else { return nil }
                return nodes.compactMap(Self.node)
            }
            return ReleaseDetail(
                summary: Self.summary(release),
                author: (release["author"] as? String).flatMap { $0.isEmpty || $0 == "null" ? nil : $0 },
                htmlURL: (release["html_url"] as? String).flatMap { $0.isEmpty ? nil : $0 },
                blocks: blocks,
                newer: (root["newer"] as? [String: Any]).map(Self.summary),
                older: (root["older"] as? [String: Any]).map(Self.summary)
            )
        }
    }

    public func combineKey(instance: FilebeamInstance, link: String, key: String) async throws -> String {
        try await ffi {
            let base = link.split(separator: "#", maxSplits: 1).first.map(String.init) ?? link
            let normalized = key.hasPrefix("v1.") ? key : "v1." + key
            return try presentShareLink(instance: instance.origin, shareUrl: base, shareKey: normalized, includeKey: true).link
        }
    }

    private static func object(_ json: String) throws -> [String: Any] {
        guard var root = try JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any] else { throw FilebeamDomainError.remote("The instance returned an unexpected document.") }
        if root["releases"] == nil && root["release"] == nil, let data = root["data"] as? [String: Any] { root = data }
        return root
    }

    private static func summary(_ value: [String: Any]) -> ReleaseSummary {
        let tag = value["tag"] as? String ?? ""
        let title = (value["title"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? tag
        return ReleaseSummary(tag: tag, title: title, description: value["description"] as? String ?? "", publishedAt: instant(value["published_at"] as? String), prerelease: value["prerelease"] as? Bool ?? false, latest: value["latest"] as? Bool ?? false, featured: value["featured"] as? Bool ?? false)
    }

    private static func node(_ value: Any) -> ReleaseNode? {
        if let text = value as? String { return .text(text) }
        guard let element = value as? [String: Any], let tag = element["t"] as? String else { return nil }
        let attributes = (element["a"] as? [String: Any] ?? [:]).compactMapValues { attribute -> String? in
            if let string = attribute as? String { return string }
            if let bool = attribute as? Bool { return bool ? "true" : "false" }
            if let number = attribute as? NSNumber { return number.stringValue }
            return nil
        }
        return .element(tag: tag, attributes: attributes, children: (element["c"] as? [Any] ?? []).compactMap(node))
    }

    private static func registry(_ value: FilebeamCore.DeviceRegistry) -> FilebeamDomain.DeviceRegistry {
        FilebeamDomain.DeviceRegistry(revision: value.revision, devices: value.devices.map { device in
            FilebeamDomain.RegisteredDevice(id: device.id, displayName: device.displayName, platform: device.platform, publicKey: device.publicKey, keyVersion: device.keyVersion, status: device.status, lastSeenAt: instant(device.lastSeenAt), presenceExpiresAt: instant(device.presenceExpiresAt), revokedAt: instant(device.revokedAt))
        })
    }

    static func instant(_ value: String?) -> Date? {
        guard let value, !value.isEmpty else { return nil }
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}
