import Foundation

/// Builds palettes from the same definitions the web app uses (`backend/app/Support/Theming`).
/// Preset tables are copied verbatim and custom colours run the same OKLCH derivation, so a
/// colour chosen here matches the web pixel for pixel.
public enum PaletteEngine {
    public static let defaultPrimary: RGB = 0x8b35ff

    // Columns: chrome, page, surface, raised, sunken, footer, settings, border, control, control hover, hover, active.
    private static let neutralsLight: [ThemePreset: String] = [
        .purple: "f7f7fa f7f7fa fefefe ffffff f2f1f6 f5f5f8 f5f5f8 d3d2d9 76757c 5f5d67 edebf2 e6e4ed",
        .blue: "f6f7fa f6f7fa fdfefe ffffff f0f2f6 f3f5f8 f3f5f8 d0d3d9 72767d 5a5f68 e9edf3 e2e6ed",
        .teal: "f5f8f8 f5f8f8 fdfefe ffffff eef3f3 f2f6f5 f2f6f5 cdd5d4 6e7877 556260 e6efed dee8e7",
        .green: "f5f8f6 f5f8f6 fdfefd ffffff eff3f0 f3f6f4 f3f6f4 cfd5d0 717873 58625b e8eeea e1e8e3",
        .amber: "f9f7f4 f9f7f4 fefefd ffffff f4f2ee f6f5f2 f6f5f2 d6d3cd 7a756e 645e55 f0ece6 eae5de",
        .orange: "faf7f5 faf7f5 fefdfd ffffff f6f1ef f7f4f3 f7f4f3 d8d2ce 7c7470 675d57 f2ebe7 ece4df",
        .rose: "faf6f7 faf6f7 fefdfe ffffff f6f0f1 f8f4f4 f8f4f4 d9d1d2 7d7374 685b5d f2eaeb ede3e4",
    ]
    private static let neutralsDark: [ThemePreset: String] = [
        .purple: "0b0914 100e16 191621 211d2a 120f19 14111a 15121c 3b3447 84778f a296ae 2a2435 322a3f",
        .blue: "080b10 0c1015 15181e 1c2025 0e1116 101318 111419 333842 777d88 959ca7 24282e 2b2f35",
        .teal: "060d0c 091110 121a19 192120 0b1312 0d1514 0e1615 2e3b39 72817f 90a09d 212a28 28312f",
        .green: "070c09 0b110d 141a16 1b211d 0d130f 0f1410 101612 313b34 758078 949f97 232925 2a302c",
        .amber: "0e0a06 120f0a 1b1812 231f19 14100b 16120d 17130e 3d372e 837c71 a29b90 2b2721 322e28",
        .orange: "0f0907 140e0b 1d1713 241e1b 15100c 17110e 19120f 403630 867a74 a69992 2d2622 342d29",
        .rose: "0f090a 140d0e 1d1617 251d1e 160f10 181112 191213 413436 87797b a6979a 2d2526 342c2d",
    ]
    // Columns: action, top, hover, active, on-action, light ink, dark ink, light border, dark border, dark progress.
    private static let accents: [ThemePreset: String] = [
        .purple: "7c3aed 8545e6 6d28d9 5b21b6 ffffff 6d28d9 c4a6ee 6d28d9 ad83e8 b18be7",
        .blue: "2864c5 3570ce 2257ac 1c4791 ffffff 235bac a0c2f4 2257ac 80a8e5 8cb3ed",
        .teal: "087c73 0a7e75 086b64 075b55 ffffff 086b64 8cd1c6 086b64 66b7aa 72c1b6",
        .green: "267a48 2c7f4e 21693e 1c5935 ffffff 21693e a1d0ab 21693e 78ad87 85c495",
        .amber: "e4b34f e9bd64 dda53a d0992b 33250c 805515 e6c17c 956719 b88a35 dfb765",
        .orange: "f09a60 f3a674 e98c4b de7e3a 362014 964618 efb389 af5828 ce8b5a e5a471",
        .rose: "b83d5b bf4765 a33350 8d2b44 ffffff a33350 ecaabc a33350 d8869a dd91a6",
    ]
    // Columns: light accent, light selected, dark accent, dark selected.
    private static let tints: [ThemePreset: String] = [
        .purple: "f5f2ff eeeaff 241c38 2e214a",
        .blue: "eff5fc e5eefb 1a2331 1e2b40",
        .teal: "eff6f5 e5f0ef 162624 18302d",
        .green: "eff6f1 e6f0e8 19261d 1d2f23",
        .amber: "fbf7f0 f9f2e7 2d2618 3c301c",
        .orange: "fcf4f0 fbeee7 302219 402a1d",
        .rose: "fcf2f4 fbeaed 301e21 3e2429",
    ]
    private static let lightEffects: [ThemePreset: (RGB, RGB)] = [
        .purple: (0x8b5cf6, 0xc66bd4), .blue: (0x4d87d8, 0x51a4be), .teal: (0x309f94, 0x70b98d), .green: (0x61a76f, 0x93b660),
        .amber: (0xd7a443, 0xd48b5a), .orange: (0xe59a66, 0xd77d93), .rose: (0xd882a0, 0xb88acd),
    ]

