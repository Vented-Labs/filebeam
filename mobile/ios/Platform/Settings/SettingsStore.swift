import Foundation

struct AppSettings: Codable, Sendable, Equatable {
    var instanceOrigin: String = "https://filebeam.io"
    var relayOnly: Bool = false
}

final class SettingsStore {
    private let defaults: UserDefaults
    init(defaults: UserDefaults = .standard) { self.defaults = defaults }
    func load() throws -> AppSettings {
        guard let data = defaults.data(forKey: "app-settings") else { return AppSettings() }
        let value = try JSONDecoder().decode(AppSettings.self, from: data)
        return try validated(value)
    }
    func save(_ settings: AppSettings) throws { defaults.set(try JSONEncoder().encode(validated(settings)), forKey: "app-settings") }
    private func validated(_ settings: AppSettings) throws -> AppSettings {
        guard var c = URLComponents(string: settings.instanceOrigin.trimmingCharacters(in: .whitespacesAndNewlines)), c.scheme?.lowercased() == "https", c.host != nil, c.user == nil, c.password == nil else { throw CocoaError(.validationMissingMandatoryProperty) }
        c.path = ""; c.query = nil; c.fragment = nil
        guard let origin = c.url?.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/")) else { throw CocoaError(.validationCorrupt) }
        return AppSettings(instanceOrigin: origin, relayOnly: settings.relayOnly)
    }
}
