import Foundation
import FilebeamDomain

public enum NativePolicyDecoder {
    public enum Error: Swift.Error, Equatable {
        case malformed(String)
    }

    public static func decode(_ json: String, instance: FilebeamInstance) throws -> InstancePolicy {
        let response: Response
        do {
            response = try JSONDecoder().decode(Response.self, from: Data(json.utf8))
        } catch {
            throw Error.malformed("Invalid authenticated policy JSON: \(error.localizedDescription)")
        }

        let enabledDrivers = Set(response.transfer.enabledDrivers)
        guard !enabledDrivers.isEmpty,
              enabledDrivers.count == response.transfer.enabledDrivers.count,
              enabledDrivers.isSubset(of: supportedDrivers),
              enabledDrivers.contains(response.transfer.defaultDriver),
              response.transfer.chunkBytes > 0,
              response.transfer.chunkBytes <= maximumChunkBytes else {
            throw Error.malformed("Authenticated policy contains unsupported transfer settings.")
        }

        guard response.transfer.retention.files.options.allSatisfy({ $0 > 0 }),
              response.transfer.retention.notes.options.allSatisfy({ $0 > 0 }),
              response.transfer.retention.files.defaultHours > 0,
              response.transfer.retention.notes.defaultHours > 0 else {
            throw Error.malformed("Authenticated policy contains invalid retention settings.")
        }

        var drivers: [String: DriverPolicy] = [:]
        for name in enabledDrivers {
            guard let limits = response.transfer.limits[name], limits.maximumFileCount.isSupportedCount else {
                throw Error.malformed("Authenticated policy has invalid limits for \(name).")
            }
            drivers[name] = DriverPolicy(
                name: name,
                maximumTransferBytes: limits.maximumTransferBytes.value,
                maximumFileCount: limits.maximumFileCount.value,
                maximumNoteBytes: limits.maximumNoteBytes.value
            )
        }

        return InstancePolicy(
            instance: instance,
            anonymousUploads: response.transfer.anonymousUploadsEnabled,
            enabledTransports: Set(enabledDrivers.compactMap { $0 == "http" ? .http : $0 == "webrtc" ? .webRTC : nil }),
            defaultDriver: response.transfer.defaultDriver,
            chunkBytes: response.transfer.chunkBytes,
            retentionOptionsHours: Set(response.transfer.retention.files.options),
            drivers: drivers,
            registrationEnabled: response.account.registrationEnabled,
            usernameRoutingEnabled: response.account.usernameRoutingEnabled,
            noteRetentionOptionsHours: Set(response.transfer.retention.notes.options),
            fileDefaultRetentionHours: response.transfer.retention.files.defaultHours,
            noteDefaultRetentionHours: response.transfer.retention.notes.defaultHours
        )
    }

    private static let supportedDrivers: Set<String> = ["http", "webrtc"]
    private static let maximumChunkBytes: UInt64 = 24_999_984
    private static let maximumItems: UInt64 = 65_535

    private struct Response: Decodable {
        let account: Account
        let transfer: Transfer
    }

    private struct Account: Decodable {
        let registrationEnabled: Bool?
        let usernameRoutingEnabled: Bool?
    }

    private struct Transfer: Decodable {
        let anonymousUploadsEnabled: Bool
        let defaultDriver: String
        let enabledDrivers: [String]
        let chunkBytes: UInt64
        let limits: [String: DriverLimits]
        let retention: Retention
    }

    private struct Retention: Decodable {
        let files: RetentionKind
        let notes: RetentionKind
    }

    private struct RetentionKind: Decodable {
        let defaultHours: UInt64
        let options: [UInt64]
    }

    private struct DriverLimits: Decodable {
        let maximumTransferBytes: DecodedLimit
        let maximumFileCount: DecodedLimit
        let maximumNoteBytes: DecodedLimit

        enum CodingKeys: String, CodingKey {
            case maximumTransferBytes = "maximum_transfer_bytes"
            case maximumFileCount = "maximum_file_count"
            case maximumNoteBytes = "maximum_note_bytes"
        }

        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            maximumTransferBytes = try Self.limit(in: container, forKey: .maximumTransferBytes)
            maximumFileCount = try Self.limit(in: container, forKey: .maximumFileCount)
            maximumNoteBytes = try Self.limit(in: container, forKey: .maximumNoteBytes)
        }

        private static func limit(in container: KeyedDecodingContainer<CodingKeys>, forKey key: CodingKeys) throws -> DecodedLimit {
            guard container.contains(key) else { return .unknown }
            return try container.decode(DecodedLimit.self, forKey: key)
        }
    }

    private enum DecodedLimit: Decodable {
        case finite(UInt64)
        case unlimited
        case unknown

        init(from decoder: Decoder) throws {
            let container = try decoder.singleValueContainer()
            self = container.decodeNil() ? .unlimited : .finite(try container.decode(UInt64.self))
        }

        var value: Limit {
            switch self {
            case let .finite(value): .finite(value)
            case .unlimited: .unlimited
            case .unknown: .unknown
            }
        }

        var isSupportedCount: Bool {
            switch self {
            case let .finite(value): value <= NativePolicyDecoder.maximumItems
            case .unlimited, .unknown: true
            }
        }
    }
}