    private static func row(_ value: String) -> [RGB] { value.split(separator: " ").map { Int($0, radix: 16)! } }

    /// Structural and accent roles before fixed, editor and decorative roles are added.
    struct Roles {
        var neutrals: [RGB]
        var accentText, accentSurface, selectedSurface: RGB
        var action, actionTop, actionHover, actionActive, onAction, actionBorder, choiceBorder, progressFill: RGB

        var chrome: RGB { neutrals[0] }
        var background: RGB { neutrals[1] }
        var surface: RGB { neutrals[2] }
        var raised: RGB { neutrals[3] }
        var sunken: RGB { neutrals[4] }
        var border: RGB { neutrals[7] }
        var controlBorder: RGB { neutrals[8] }
        var controlBorderHover: RGB { neutrals[9] }
        var hover: RGB { neutrals[10] }
        var active: RGB { neutrals[11] }
    }

    static func authored(_ preset: ThemePreset, dark: Bool) -> Roles {
        let accent = row(accents[preset]!)
        let tint = row(tints[preset]!)
        return Roles(
            neutrals: row((dark ? neutralsDark : neutralsLight)[preset]!),
            accentText: dark ? accent[6] : accent[5],
            accentSurface: tint[dark ? 2 : 0],
            selectedSurface: tint[dark ? 3 : 1],
            action: accent[0], actionTop: accent[1], actionHover: accent[2], actionActive: accent[3], onAction: accent[4],
            actionBorder: dark ? accent[8] : accent[7],
            choiceBorder: dark ? accent[8] : accent[5],
            progressFill: dark ? accent[9] : accent[5]
        )
    }

    /// OKLCH targets for light custom neutrals, in neutral column order.
    private static let customLightTargets: [(Double, Double)] = [
        (0.977, 0.004), (0.977, 0.004), (0.996, 0.001), (1.0, 0.0), (0.961, 0.006), (0.970, 0.004),
        (0.970, 0.004), (0.867, 0.009), (0.565, 0.012), (0.485, 0.016), (0.944, 0.009), (0.924, 0.011),
    ]
    private static let borderColumns: Set<Int> = [7, 8, 9]

    struct Custom {
        let chroma: Double
        let hue: Double
        let anchor: RGB
        let endpoint: RGB
        init(_ seed: RGB) {
            let source = ColorMath.oklch(seed)
            chroma = source.c < 0.01 ? 0 : source.c
            hue = source.c < 0.01 ? 0 : source.h
            anchor = ColorMath.rgb(0.65, min(chroma, 0.14), hue)
            endpoint = ColorMath.rgb(0.72, min(chroma, 0.15), hue)
        }
    }

