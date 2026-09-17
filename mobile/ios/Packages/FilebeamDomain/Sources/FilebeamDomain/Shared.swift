import Foundation

public struct FilebeamInstance: Codable, Hashable, Sendable, Identifiable {
    public let origin: String
    public var id: String { origin }

    /// Accepts a canonical HTTPS origin only. Links, keys, paths, and credentials are never instance values.
    public init?(origin: String) {
        guard let components = URLComponents(string: origin.trimmingCharacters(in: .whitespacesAndNewlines)),
              components.scheme == "https", components.host != nil,
              components.user == nil, components.password == nil,
              (components.path.isEmpty || components.path == "/"),
              components.query == nil, components.fragment == nil else { return nil }
        self.origin = "https://\(components.host!)\(components.port.map { ":\($0)" } ?? "")"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        guard let value = try Self(origin: container.decode(String.self)) else {
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Filebeam instances must be canonical HTTPS origins.")
        }
        self = value
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(origin)
    }
}

public struct TransferID: Codable, Hashable, Sendable, Identifiable, ExpressibleByStringLiteral {
    public let rawValue: String
    public var id: String { rawValue }
    public init(_ rawValue: String) { self.rawValue = rawValue }
    public init(stringLiteral value: String) { self.init(value) }
}

public enum FilebeamDomainError: Error, Codable, Hashable, Sendable {
    case invalidInput(String)
    case unavailable(String)
    case rejected(String)
    case network(String)
    case remote(String)
    case storage(String)
    case cryptography(String)
    case cancelled
}

public enum Transport: String, Codable, CaseIterable, Sendable { case http, webRTC }

public enum Limit: Codable, Hashable, Sendable {
    case finite(UInt64)
    case unlimited
    case unknown

    public var finiteValue: UInt64? { if case let .finite(value) = self { value } else { nil } }
}

public enum DraftValidation: Codable, Hashable, Sendable {
    case ready
    case blocked(String)
    case unknown(String)
}

extension Array where Element == DraftValidation {
    public var submissionStatus: DraftValidation {
        if let blocked = first(where: { if case .blocked = $0 { true } else { false } }) { return blocked }
        if let unknown = first(where: { if case .unknown = $0 { true } else { false } }) { return unknown }
        return .ready
    }
}
