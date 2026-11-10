import SwiftUI
import FilebeamTheme

enum ThemeMode: String, Codable, CaseIterable, Sendable {
    case system, light, dark
    var colorScheme: ColorScheme? {
        switch self { case .system: nil; case .light: .light; case .dark: .dark }
    }
}

/// Where the accent and surfaces come from.
enum ColorSource: Hashable, Codable, Sendable {
    /// The colour the connected Filebeam instance is branded with.
    case instance
    case preset(ThemePreset)
    /// A colour picked by the user, packed as 0xRRGGBB.
    case custom(Int)

    var encoded: String {
        switch self {
        case .instance: "instance"
        case let .preset(preset): "preset:\(preset.rawValue)"
        case let .custom(seed): "custom:" + String(format: "%06x", seed & 0xFFFFFF)
        }
    }

    init(encoded: String?) {
        guard let encoded else { self = .instance; return }
        if encoded.hasPrefix("preset:"), let preset = ThemePreset(rawValue: String(encoded.dropFirst(7))) { self = .preset(preset) }
        else if encoded.hasPrefix("custom:"), let seed = Int(encoded.dropFirst(7), radix: 16) { self = .custom(seed & 0xFFFFFF) }
        else { self = .instance }
    }
}

struct Appearance: Hashable, Codable, Sendable {
    var mode: ThemeMode = .system
    var source: ColorSource = .instance
}

extension Color {
    init(rgb: Int, opacity: Double = 1) {
        self.init(.sRGB, red: Double((rgb >> 16) & 0xFF) / 255, green: Double((rgb >> 8) & 0xFF) / 255, blue: Double(rgb & 0xFF) / 255, opacity: opacity)
    }
    init(_ value: RGBA) { self.init(rgb: value.rgb, opacity: value.alpha) }
}

struct StatusColors: Hashable {
    let ink: Color, surface: Color, border: Color
    init(_ roles: StatusRoles) { ink = Color(rgb: roles.ink); surface = Color(rgb: roles.surface); border = Color(rgb: roles.border) }
    init(ink: Color, surface: Color, border: Color) { self.ink = ink; self.surface = surface; self.border = border }
}

struct EditorColors: Hashable {
    let background, gutterBackground, gutterText, gutterActiveBackground, gutterActiveText, border, activeLine, selection, caret, text: Color
    let keyword, string, number, constant, function, property, type, tag, attribute, comment, punctuation, `operator`, meta, regexp, escape, link, heading, invalid: Color
    init(_ r: EditorRoles) {
        background = Color(rgb: r.background); gutterBackground = Color(rgb: r.gutterBackground); gutterText = Color(rgb: r.gutterText)
        gutterActiveBackground = Color(rgb: r.gutterActiveBackground); gutterActiveText = Color(rgb: r.gutterActiveText); border = Color(rgb: r.border)
        activeLine = Color(r.activeLine); selection = Color(rgb: r.selection); caret = Color(rgb: r.caret); text = Color(rgb: r.text)
        keyword = Color(rgb: r.keyword); string = Color(rgb: r.string); number = Color(rgb: r.number); constant = Color(rgb: r.constant)
        function = Color(rgb: r.function); property = Color(rgb: r.property); type = Color(rgb: r.type); tag = Color(rgb: r.tag)
        attribute = Color(rgb: r.attribute); comment = Color(rgb: r.comment); punctuation = Color(rgb: r.punctuation); `operator` = Color(rgb: r.operator)
        meta = Color(rgb: r.meta); regexp = Color(rgb: r.regexp); escape = Color(rgb: r.escape); link = Color(rgb: r.link); heading = Color(rgb: r.heading); invalid = Color(rgb: r.invalid)
    }
    func color(for kind: TokenKind) -> Color {
        switch kind {
        case .keyword: keyword; case .string: string; case .number: number; case .constant: constant; case .function: function
        case .property: property; case .type: type; case .tag: tag; case .attribute: attribute; case .comment: comment
        case .punctuation: punctuation; case .operator: `operator`; case .meta: meta; case .escape: escape; case .link: link
        case .heading: heading; case .invalid: invalid
        }
    }
}

/// Every colour the app draws with, as SwiftUI colours. Built from a `Palette` so a screen looks
/// the same whether the palette came from a preset, the instance or a custom colour.
struct Theme: Hashable {
    let palette: Palette
    let dark: Bool
    let chrome, background, surface, raised, sunken, border, controlBorder, controlBorderHover, hover, active: Color
    let text, textMuted, textSubtle, disabledBorder, disabledSurface, disabledText: Color
    let accentText, accentSurface, selectedSurface: Color
    let action, actionTop, actionPressed, onAction, actionBorder, choiceBorder, progressFill, progressTrack: Color
    let success, warning, danger, info: StatusColors
    let scrim, lineSoft, lineStrong, shadow, highlight, sheen: Color
    let brand: Color
    let brandGradient: [Color]
    let glowPrimary, glowSecondary: Color
    let cardBack, cardBackEnd, cardFront, cardFrontEnd: Color
    let editor: EditorColors

