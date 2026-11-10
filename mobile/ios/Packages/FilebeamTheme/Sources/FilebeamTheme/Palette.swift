import Foundation

/// A colour with an alpha channel, for scrims, lines and glows.
public struct RGBA: Hashable, Sendable {
    public var rgb: RGB
    public var alpha: Double
    public init(_ rgb: RGB, alpha: Double = 1) { self.rgb = rgb; self.alpha = alpha }
    public static let clear = RGBA(0, alpha: 0)
}

public struct StatusRoles: Hashable, Sendable {
    public var ink: RGB
    public var surface: RGB
    public var border: RGB
    public init(ink: RGB, surface: RGB, border: RGB) { self.ink = ink; self.surface = surface; self.border = border }
}

/// Syntax and surface colours for the note editor and viewer.
public struct EditorRoles: Hashable, Sendable {
    public var background, gutterBackground, gutterText, gutterActiveBackground, gutterActiveText, border: RGB
    public var activeLine: RGBA
    public var selection, caret, text, keyword, string, number, constant, function, property, type, tag, attribute: RGB
    public var comment, punctuation, `operator`, meta, regexp, escape, link, heading, invalid: RGB
}

/// Every colour role the app draws with. Roles mirror the web tokens (`--fb-*`) so a screen
/// looks the same whether the palette came from a preset or a custom colour.
public struct Palette: Hashable, Sendable {
    public var dark: Bool
    public var chrome, background, surface, raised, sunken, border, controlBorder, controlBorderHover, hover, active: RGB
    public var text, textMuted, textSubtle, disabledBorder, disabledSurface, disabledText: RGB
    public var accentText, accentSurface, selectedSurface: RGB
    public var action, actionTop, actionPressed, onAction, actionBorder, choiceBorder, progressFill, progressTrack: RGB
    public var success, warning, danger, info: StatusRoles
    public var scrim, lineSoft, lineStrong, shadow, highlight, sheen: RGBA
    public var brand: RGB
    /// Five stops matching the web's 135 degree mark gradient.
    public var brandGradient: [RGB]
    public var glowPrimary, glowSecondary: RGBA
    public var cardBack, cardBackEnd, cardFront, cardFrontEnd: RGB
    public var editor: EditorRoles

    /// Stops for `brandGradient`.
    public static let brandStops: [Double] = [0, 0.275, 0.55, 0.775, 1]
}