    static func custom(_ seed: RGB, dark: Bool) -> Roles {
        let custom = Custom(seed)
        let chroma = custom.chroma, hue = custom.hue
        let light = !dark
        var neutrals = row(neutralsDark[.purple]!).enumerated().map { index, sourceColor -> RGB in
            let border = borderColumns.contains(index)
            if light {
                let (l, cap) = customLightTargets[index]
                return ColorMath.rgb(l, min(cap, chroma * 0.15), hue)
            }
            let from = ColorMath.oklch(sourceColor)
            return ColorMath.rgb(from.l, min(from.c, border ? 0.018 : 0.012, chroma * (border ? 0.20 : 0.15)), hue)
        }
        let surface = neutrals[2]
        let accentSurface = ColorMath.mix(custom.anchor, surface, light ? 0.07 : 0.12)
        let selectedSurface = ColorMath.mix(custom.anchor, surface, light ? 0.12 : 0.21)
        let backgrounds = neutrals.enumerated().filter { !borderColumns.contains($0.offset) }.map(\.element) + [accentSurface, selectedSurface]
        guard let accentText = ColorMath.against(Oklch(l: light ? 0.43 : 0.79, c: min(chroma, light ? 0.11 : 0.10), h: hue), backgrounds, 4.8, darker: light) else {
            preconditionFailure("No legible accent for this colour")
        }
        for index in [8, 9] {
            neutrals[index] = ColorMath.against(ColorMath.oklch(neutrals[index]), backgrounds, 3.0, darker: light)!
        }
        let warm = (45.0...105.0).contains(hue) && chroma >= 0.04
        let ink = warm ? 0x2b2318 : 0xffffff
        let levels: [Double] = warm ? [0.78, 0.80, 0.75, 0.72] : [0.50, 0.525, 0.46, 0.42]
        let fillChroma = min(chroma, warm ? 0.14 : 0.19)
        var fills: [RGB] = []
        for step in 0...1000 {
            let shift = (warm ? 1.0 : -1.0) * Double(step) * 0.001
            precondition(levels.min()! + shift >= 0 && levels.max()! + shift <= 1, "No legible action colour for this seed")
            fills = levels.map { ColorMath.rgb($0 + shift, fillChroma, hue) }
            if ColorMath.minimumContrast(ink, fills) >= 4.8 { break }
        }
        let border = light ? accentText : ColorMath.against(Oklch(l: 0.67, c: min(chroma, 0.09), h: hue), backgrounds, 3.0, darker: false)!
        let track = light ? neutrals[11] : neutrals[7]
        let progress = light ? accentText : ColorMath.against(Oklch(l: 0.74, c: min(chroma, 0.10), h: hue), [track], 3.5, darker: false)!
        return Roles(
            neutrals: neutrals, accentText: accentText, accentSurface: accentSurface, selectedSurface: selectedSurface,
            action: fills[0], actionTop: fills[1], actionHover: fills[2], actionActive: fills[3], onAction: ink,
            actionBorder: border, choiceBorder: border, progressFill: progress
        )
    }

    private struct Fixed {
        let text: [RGB]
        let status: [RGB]
        // bg, gutter bg, gutter fg, gutter active bg, gutter active fg, border, selection, selection inactive, matching, search, caret
        let editorSurface: [RGB]
        // fg keyword string number constant function property type tag attribute comment punctuation operator meta regexp escape link heading invalid
        let syntax: [RGB]
        let activeLine: RGB
        let activeLineAlpha: Double = Double(0x05) / 255.0

        init(dark: Bool) {
            text = row(dark ? "f4f1fa b5abbf ada3b7 443a50 2b2434 a89cb5" : "262230 5e576a 62596d d8d2e0 ece8f1 777080")
            status = row(dark
                ? "a1d0ab 1d3025 79b688 e6c17c 352b1b c09d58 f0a7b9 3b242d ca7d91 a0c2f4 1e2c42 789ecd"
                : "21693e edf7f0 34824e 805515 fbf4e5 986b26 b4233f fceef0 c34359 235bac edf3fc 4675b8")
            editorSurface = row(dark
                ? "120f19 191621 ada6b8 30263f eceaf2 3b3447 332a46 282233 30263f 3a3021 eceaf2"
                : "fcfbfe f3f1f7 5c606d eee9f6 292532 d7d2e0 ece6f7 eeebf4 eee9f6 f7edd6 292532")
            syntax = row(dark
                ? "eceaf2 cbacf5 91ceac e7bc87 f3aec2 9abff5 89c8d3 d6b8f0 f0a7b9 e8c095 ada6b8 aba5b7 beb7cc dcc088 f3abc0 edc49a 9abff5 ceaeff f5a3b5"
                : "292532 783ab0 176b45 925016 aa355a 245fa5 086976 744a97 a73558 8a4b17 5c606d 565c6a 565c6a 765a17 a03652 964712 245fa5 6e38b0 b4233f")
            activeLine = dark ? 0xffffff : 0x242033
        }
    }

