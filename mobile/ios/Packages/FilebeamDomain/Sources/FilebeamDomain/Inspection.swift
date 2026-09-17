import Foundation

/// Non-content metadata used to select a receive workflow before transfer bytes are read.
public struct TransferInspection: Codable, Hashable, Sendable {
    public let kind: TransferKind
    public let transport: Transport?
    public let passwordRequired: Bool
    public let burnOnRead: Bool

    public init(kind: TransferKind, transport: Transport? = nil, passwordRequired: Bool = false, burnOnRead: Bool = false) {
        self.kind = kind
        self.transport = transport
        self.passwordRequired = passwordRequired
        self.burnOnRead = burnOnRead
    }
}
