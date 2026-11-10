import Foundation
import XCTest
@testable import FilebeamTheme

/// The fixture is the web theme generator's output; iOS must derive identical colours.
final class PaletteEngineTests: XCTestCase {
    private func golden() throws -> [String: [String: [String: String]]] {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "palette-golden", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode([String: [String: [String: String]]].self, from: Data(contentsOf: url))
    }

    private func hexes(_ value: String) -> [String] {
        value.matches(of: /#[0-9a-f]{6}/).map { String($0.output) }
    }

    private func roles(_ palette: Palette) -> [(String, RGB)] {
        [
            ("--fb-bg", palette.background), ("--fb-surface", palette.surface), ("--fb-surface-raised", palette.raised),
            ("--fb-surface-sunken", palette.sunken), ("--fb-border", palette.border), ("--fb-control-border", palette.controlBorder),
            ("--fb-control-border-hover", palette.controlBorderHover), ("--fb-surface-hover", palette.hover),
            ("--fb-surface-active", palette.active), ("--fb-accent-text", palette.accentText),
            ("--fb-accent-surface", palette.accentSurface), ("--fb-selected-surface", palette.selectedSurface),
            ("--fb-action", palette.action), ("--fb-action-active", palette.actionPressed), ("--fb-on-action", palette.onAction),
            ("--fb-action-border", palette.actionBorder), ("--fb-progress-fill", palette.progressFill),
            ("--fb-progress-track", palette.progressTrack), ("--fb-editor-bg", palette.editor.background),
            ("--fb-editor-keyword", palette.editor.keyword), ("--fb-editor-type", palette.editor.type),
            ("--fb-editor-heading", palette.editor.heading), ("--fb-editor-selection", palette.editor.selection),
        ]
    }

    func testMatchesWebThemeGenerator() throws {
        var mismatches: [String] = []
        for (seed, modes) in try golden() {
            for mode in ["dark", "light"] {
                let expected = try XCTUnwrap(modes[mode])
                let palette = PaletteEngine.seeded(try XCTUnwrap(ColorMath.parse(seed)), dark: mode == "dark")
                for (name, color) in roles(palette) where ColorMath.format(color) != expected[name] {
                    mismatches.append("\(seed) \(mode) \(name): \(ColorMath.format(color)) != \(expected[name] ?? "nil")")
                }
                if palette.brandGradient.map(ColorMath.format) != hexes(expected["--fb-brand-gradient"] ?? "") {
                    mismatches.append("\(seed) \(mode) gradient")
                }
                if [palette.cardBack, palette.cardBackEnd].map(ColorMath.format) != hexes(expected["--fb-dropzone-mark"] ?? "") {
                    mismatches.append("\(seed) \(mode) card back")
                }
                if [palette.cardFront, palette.cardFrontEnd].map(ColorMath.format) != hexes(expected["--fb-dropzone-mark-hover"] ?? "") {
                    mismatches.append("\(seed) \(mode) card front")
                }
            }
        }
        XCTAssertTrue(mismatches.isEmpty, mismatches.joined(separator: "\n"))
    }

    func testTextAndActionsMeetTheContrastContract() throws {
        for seed in try golden().keys {
            for dark in [true, false] {
                let palette = PaletteEngine.seeded(try XCTUnwrap(ColorMath.parse(seed)), dark: dark)
                let surfaces = [palette.background, palette.surface, palette.raised, palette.sunken]
                XCTAssertGreaterThanOrEqual(ColorMath.minimumContrast(palette.text, surfaces), 4.5, "\(seed) text")
                XCTAssertGreaterThanOrEqual(ColorMath.minimumContrast(palette.accentText, surfaces), 4.5, "\(seed) accent")
                XCTAssertGreaterThanOrEqual(ColorMath.contrast(palette.onAction, palette.action), 4.8, "\(seed) action")
                XCTAssertGreaterThanOrEqual(ColorMath.minimumContrast(palette.controlBorder, surfaces), 3.0, "\(seed) control")
            }
        }
    }

    func testRoundTripsThroughOklch() {
        for rgb in [0x8b35ff, 0x000000, 0xffffff, 0x808080, 0x12ab9f] {
            XCTAssertEqual(rgb, ColorMath.rgb(ColorMath.oklch(rgb)))
        }
    }

    func testEveryPresetAndModeBuilds() {
        for preset in ThemePreset.allCases {
            for dark in [true, false] {
                let palette = PaletteEngine.preset(preset, dark: dark)
                XCTAssertEqual(palette.brandGradient.count, 5)
                XCTAssertEqual(palette.dark, dark)
            }
        }
    }
}
