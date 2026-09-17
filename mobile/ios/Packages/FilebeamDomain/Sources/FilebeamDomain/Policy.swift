import Foundation

public struct DriverPolicy: Codable, Hashable, Sendable {
    public let name: String
    public let maximumTransferBytes: Limit
    public let maximumFileCount: Limit
    public let maximumNoteBytes: Limit
    public init(name: String, maximumTransferBytes: Limit, maximumFileCount: Limit, maximumNoteBytes: Limit) { self.name = name; self.maximumTransferBytes = maximumTransferBytes; self.maximumFileCount = maximumFileCount; self.maximumNoteBytes = maximumNoteBytes }
}

public struct InstancePolicy: Codable, Hashable, Sendable {
    public let instance: FilebeamInstance
    public let anonymousUploads: Bool
    public let enabledTransports: Set<Transport>
    public let defaultDriver: String
    public let chunkBytes: UInt64
    public let retentionOptionsHours: Set<UInt64>
    public let registrationEnabled: Bool?
    public let usernameRoutingEnabled: Bool?
    public let noteRetentionOptionsHours: Set<UInt64>
    public let fileDefaultRetentionHours: UInt64?
    public let noteDefaultRetentionHours: UInt64?
    public let drivers: [String: DriverPolicy]
    public init(instance: FilebeamInstance, anonymousUploads: Bool, enabledTransports: Set<Transport>, defaultDriver: String, chunkBytes: UInt64, retentionOptionsHours: Set<UInt64>, drivers: [String: DriverPolicy], registrationEnabled: Bool? = nil, usernameRoutingEnabled: Bool? = nil, noteRetentionOptionsHours: Set<UInt64>? = nil, fileDefaultRetentionHours: UInt64? = nil, noteDefaultRetentionHours: UInt64? = nil) {
        self.instance = instance; self.anonymousUploads = anonymousUploads; self.enabledTransports = enabledTransports; self.defaultDriver = defaultDriver; self.chunkBytes = chunkBytes; self.retentionOptionsHours = retentionOptionsHours; self.drivers = drivers
        self.registrationEnabled = registrationEnabled; self.usernameRoutingEnabled = usernameRoutingEnabled
        self.noteRetentionOptionsHours = noteRetentionOptionsHours ?? retentionOptionsHours
        self.fileDefaultRetentionHours = fileDefaultRetentionHours; self.noteDefaultRetentionHours = noteDefaultRetentionHours
    }
}

public enum SendPolicy {
    public static func validate(_ draft: FileDraft, policy: InstancePolicy, isAuthenticated: Bool) -> [DraftValidation] {
        var result: [DraftValidation] = []
        if draft.sources.isEmpty { result.append(.blocked("Select at least one file.")) }
        for source in draft.sources {
            switch source.state {
            case .ready, .sizePending: break
            case .importing: result.append(.blocked("Wait for file import to finish."))
            case let .importFailed(message): result.append(.blocked(message.isEmpty ? "A selected file could not be imported." : message))
            }
        }
        if !policy.anonymousUploads && !isAuthenticated { result.append(.blocked("Sign in to upload to this instance.")) }
        if !policy.enabledTransports.contains(draft.options.transport) { result.append(.blocked("The selected transport is unavailable.")) }
        if draft.options.turbo && draft.options.transport != .http { result.append(.blocked("Turbo Transfer requires HTTP.")) }
        if let recipient = draft.options.recipient {
            if draft.options.transport != .http { result.append(.blocked("Recipient delivery requires HTTP.")) }
            if draft.options.turbo { result.append(.blocked("Recipient delivery cannot use Turbo Transfer.")) }
            if draft.options.passwordEnabled { result.append(.blocked("Recipient delivery cannot use a transfer password.")) }
            if recipient.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || recipient.publicKey.isEmpty { result.append(.blocked("A validated recipient key is required.")) }
        }
        result += retention(draft.options.retentionHours, options: policy.retentionOptionsHours)
        guard let driver = driver(for: draft.options.transport, override: draft.options.driver, policy: policy) else { return result + [.blocked("The selected transport driver is unavailable.")] }
        let count = draft.options.archive ? 1 : UInt64(draft.sources.count)
        if UInt64(draft.sources.count) > protocolMaximumItems { result.append(.blocked("The selection exceeds the protocol file-count limit.")) }
        result += check(count, against: driver.maximumFileCount, message: "The selection exceeds this driver's file-count limit.")
        // Archive planning is authoritative in the native core. It must not prevent preparation.
        if case .unknown = driver.maximumTransferBytes {
            result.append(.unknown("The driver's byte limit is unavailable."))
        } else if !draft.options.archive, draft.sources.contains(where: { $0.sizeBytes == nil || $0.state == .sizePending }) {
            if case .unlimited = driver.maximumTransferBytes {} else { result.append(.unknown("One or more source sizes are pending or unavailable.")) }
        }
        else if case .finite = driver.maximumTransferBytes {
            let sizes = draft.sources.compactMap(\.sizeBytes)
            guard let ciphertext = ciphertextBytes(fileSizes: sizes, chunkBytes: policy.chunkBytes) else { result.append(.unknown("Encrypted size cannot be calculated.")); return result }
            result += check(ciphertext, against: driver.maximumTransferBytes, message: "The encrypted selection exceeds this driver's byte limit.")
        }
        return result
    }