    private static func editor(primary: RGB, dark: Bool, roles: Roles, fixed: Fixed) -> EditorRoles {
        var surface = fixed.editorSurface
        var syntax = fixed.syntax
        if primary != defaultPrimary {
            let light = !dark
            let bg = light ? ColorMath.mix(roles.sunken, 0xffffff, 0.18) : roles.sunken
            let gutter = light ? ColorMath.mix(roles.sunken, bg, 0.75) : roles.surface
            let selected = roles.selectedSurface
            let matching = ColorMath.mix(selected, light ? bg : gutter, 0.75)
            let inactive = ColorMath.mix(selected, bg, 0.6)
            surface[0] = bg; surface[1] = gutter; surface[3] = matching; surface[5] = roles.border
            surface[6] = selected; surface[7] = inactive; surface[8] = matching
            let plain = [bg, selected, inactive, matching, surface[9]]
            let backgrounds = plain + plain.map { ColorMath.composite(fixed.activeLine, fixed.activeLineAlpha, $0) }
            let seed = ColorMath.oklch(primary)
            let anchorHue = ColorMath.oklch(defaultPrimary).h
            // keyword, type and heading follow the accent hue; every other syntax colour stays put.
            for index in [1, 7, 17] {
                let base = ColorMath.oklch(syntax[index])
                let c = seed.c < 0.01 ? 0 : min(base.c, seed.c * 0.8, 0.14)
                let h = seed.c < 0.01 ? 0 : (base.h + seed.h - anchorHue + 360).truncatingRemainder(dividingBy: 360)
                let ink = ColorMath.against(Oklch(l: base.l, c: c, h: h), backgrounds, 4.8, darker: light) ?? syntax[index]
                syntax[index] = ColorMath.against(ColorMath.oklch(ink), [bg], 5.5, darker: light) ?? ink
            }
        }
        return EditorRoles(
            background: surface[0], gutterBackground: surface[1], gutterText: surface[2], gutterActiveBackground: surface[3],
            gutterActiveText: surface[4], border: surface[5], activeLine: RGBA(fixed.activeLine, alpha: fixed.activeLineAlpha),
            selection: surface[6], caret: surface[10], text: syntax[0], keyword: syntax[1], string: syntax[2], number: syntax[3],
            constant: syntax[4], function: syntax[5], property: syntax[6], type: syntax[7], tag: syntax[8], attribute: syntax[9],
            comment: syntax[10], punctuation: syntax[11], operator: syntax[12], meta: syntax[13], regexp: syntax[14], escape: syntax[15],
            link: syntax[16], heading: syntax[17], invalid: syntax[18]
        )
    }

    private struct Effects { let glowA, glowB: RGBA; let cardBack, cardBackEnd, cardFront, cardFrontEnd: RGB }

    private static func effects(primary: RGB, dark: Bool, roles: Roles) -> Effects {
        if dark {
            let seed = ColorMath.oklch(primary)
            let anchor = ColorMath.oklch(defaultPrimary)
            func shift(_ source: RGB) -> RGB {
                if primary == defaultPrimary { return source }
                let from = ColorMath.oklch(source)
                return ColorMath.rgb(
                    from.l,
                    seed.c < 0.01 ? 0 : from.c * min(1.4, seed.c / anchor.c),
                    seed.c < 0.01 ? 0 : (from.h + seed.h - anchor.h + 360).truncatingRemainder(dividingBy: 360)
                )
            }
            return Effects(
                glowA: RGBA(shift(0x573182), alpha: Double(0x1f) / 255), glowB: RGBA(shift(0x342043), alpha: Double(0x13) / 255),
                cardBack: shift(0x2e2639), cardBackEnd: shift(0x211b2b), cardFront: shift(0x3d2c4c), cardFrontEnd: shift(0x2c2138)
            )
        }
        let a: RGB, b: RGB, strong: Bool
        if let preset = ThemePreset.matching(seed: primary) {
            (a, b) = lightEffects[preset]!
            strong = preset == .amber || preset == .orange
        } else {
            let seed = ColorMath.oklch(primary)
            let grey = seed.c < 0.01
            a = ColorMath.rgb(0.67, grey ? 0 : min(seed.c, 0.13), grey ? 0 : seed.h)
            b = ColorMath.rgb(0.73, grey ? 0 : min(seed.c, 0.10), grey ? 0 : seed.h)
            strong = seed.c >= 0.04 && (45.0...105.0).contains(seed.h)
        }
        return Effects(
            glowA: RGBA(a, alpha: strong ? 0.22 : 0.16), glowB: RGBA(b, alpha: strong ? 0.10 : 0.08),
            cardBack: ColorMath.mix(a, roles.surface, 0.04), cardBackEnd: ColorMath.mix(a, roles.surface, 0.12),
            cardFront: ColorMath.mix(a, roles.surface, 0.09), cardFrontEnd: ColorMath.mix(b, roles.surface, 0.19)
        )
    }

