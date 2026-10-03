import Foundation

public struct ReceivingDefaults: Codable, Hashable, Sendable {
    public var receivingPolicy: String
    public var autoDownloadFriends: Bool
    public let revision: UInt64
}
public struct EffectiveReceiving: Codable, Hashable, Sendable {
    public let canSend: Bool
    public let autoDownload: Bool
}
public struct Contact: Codable, Hashable, Sendable, Identifiable {
    public let id: UInt64
    public let username: String
    public let name: String
    public let status: String
    public var canSend: Bool?
    public var autoDownload: Bool?
    public let effective: EffectiveReceiving
}
public struct BlockedContact: Codable, Hashable, Sendable, Identifiable {
    public let id: UInt64
    public let username: String
    public let name: String
}
public struct ContactDirectory: Codable, Hashable, Sendable {
    public var settings: ReceivingDefaults
    public var contacts: [Contact]
    public let blocked: [BlockedContact]
}
public struct StagedInboxItem: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let bytes: UInt64
    public let state: String
    public let keyBundleID: UInt64
    enum CodingKeys: String, CodingKey { case id, bytes, state; case keyBundleID = "key_bundle_id" }
}
public struct InboxReceiverState: Codable, Hashable, Sendable {
    public let enabled: Bool
    public let entries: [StagedInboxItem]
}