    public static func validate(_ draft: NoteDraft, policy: InstancePolicy, isAuthenticated: Bool) -> [DraftValidation] {
        var result: [DraftValidation] = []
        if draft.text.isEmpty { result.append(.blocked("Enter note text.")) }
        if !policy.anonymousUploads && !isAuthenticated { result.append(.blocked("Sign in to upload to this instance.")) }
        let transport: Transport = draft.options.live ? .webRTC : .http
        if !policy.enabledTransports.contains(transport) { result.append(.blocked("The selected transport is unavailable.")) }
        result += retention(draft.options.retentionHours, options: policy.noteRetentionOptionsHours)
        guard let driver = driver(for: transport, override: draft.options.driver, policy: policy) else { return result + [.blocked("The selected transport driver is unavailable.")] }
        // Note limits are specified over the encrypted note payload, not Swift character count.
        guard let ciphertext = ciphertextBytes(fileSizes: [UInt64(draft.text.lengthOfBytes(using: .utf8))], chunkBytes: policy.chunkBytes) else { return result + [.unknown("Encrypted note size cannot be calculated.")] }
        if case .unknown = driver.maximumNoteBytes { return result + [.unknown("The driver's note byte limit is unavailable.")] }
        result += check(ciphertext, against: driver.maximumNoteBytes, message: "The encrypted note exceeds this driver's byte limit.")
        return result
    }

    /// Native v1 file encryption emits a tag per chunk. It deliberately declines invalid chunk sizes and overflow.
    public static func ciphertextBytes(fileSizes: [UInt64], chunkBytes: UInt64) -> UInt64? {
        guard chunkBytes > 0, chunkBytes <= protocolMaximumChunkBytes else { return nil }
        var total: UInt64 = 0
        for size in fileSizes {
            let rounded = size.addingReportingOverflow(chunkBytes - 1)
            guard !rounded.overflow else { return nil }
            let chunks = max(1, rounded.partialValue / chunkBytes)
            guard chunks <= protocolMaximumChunksPerItem else { return nil }
            let overhead = chunks.multipliedReportingOverflow(by: 16)
            guard !overhead.overflow else { return nil }
            let plaintext = total.addingReportingOverflow(size)
            guard !plaintext.overflow else { return nil }
            let ciphertext = plaintext.partialValue.addingReportingOverflow(overhead.partialValue)
            guard !ciphertext.overflow else { return nil }
            total = ciphertext.partialValue
        }
        return total
    }

    private static let protocolMaximumChunkBytes: UInt64 = 24_999_984
    private static let protocolMaximumChunksPerItem: UInt64 = 65_535
    private static let protocolMaximumItems: UInt64 = 65_535

    private static func driver(for transport: Transport, override: String?, policy: InstancePolicy) -> DriverPolicy? {
        let name = transport == .http ? "http" : "webrtc"
        guard override == nil || override == name else { return nil }
        return policy.drivers[name]
    }
    private static func check(_ value: UInt64, against limit: Limit, message: String) -> [DraftValidation] { if case let .finite(maximum) = limit, value > maximum { [.blocked(message)] } else { [] } }
    private static func retention(_ value: UInt64?, options: Set<UInt64>) -> [DraftValidation] { guard let value else { return [] }; return options.isEmpty || options.contains(value) ? [] : [.blocked("The selected lifetime is unavailable.")] }
}
