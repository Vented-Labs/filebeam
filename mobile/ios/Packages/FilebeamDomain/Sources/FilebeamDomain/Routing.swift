import Foundation

public enum ExternalRequest: Codable, Hashable, Sendable, Identifiable {
    case receive(input: String, selectedInstance: FilebeamInstance)
    case profile(url: String)
    case invite(url: String)
    case verifyEmail(url: String)
    case resetPassword(url: String)
    case sharedFiles([FileSource])
    case sharedText(text: String, links: [String])

    public var id: String {
        switch self { case let .receive(input, _): return "receive:\(input)"; case let .profile(url): return "profile:\(url)"; case let .invite(url): return "invite:\(url)"; case let .verifyEmail(url): return "verify:\(url)"; case let .resetPassword(url): return "reset:\(url)"; case let .sharedFiles(files): return "files:\(files.map(\.id.uuidString).joined(separator: ","))"; case let .sharedText(text, links): return "text:\(text):\(links.joined(separator: ","))" }
    }
}

public enum InputRoute: Codable, Hashable, Sendable {
    case transfer(transferID: TransferID, instance: FilebeamInstance, input: String)
    case profile(url: String)
    case invite(url: String)
    case verifyEmail(url: String)
    case resetPassword(url: String)
    case invalid(String)
}

public enum InputRouter {
    /// Keeps the original input (including `#key`) for the receive workflow.
    public static func route(_ input: String, selectedInstance: FilebeamInstance) -> InputRoute {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return .invalid("Enter a transfer link or ID.") }
        guard let components = URLComponents(string: trimmed), let scheme = components.scheme, let host = components.host else {
            return .transfer(transferID: TransferID(trimmed), instance: selectedInstance, input: trimmed)
        }
        guard scheme == "https", let instance = FilebeamInstance(origin: "https://\(host)\(components.port.map { ":\($0)" } ?? "")") else {
            return .invalid("Use an HTTPS Filebeam link.")
        }
        let path = components.path.lowercased()
        if path.contains("verify") { return .verifyEmail(url: trimmed) }
        if path.contains("reset") { return .resetPassword(url: trimmed) }
        if path.contains("invite") { return .invite(url: trimmed) }
        if path.contains("profile") || path.hasPrefix("/u/") { return .profile(url: trimmed) }
        let id = components.path.split(separator: "/").last.map(String.init) ?? ""
        return id.isEmpty ? .invalid("The link does not identify a transfer.") : .transfer(transferID: TransferID(id), instance: instance, input: trimmed)
    }
}

public struct ExternalRequestCoordinator: Codable, Hashable, Sendable {
    public private(set) var active: ExternalRequest?
    public private(set) var queued: [ExternalRequest]
    public init(active: ExternalRequest? = nil, queued: [ExternalRequest] = []) { self.active = active; self.queued = queued }

    /// Never replaces an active draft or consent request. The UI explicitly accepts or dismisses entries.
    public mutating func enqueue(_ request: ExternalRequest) { if active == nil { active = request } else { queued.append(request) } }
    public mutating func completeActive() { active = queued.isEmpty ? nil : queued.removeFirst() }
    public mutating func dismissActive() { completeActive() }
}
