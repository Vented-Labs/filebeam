import Foundation

/// Non-content metadata used to select a receive workflow before transfer bytes are read.
public struct TransferInspection: Codable, Hashable, Sendable {
    public let kind: TransferKind
    public let transport: Transport?
    public let passwordRequired: Bool
    public let burnOnRead: Bool
    /// The server's lifecycle word for the transfer, e.g. "available" or "expired".
    public let status: String

    public init(kind: TransferKind, transport: Transport? = nil, passwordRequired: Bool = false, burnOnRead: Bool = false, status: String = "available") {
        self.kind = kind
        self.transport = transport
        self.passwordRequired = passwordRequired
        self.burnOnRead = burnOnRead
        self.status = status
    }

    /// Statuses in which the server will not serve the payload any more.
    public static let unavailableStatuses: Set<String> = ["expired", "deleted", "removed", "ended", "burned", "abandoned", "cleanup"]
    public var isUnavailable: Bool { Self.unavailableStatuses.contains(status) }
}
