import Foundation

public enum DeviceTrust: String, Codable, Hashable, Sendable { case pending, verified, revoked }
public enum DeviceAvailability: String, Codable, Hashable, Sendable { case ready, busy, needsOpen, locked, offline, unknown }

/// Presentation-safe registry entry. Device identity remains the immutable ID,
/// not the mutable display name.
public struct DeviceSnapshot: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let platform: String
    public let trust: DeviceTrust
    public let availability: DeviceAvailability
    public let detail: String?
    public let canReceive: Bool

    public init(id: String, name: String, platform: String, trust: DeviceTrust, availability: DeviceAvailability, detail: String? = nil, canReceive: Bool) {
        self.id = id; self.name = name; self.platform = platform; self.trust = trust; self.availability = availability; self.detail = detail; self.canReceive = canReceive
    }
}

public struct DeviceDirectory: Codable, Hashable, Sendable {
    public let instance: FilebeamInstance
    public let generation: UInt64
    public let revision: UInt64
    public let currentDeviceID: String?
    public let devices: [DeviceSnapshot]
    public let stale: Bool
    public let detail: String?

    public init(instance: FilebeamInstance, generation: UInt64, revision: UInt64, currentDeviceID: String?, devices: [DeviceSnapshot], stale: Bool, detail: String? = nil) {
        self.instance = instance; self.generation = generation; self.revision = revision; self.currentDeviceID = currentDeviceID; self.devices = devices; self.stale = stale; self.detail = detail
    }
}

/// Server-authored, one-use approval data. It contains no credential or private key.
public struct DevicePairingPayload: Codable, Hashable, Sendable {
    public let version: UInt8
    public let instanceOrigin: String
    public let deploymentID: String
    public let accountID: UInt64
    public let generation: UInt64
    public let candidateDeviceID: String
    public let candidateKeyVersion: UInt32
    public let candidatePublicKey: String
    public let challengeSHA256: String
    public let expiresAtUnix: UInt64
    enum CodingKeys: String, CodingKey { case version, instanceOrigin = "instance_origin", deploymentID = "deployment_id", accountID = "account_id", generation, candidateDeviceID = "candidate_device_id", candidateKeyVersion = "candidate_key_version", candidatePublicKey = "candidate_public_key", challengeSHA256 = "challenge_sha256", expiresAtUnix = "expires_at_unix" }
    public init(version: UInt8 = 2, instanceOrigin: String, deploymentID: String, accountID: UInt64, generation: UInt64, candidateDeviceID: String, candidateKeyVersion: UInt32, candidatePublicKey: String, challengeSHA256: String, expiresAtUnix: UInt64) {
        self.version = version; self.instanceOrigin = instanceOrigin; self.deploymentID = deploymentID; self.accountID = accountID; self.generation = generation; self.candidateDeviceID = candidateDeviceID; self.candidateKeyVersion = candidateKeyVersion; self.candidatePublicKey = candidatePublicKey; self.challengeSHA256 = challengeSHA256; self.expiresAtUnix = expiresAtUnix
    }
}