    init(_ p: Palette) {
        palette = p
        dark = p.dark
        chrome = Color(rgb: p.chrome); background = Color(rgb: p.background); surface = Color(rgb: p.surface); raised = Color(rgb: p.raised); sunken = Color(rgb: p.sunken)
        border = Color(rgb: p.border); controlBorder = Color(rgb: p.controlBorder); controlBorderHover = Color(rgb: p.controlBorderHover); hover = Color(rgb: p.hover); active = Color(rgb: p.active)
        text = Color(rgb: p.text); textMuted = Color(rgb: p.textMuted); textSubtle = Color(rgb: p.textSubtle); disabledBorder = Color(rgb: p.disabledBorder); disabledSurface = Color(rgb: p.disabledSurface); disabledText = Color(rgb: p.disabledText)
        accentText = Color(rgb: p.accentText); accentSurface = Color(rgb: p.accentSurface); selectedSurface = Color(rgb: p.selectedSurface)
        action = Color(rgb: p.action); actionTop = Color(rgb: p.actionTop); actionPressed = Color(rgb: p.actionPressed); onAction = Color(rgb: p.onAction); actionBorder = Color(rgb: p.actionBorder); choiceBorder = Color(rgb: p.choiceBorder); progressFill = Color(rgb: p.progressFill); progressTrack = Color(rgb: p.progressTrack)
        success = StatusColors(p.success); warning = StatusColors(p.warning); danger = StatusColors(p.danger); info = StatusColors(p.info)
        scrim = Color(p.scrim); lineSoft = Color(p.lineSoft); lineStrong = Color(p.lineStrong); shadow = Color(p.shadow); highlight = Color(p.highlight); sheen = Color(p.sheen)
        brand = Color(rgb: p.brand)
        brandGradient = p.brandGradient.map { Color(rgb: $0) }
        glowPrimary = Color(p.glowPrimary); glowSecondary = Color(p.glowSecondary)
        cardBack = Color(rgb: p.cardBack); cardBackEnd = Color(rgb: p.cardBackEnd); cardFront = Color(rgb: p.cardFront); cardFrontEnd = Color(rgb: p.cardFrontEnd)
        editor = EditorColors(p.editor)
    }

    /// Dark themes use a subtle top-lit gradient on primary actions; light themes stay flat.
    var actionGradient: LinearGradient { LinearGradient(colors: dark ? [actionTop, action] : [action, action], startPoint: .top, endPoint: .bottom) }
    var brandLinearGradient: LinearGradient { LinearGradient(colors: brandGradient, startPoint: .topLeading, endPoint: .bottomTrailing) }
    var avatarGradient: LinearGradient { LinearGradient(colors: Array(brandGradient.suffix(3)), startPoint: .topLeading, endPoint: .bottomTrailing) }

    /// Resolves an appearance to concrete colours. `instanceColor` is the connected instance's brand colour.
    static func resolve(_ appearance: Appearance, systemDark: Bool, instanceColor: Int?) -> Theme {
        let dark: Bool = switch appearance.mode { case .system: systemDark; case .light: false; case .dark: true }
        let palette: Palette = switch appearance.source {
        case .instance: PaletteEngine.seeded(instanceColor ?? PaletteEngine.defaultPrimary, dark: dark)
        case let .preset(preset): PaletteEngine.preset(preset, dark: dark)
        case let .custom(seed): PaletteEngine.seeded(seed, dark: dark)
        }
        return Theme(palette)
    }

    static let fallback = Theme(PaletteEngine.preset(.purple, dark: false))
}

private struct ThemeKey: EnvironmentKey { static let defaultValue = Theme.fallback }
private struct ReduceMotionKey: EnvironmentKey { static let defaultValue = false }

extension EnvironmentValues {
    /// The active Filebeam theme. Every component reads colours from here, never from asset catalogs.
    var theme: Theme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}

/// Device-local look and first-run flags. Nothing here is synced to an account.
@MainActor @Observable
final class AppearanceStore {
    private let defaults: UserDefaults
    private static let modeKey = "appearance.mode"
    private static let sourceKey = "appearance.source"
    private static let tourKey = "tour.seen"
    private static let peerKey = "peer-exposure.accepted-at"

    var appearance: Appearance {
        didSet {
            defaults.set(appearance.mode.rawValue, forKey: Self.modeKey)
            defaults.set(appearance.source.encoded, forKey: Self.sourceKey)
        }
    }
    var hasSeenTour: Bool {
        didSet { defaults.set(hasSeenTour, forKey: Self.tourKey) }
    }
    /// When the user last agreed to expose their address to a peer; consent lasts a year, as on the web.
    var peerExposureAcceptedAt: Date? {
        didSet { defaults.set(peerExposureAcceptedAt?.timeIntervalSince1970, forKey: Self.peerKey) }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        appearance = Appearance(mode: ThemeMode(rawValue: defaults.string(forKey: Self.modeKey) ?? "") ?? .system, source: ColorSource(encoded: defaults.string(forKey: Self.sourceKey)))
        hasSeenTour = defaults.bool(forKey: Self.tourKey)
        let accepted = defaults.double(forKey: Self.peerKey)
        peerExposureAcceptedAt = accepted > 0 ? Date(timeIntervalSince1970: accepted) : nil
    }

    var recentlyConsentedToPeerExposure: Bool {
        guard let peerExposureAcceptedAt else { return false }
        return Date().timeIntervalSince(peerExposureAcceptedAt) < 365 * 24 * 3600
    }
}
