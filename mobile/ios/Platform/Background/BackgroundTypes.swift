import Foundation

public enum BackgroundDirection: String, Codable, Sendable, Hashable {
    case upload
    case download
    case inboxDownload
}

public enum BackgroundHTTPEvent: Sendable, Equatable {
    case progress(checkpointID: String, done: UInt64, total: UInt64?)
    case readyToFinalize(checkpointID: String, direction: BackgroundDirection)
    case failed(checkpointID: String, message: String)
    case paused(checkpointID: String, direction: BackgroundDirection)
}

struct BackgroundTaskDescriptor: Codable, Equatable, Sendable {
    let operationID: String
    let checkpointID: String
    let direction: BackgroundDirection
    private static let prefix = "filebeam-bg-v1."

    init(operationID: String, checkpointID: String, direction: BackgroundDirection) throws {
        guard Self.validID(operationID), Self.validID(checkpointID) else { throw BackgroundJournalError.invalidState }
        self.operationID = operationID
        self.checkpointID = checkpointID
        self.direction = direction
    }

    var taskDescription: String {
        let data = (try? JSONEncoder().encode(self)) ?? Data()
        return Self.prefix + data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }

    static func parse(_ value: String?) -> BackgroundTaskDescriptor? {
        guard let value, value.hasPrefix(prefix) else { return nil }
        var encoded = String(value.dropFirst(prefix.count)).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        encoded += String(repeating: "=", count: (4 - encoded.count % 4) % 4)
        guard let data = Data(base64Encoded: encoded), let descriptor = try? JSONDecoder().decode(Self.self, from: data), validID(descriptor.operationID), validID(descriptor.checkpointID) else { return nil }
        return descriptor
    }

    private static func validID(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.count <= 160 && value.unicodeScalars.allSatisfy { $0.value >= 0x21 && $0.value <= 0x7e }
    }
}

enum BackgroundJournalError: Error { case invalidState, limitExceeded }
