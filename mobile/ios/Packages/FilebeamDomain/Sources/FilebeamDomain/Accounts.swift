import Foundation

public struct AccountSession: Codable, Hashable, Sendable, Identifiable {
    public let id: UInt64
    public let name: String
    public let username: String?
    public let email: String
    public let emailVerifiedAt: Date?
    public let profileURL: String?
    public let inboxEnabled: Bool
    public let usernameRoutingEnabled: Bool
    public let notificationChannel: String
    public init(id: UInt64, name: String, username: String?, email: String, emailVerifiedAt: Date?, profileURL: String?, inboxEnabled: Bool, usernameRoutingEnabled: Bool, notificationChannel: String) { self.id = id; self.name = name; self.username = username; self.email = email; self.emailVerifiedAt = emailVerifiedAt; self.profileURL = profileURL; self.inboxEnabled = inboxEnabled; self.usernameRoutingEnabled = usernameRoutingEnabled; self.notificationChannel = notificationChannel }
}

public enum KeyCustody: String, Codable, Sendable { case password, selfCustody }
public struct AccountKeyBundle: Codable, Hashable, Sendable, Identifiable {
    public let id: UInt64; public let userID: UInt64; public let version: UInt32; public let publicKey: String; public let fingerprint: String; public let custody: KeyCustody; public let encryptedPrivateKey: String?; public let isActive: Bool
    public init(id: UInt64, userID: UInt64, version: UInt32, publicKey: String, fingerprint: String, custody: KeyCustody, encryptedPrivateKey: String?, isActive: Bool) { self.id = id; self.userID = userID; self.version = version; self.publicKey = publicKey; self.fingerprint = fingerprint; self.custody = custody; self.encryptedPrivateKey = encryptedPrivateKey; self.isActive = isActive }
}

public struct KeySituation: Codable, Hashable, Sendable { public let activeBundleID: UInt64?; public let historicalBundleIDs: [UInt64]; public let replacementAcknowledgementRequired: Bool; public init(activeBundleID: UInt64?, historicalBundleIDs: [UInt64], replacementAcknowledgementRequired: Bool) { self.activeBundleID = activeBundleID; self.historicalBundleIDs = historicalBundleIDs; self.replacementAcknowledgementRequired = replacementAcknowledgementRequired } }
public struct InboxItem: Codable, Hashable, Sendable, Identifiable { public let id: TransferID; public let ciphertextBytes: UInt64; public let itemCount: UInt64; public let completedAt: Date?; public let expiresAt: Date?; public init(id: TransferID, ciphertextBytes: UInt64, itemCount: UInt64, completedAt: Date?, expiresAt: Date?) { self.id = id; self.ciphertextBytes = ciphertextBytes; self.itemCount = itemCount; self.completedAt = completedAt; self.expiresAt = expiresAt } }
public struct InboxMetadata: Codable, Hashable, Sendable { public let transferID: TransferID; public let encryptedManifest: String?; public let recipientKeyBundle: AccountKeyBundle; public let encryptedRecipientKey: String; public init(transferID: TransferID, encryptedManifest: String?, recipientKeyBundle: AccountKeyBundle, encryptedRecipientKey: String) { self.transferID = transferID; self.encryptedManifest = encryptedManifest; self.recipientKeyBundle = recipientKeyBundle; self.encryptedRecipientKey = encryptedRecipientKey } }
public struct OpenedInbox: Codable, Hashable, Sendable { public let transferID: TransferID; public let keyBundleID: UInt64; public let filenames: [String]; public init(transferID: TransferID, keyBundleID: UInt64, filenames: [String]) { self.transferID = transferID; self.keyBundleID = keyBundleID; self.filenames = filenames } }
public struct AccountPreferences: Codable, Hashable, Sendable { public var inboxEnabled: Bool; public var notificationChannel: String; public init(inboxEnabled: Bool, notificationChannel: String) { self.inboxEnabled = inboxEnabled; self.notificationChannel = notificationChannel } }
