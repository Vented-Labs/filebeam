import Foundation

/// Colour presets shared with the web app. `seed` and `endpoint` anchor the brand gradient.
public enum ThemePreset: String, CaseIterable, Codable, Sendable {
    case purple, blue, teal, green, amber, orange, rose

    public var seed: RGB {
        switch self {
        case .purple: 0x8b35ff
        case .blue: 0x3d78d8
        case .teal: 0x168f85
        case .green: 0x388b59
        case .amber: 0xcf9b35
        case .orange: 0xda7b40
        case .rose: 0xcf5d78
        }
    }

    public var endpoint: RGB {
        switch self {
        case .purple: 0xc05ada
        case .blue: 0x4a9dbd
        case .teal: 0x48a778
        case .green: 0x76a543
        case .amber: 0xd78135
        case .orange: 0xd46369
        case .rose: 0xb570c0
        }
    }

    public var title: String { rawValue.prefix(1).uppercased() + rawValue.dropFirst() }

    public static func matching(seed: RGB) -> ThemePreset? { allCases.first { $0.seed == seed & 0xFFFFFF } }
}