    private static func brandGradient(seed: RGB, endpoint: RGB, dark: Bool, isDefault: Bool) -> [RGB] {
        let tip: RGB = isDefault && dark ? 0xc75bfa : dark ? endpoint : ColorMath.mix(endpoint, 0x000000, 0.90)
        let start = ColorMath.mix(seed, 0x000000, 0.82)
        return [start, ColorMath.mix(start, seed, 0.5), seed, ColorMath.mix(seed, tip, 0.5), tip]
    }

    public static func preset(_ preset: ThemePreset, dark: Bool) -> Palette {
        assemble(primary: preset.seed, gradientSeed: preset.seed, endpoint: preset.endpoint, dark: dark, roles: authored(preset, dark: dark), isDefault: preset == .purple)
    }

    /// Any sRGB seed. A seed equal to a preset's seed yields that authored preset, as on the web.
    public static func seeded(_ seed: RGB, dark: Bool) -> Palette {
        let rgb = seed & 0xFFFFFF
        if let preset = ThemePreset.matching(seed: rgb) { return self.preset(preset, dark: dark) }
        let custom = Custom(rgb)
        return assemble(primary: rgb, gradientSeed: custom.anchor, endpoint: custom.endpoint, dark: dark, roles: self.custom(rgb, dark: dark), isDefault: false)
    }

    private static func assemble(primary: RGB, gradientSeed: RGB, endpoint: RGB, dark: Bool, roles: Roles, isDefault: Bool) -> Palette {
        let fixed = Fixed(dark: dark)
        // Text and disabled tints are authored for purple. For any other colour they keep their
        // lightness and take the hue of that palette's own neutrals, so no purple cast remains.
        let text: [RGB]
        if isDefault {
            text = fixed.text
        } else {
            let neutral = ColorMath.oklch(roles.background)
            text = fixed.text.map { color in
                let source = ColorMath.oklch(color)
                return ColorMath.rgb(source.l, neutral.c < 0.002 ? 0 : source.c, neutral.h)
            }
        }
        let effects = effects(primary: primary, dark: dark, roles: roles)
        let ink = dark ? 0xffffff : 0x242033
        func status(_ offset: Int) -> StatusRoles { StatusRoles(ink: fixed.status[offset], surface: fixed.status[offset + 1], border: fixed.status[offset + 2]) }
        return Palette(
            dark: dark,
            chrome: roles.chrome, background: roles.background, surface: roles.surface, raised: roles.raised, sunken: roles.sunken,
            border: roles.border, controlBorder: roles.controlBorder, controlBorderHover: roles.controlBorderHover, hover: roles.hover, active: roles.active,
            text: text[0], textMuted: text[1], textSubtle: text[2], disabledBorder: text[3], disabledSurface: text[4], disabledText: text[5],
            accentText: roles.accentText, accentSurface: roles.accentSurface, selectedSurface: roles.selectedSurface,
            action: roles.action, actionTop: roles.actionTop, actionPressed: roles.actionActive, onAction: roles.onAction,
            actionBorder: roles.actionBorder, choiceBorder: roles.choiceBorder, progressFill: roles.progressFill,
            progressTrack: dark ? roles.border : roles.active,
            success: status(0), warning: status(3), danger: status(6), info: status(9),
            scrim: dark ? RGBA(0x08060e, alpha: 0.68) : RGBA(0x181420, alpha: 0.34),
            lineSoft: RGBA(ink, alpha: 0.07), lineStrong: RGBA(ink, alpha: 0.10),
            shadow: dark ? RGBA(0x000000, alpha: 0.36) : RGBA(0x181420, alpha: 0.10),
            highlight: dark ? RGBA(0xffffff, alpha: 0.08) : .clear,
            sheen: dark ? RGBA(0xffffff, alpha: 0.12) : .clear,
            brand: primary,
            brandGradient: brandGradient(seed: gradientSeed, endpoint: endpoint, dark: dark, isDefault: isDefault),
            glowPrimary: effects.glowA, glowSecondary: effects.glowB,
            cardBack: effects.cardBack, cardBackEnd: effects.cardBackEnd, cardFront: effects.cardFront, cardFrontEnd: effects.cardFrontEnd,
            editor: editor(primary: primary, dark: dark, roles: roles, fixed: fixed)
        )
    }
}
