package io.filebeam.android.ui.design.color

import androidx.compose.ui.graphics.Color
import kotlin.math.min

/**
 * Builds [FilebeamPalette]s from the same definitions the web app uses
 * (`backend/app/Support/Theming`). Preset tables are copied verbatim and custom colours run
 * the same OKLCH derivation, so a colour chosen here matches the web pixel for pixel.
 */
internal object PaletteEngine {
    const val DEFAULT_PRIMARY = 0x8b35ff

    // Columns: chrome, page, surface, raised, sunken, footer, settings, border, control, control hover, hover, active.
    private val neutralsLight = mapOf(
        ThemePreset.Purple to "f7f7fa f7f7fa fefefe ffffff f2f1f6 f5f5f8 f5f5f8 d3d2d9 76757c 5f5d67 edebf2 e6e4ed",
        ThemePreset.Blue to "f6f7fa f6f7fa fdfefe ffffff f0f2f6 f3f5f8 f3f5f8 d0d3d9 72767d 5a5f68 e9edf3 e2e6ed",
        ThemePreset.Teal to "f5f8f8 f5f8f8 fdfefe ffffff eef3f3 f2f6f5 f2f6f5 cdd5d4 6e7877 556260 e6efed dee8e7",
        ThemePreset.Green to "f5f8f6 f5f8f6 fdfefd ffffff eff3f0 f3f6f4 f3f6f4 cfd5d0 717873 58625b e8eeea e1e8e3",
        ThemePreset.Amber to "f9f7f4 f9f7f4 fefefd ffffff f4f2ee f6f5f2 f6f5f2 d6d3cd 7a756e 645e55 f0ece6 eae5de",
        ThemePreset.Orange to "faf7f5 faf7f5 fefdfd ffffff f6f1ef f7f4f3 f7f4f3 d8d2ce 7c7470 675d57 f2ebe7 ece4df",
        ThemePreset.Rose to "faf6f7 faf6f7 fefdfe ffffff f6f0f1 f8f4f4 f8f4f4 d9d1d2 7d7374 685b5d f2eaeb ede3e4",
    )
    private val neutralsDark = mapOf(
        ThemePreset.Purple to "0b0914 100e16 191621 211d2a 120f19 14111a 15121c 3b3447 84778f a296ae 2a2435 322a3f",
        ThemePreset.Blue to "080b10 0c1015 15181e 1c2025 0e1116 101318 111419 333842 777d88 959ca7 24282e 2b2f35",
        ThemePreset.Teal to "060d0c 091110 121a19 192120 0b1312 0d1514 0e1615 2e3b39 72817f 90a09d 212a28 28312f",
        ThemePreset.Green to "070c09 0b110d 141a16 1b211d 0d130f 0f1410 101612 313b34 758078 949f97 232925 2a302c",
        ThemePreset.Amber to "0e0a06 120f0a 1b1812 231f19 14100b 16120d 17130e 3d372e 837c71 a29b90 2b2721 322e28",
        ThemePreset.Orange to "0f0907 140e0b 1d1713 241e1b 15100c 17110e 19120f 403630 867a74 a69992 2d2622 342d29",
        ThemePreset.Rose to "0f090a 140d0e 1d1617 251d1e 160f10 181112 191213 413436 87797b a6979a 2d2526 342c2d",
    )

    // Columns: action, top, hover, active, on-action, light ink, dark ink, light border, dark border, dark progress.
    private val accents = mapOf(
        ThemePreset.Purple to "7c3aed 8545e6 6d28d9 5b21b6 ffffff 6d28d9 c4a6ee 6d28d9 ad83e8 b18be7",
        ThemePreset.Blue to "2864c5 3570ce 2257ac 1c4791 ffffff 235bac a0c2f4 2257ac 80a8e5 8cb3ed",
        ThemePreset.Teal to "087c73 0a7e75 086b64 075b55 ffffff 086b64 8cd1c6 086b64 66b7aa 72c1b6",
        ThemePreset.Green to "267a48 2c7f4e 21693e 1c5935 ffffff 21693e a1d0ab 21693e 78ad87 85c495",
        ThemePreset.Amber to "e4b34f e9bd64 dda53a d0992b 33250c 805515 e6c17c 956719 b88a35 dfb765",
        ThemePreset.Orange to "f09a60 f3a674 e98c4b de7e3a 362014 964618 efb389 af5828 ce8b5a e5a471",
        ThemePreset.Rose to "b83d5b bf4765 a33350 8d2b44 ffffff a33350 ecaabc a33350 d8869a dd91a6",
    )

    // Columns: light accent, light selected, dark accent, dark selected.
    private val tints = mapOf(
        ThemePreset.Purple to "f5f2ff eeeaff 241c38 2e214a",
        ThemePreset.Blue to "eff5fc e5eefb 1a2331 1e2b40",
        ThemePreset.Teal to "eff6f5 e5f0ef 162624 18302d",
        ThemePreset.Green to "eff6f1 e6f0e8 19261d 1d2f23",
        ThemePreset.Amber to "fbf7f0 f9f2e7 2d2618 3c301c",
        ThemePreset.Orange to "fcf4f0 fbeee7 302219 402a1d",
        ThemePreset.Rose to "fcf2f4 fbeaed 301e21 3e2429",
    )

    private val lightEffects = mapOf(
        ThemePreset.Purple to (0x8b5cf6 to 0xc66bd4),
        ThemePreset.Blue to (0x4d87d8 to 0x51a4be),
        ThemePreset.Teal to (0x309f94 to 0x70b98d),
        ThemePreset.Green to (0x61a76f to 0x93b660),
        ThemePreset.Amber to (0xd7a443 to 0xd48b5a),
        ThemePreset.Orange to (0xe59a66 to 0xd77d93),
        ThemePreset.Rose to (0xd882a0 to 0xb88acd),
    )

    private fun row(value: String) = value.split(' ').map { it.toInt(16) }

    /** Structural and accent roles before fixed, editor and decorative roles are added. */
    internal class Roles(
        val neutrals: List<Int>,
        val accentText: Int,
        val accentSurface: Int,
        val selectedSurface: Int,
        val action: Int,
        val actionTop: Int,
        val actionHover: Int,
        val actionActive: Int,
        val onAction: Int,
        val actionBorder: Int,
        val choiceBorder: Int,
        val progressFill: Int,
    ) {
        val chrome get() = neutrals[0]
        val background get() = neutrals[1]
        val surface get() = neutrals[2]
        val raised get() = neutrals[3]
        val sunken get() = neutrals[4]
        val settings get() = neutrals[6]
        val border get() = neutrals[7]
        val controlBorder get() = neutrals[8]
        val controlBorderHover get() = neutrals[9]
        val hover get() = neutrals[10]
        val active get() = neutrals[11]
    }

    internal fun authored(preset: ThemePreset, dark: Boolean): Roles {
        val accent = row(accents.getValue(preset))
        val tint = row(tints.getValue(preset))
        return Roles(
            neutrals = row((if (dark) neutralsDark else neutralsLight).getValue(preset)),
            accentText = if (dark) accent[6] else accent[5],
            accentSurface = tint[if (dark) 2 else 0],
            selectedSurface = tint[if (dark) 3 else 1],
            action = accent[0],
            actionTop = accent[1],
            actionHover = accent[2],
            actionActive = accent[3],
            onAction = accent[4],
            actionBorder = if (dark) accent[8] else accent[7],
            choiceBorder = if (dark) accent[8] else accent[5],
            progressFill = if (dark) accent[9] else accent[5],
        )
    }

    /** OKLCH targets for light custom neutrals, in neutral column order. */
    private val customLightTargets = listOf(
        0.977 to 0.004, 0.977 to 0.004, 0.996 to 0.001, 1.0 to 0.0, 0.961 to 0.006, 0.970 to 0.004,
        0.970 to 0.004, 0.867 to 0.009, 0.565 to 0.012, 0.485 to 0.016, 0.944 to 0.009, 0.924 to 0.011,
    )
    private val borderColumns = setOf(7, 8, 9)

    internal class Custom(seed: Int) {
        private val source = ColorMath.oklch(seed)
        val chroma = if (source.c < 0.01) 0.0 else source.c
        val hue = if (source.c < 0.01) 0.0 else source.h
        val anchor = ColorMath.rgb(0.65, min(chroma, 0.14), hue)
        val endpoint = ColorMath.rgb(0.72, min(chroma, 0.15), hue)
    }

    internal fun custom(seed: Int, dark: Boolean): Roles {
        val custom = Custom(seed)
        val chroma = custom.chroma
        val hue = custom.hue
        val light = !dark
        val neutrals = row(neutralsDark.getValue(ThemePreset.Purple)).mapIndexed { index, sourceColor ->
            val border = index in borderColumns
            if (light) {
                val (l, cap) = customLightTargets[index]
                ColorMath.rgb(l, min(cap, chroma * 0.15), hue)
            } else {
                val from = ColorMath.oklch(sourceColor)
                ColorMath.rgb(from.l, minOf(from.c, if (border) 0.018 else 0.012, chroma * (if (border) 0.20 else 0.15)), hue)
            }
        }.toMutableList()
        val surface = neutrals[2]
        val accentSurface = ColorMath.mix(custom.anchor, surface, if (light) 0.07 else 0.12)
        val selectedSurface = ColorMath.mix(custom.anchor, surface, if (light) 0.12 else 0.21)
        val backgrounds = neutrals.filterIndexed { index, _ -> index !in borderColumns } + accentSurface + selectedSurface
        val accentText = requireNotNull(
            ColorMath.against(Oklch(if (light) 0.43 else 0.79, min(chroma, if (light) 0.11 else 0.10), hue), backgrounds, 4.8, light),
        ) { "No legible accent for this colour" }
        for (index in listOf(8, 9)) {
            neutrals[index] = requireNotNull(ColorMath.against(ColorMath.oklch(neutrals[index]), backgrounds, 3.0, light))
        }
        val warm = hue in 45.0..105.0 && chroma >= 0.04
        val ink = if (warm) 0x2b2318 else 0xffffff
        val levels = if (warm) listOf(0.78, 0.80, 0.75, 0.72) else listOf(0.50, 0.525, 0.46, 0.42)
        val fillChroma = min(chroma, if (warm) 0.14 else 0.19)
        var fills = emptyList<Int>()
        for (step in 0..1000) {
            val shift = (if (warm) 1 else -1) * step * 0.001
            check(levels.min() + shift >= 0 && levels.max() + shift <= 1) { "No legible action colour for this seed" }
            fills = levels.map { ColorMath.rgb(it + shift, fillChroma, hue) }
            if (ColorMath.minimumContrast(ink, fills) >= 4.8) break
        }
        val border = if (light) accentText else requireNotNull(
            ColorMath.against(Oklch(0.67, min(chroma, 0.09), hue), backgrounds, 3.0, false),
        )
        val track = if (light) neutrals[11] else neutrals[7]
        val progress = if (light) accentText else requireNotNull(
            ColorMath.against(Oklch(0.74, min(chroma, 0.10), hue), listOf(track), 3.5, false),
        )
        return Roles(
            neutrals = neutrals,
            accentText = accentText,
            accentSurface = accentSurface,
            selectedSurface = selectedSurface,
            action = fills[0],
            actionTop = fills[1],
            actionHover = fills[2],
            actionActive = fills[3],
            onAction = ink,
            actionBorder = border,
            choiceBorder = border,
            progressFill = progress,
        )
    }

    private class Fixed(dark: Boolean) {
        val text = row(if (dark) "f4f1fa b5abbf ada3b7 443a50 2b2434 a89cb5" else "262230 5e576a 62596d d8d2e0 ece8f1 777080")
        val status = row(
            if (dark) "a1d0ab 1d3025 79b688 e6c17c 352b1b c09d58 f0a7b9 3b242d ca7d91 a0c2f4 1e2c42 789ecd"
            else "21693e edf7f0 34824e 805515 fbf4e5 986b26 b4233f fceef0 c34359 235bac edf3fc 4675b8",
        )

        // bg, gutter bg, gutter fg, gutter active bg, gutter active fg, border, selection, selection inactive, matching, search, caret
        val editorSurface = row(
            if (dark) "120f19 191621 ada6b8 30263f eceaf2 3b3447 332a46 282233 30263f 3a3021 eceaf2"
            else "fcfbfe f3f1f7 5c606d eee9f6 292532 d7d2e0 ece6f7 eeebf4 eee9f6 f7edd6 292532",
        )

        // fg keyword string number constant function property type tag attribute comment punctuation operator meta regexp escape link heading invalid
        val syntax = row(
            if (dark) "eceaf2 cbacf5 91ceac e7bc87 f3aec2 9abff5 89c8d3 d6b8f0 f0a7b9 e8c095 ada6b8 aba5b7 beb7cc dcc088 f3abc0 edc49a 9abff5 ceaeff f5a3b5"
            else "292532 783ab0 176b45 925016 aa355a 245fa5 086976 744a97 a73558 8a4b17 5c606d 565c6a 565c6a 765a17 a03652 964712 245fa5 6e38b0 b4233f",
        )
        val activeLine = if (dark) 0xffffff else 0x242033
        val activeLineAlpha = 0x05 / 255.0
    }

    private fun editor(primary: Int, dark: Boolean, roles: Roles, fixed: Fixed): EditorColors {
        val surface = fixed.editorSurface.toMutableList()
        val syntax = fixed.syntax.toMutableList()
        if (primary != DEFAULT_PRIMARY) {
            val light = !dark
            val bg = if (light) ColorMath.mix(roles.sunken, 0xffffff, 0.18) else roles.sunken
            val gutter = if (light) ColorMath.mix(roles.sunken, bg, 0.75) else roles.surface
            val selected = roles.selectedSurface
            val matching = ColorMath.mix(selected, if (light) bg else gutter, 0.75)
            val inactive = ColorMath.mix(selected, bg, 0.6)
            surface[0] = bg
            surface[1] = gutter
            surface[3] = matching
            surface[5] = roles.border
            surface[6] = selected
            surface[7] = inactive
            surface[8] = matching
            val plain = listOf(bg, selected, inactive, matching, surface[9])
            val backgrounds = plain + plain.map { ColorMath.composite(fixed.activeLine, fixed.activeLineAlpha, it) }
            val seed = ColorMath.oklch(primary)
            val anchorHue = ColorMath.oklch(DEFAULT_PRIMARY).h
            // keyword, type and heading follow the accent hue; every other syntax colour stays put.
            for (index in listOf(1, 7, 17)) {
                val base = ColorMath.oklch(syntax[index])
                val c = if (seed.c < 0.01) 0.0 else minOf(base.c, seed.c * 0.8, 0.14)
                val h = if (seed.c < 0.01) 0.0 else (base.h + seed.h - anchorHue + 360).rem(360.0)
                val ink = ColorMath.against(Oklch(base.l, c, h), backgrounds, 4.8, light) ?: syntax[index]
                syntax[index] = ColorMath.against(ColorMath.oklch(ink), listOf(bg), 5.5, light) ?: ink
            }
        }
        return EditorColors(
            background = opaque(surface[0]),
            gutterBackground = opaque(surface[1]),
            gutterText = opaque(surface[2]),
            gutterActiveBackground = opaque(surface[3]),
            gutterActiveText = opaque(surface[4]),
            border = opaque(surface[5]),
            activeLine = translucent(fixed.activeLine, fixed.activeLineAlpha.toFloat()),
            selection = opaque(surface[6]),
            caret = opaque(surface[10]),
            text = opaque(syntax[0]),
            keyword = opaque(syntax[1]),
            string = opaque(syntax[2]),
            number = opaque(syntax[3]),
            constant = opaque(syntax[4]),
            function = opaque(syntax[5]),
            property = opaque(syntax[6]),
            type = opaque(syntax[7]),
            tag = opaque(syntax[8]),
            attribute = opaque(syntax[9]),
            comment = opaque(syntax[10]),
            punctuation = opaque(syntax[11]),
            operator = opaque(syntax[12]),
            meta = opaque(syntax[13]),
            regexp = opaque(syntax[14]),
            escape = opaque(syntax[15]),
            link = opaque(syntax[16]),
            heading = opaque(syntax[17]),
            invalid = opaque(syntax[18]),
        )
    }

    private class Effects(val glowA: Color, val glowB: Color, val cardBack: Int, val cardBackEnd: Int, val cardFront: Int, val cardFrontEnd: Int)

    private fun effects(primary: Int, dark: Boolean, roles: Roles): Effects {
        if (dark) {
            val seed = ColorMath.oklch(primary)
            val anchor = ColorMath.oklch(DEFAULT_PRIMARY)
            fun shift(source: Int): Int {
                if (primary == DEFAULT_PRIMARY) return source
                val from = ColorMath.oklch(source)
                return ColorMath.rgb(
                    from.l,
                    if (seed.c < 0.01) 0.0 else from.c * min(1.4, seed.c / anchor.c),
                    if (seed.c < 0.01) 0.0 else (from.h + seed.h - anchor.h + 360).rem(360.0),
                )
            }
            return Effects(
                glowA = translucent(shift(0x573182), 0x1f / 255f),
                glowB = translucent(shift(0x342043), 0x13 / 255f),
                cardBack = shift(0x2e2639),
                cardBackEnd = shift(0x211b2b),
                cardFront = shift(0x3d2c4c),
                cardFrontEnd = shift(0x2c2138),
            )
        }
        val preset = ThemePreset.fromSeed(primary)
        val a: Int
        val b: Int
        val strong: Boolean
        if (preset != null) {
            a = lightEffects.getValue(preset).first
            b = lightEffects.getValue(preset).second
            strong = preset == ThemePreset.Amber || preset == ThemePreset.Orange
        } else {
            val seed = ColorMath.oklch(primary)
            val grey = seed.c < 0.01
            a = ColorMath.rgb(0.67, if (grey) 0.0 else min(seed.c, 0.13), if (grey) 0.0 else seed.h)
            b = ColorMath.rgb(0.73, if (grey) 0.0 else min(seed.c, 0.10), if (grey) 0.0 else seed.h)
            strong = seed.c >= 0.04 && seed.h in 45.0..105.0
        }
        return Effects(
            glowA = translucent(a, if (strong) 0.22f else 0.16f),
            glowB = translucent(b, if (strong) 0.10f else 0.08f),
            cardBack = ColorMath.mix(a, roles.surface, 0.04),
            cardBackEnd = ColorMath.mix(a, roles.surface, 0.12),
            cardFront = ColorMath.mix(a, roles.surface, 0.09),
            cardFrontEnd = ColorMath.mix(b, roles.surface, 0.19),
        )
    }

    private fun brandGradient(seed: Int, endpoint: Int, dark: Boolean, default: Boolean): List<Color> {
        val tip = when {
            default && dark -> 0xc75bfa
            dark -> endpoint
            else -> ColorMath.mix(endpoint, 0x000000, 0.90)
        }
        val start = ColorMath.mix(seed, 0x000000, 0.82)
        return listOf(start, ColorMath.mix(start, seed, 0.5), seed, ColorMath.mix(seed, tip, 0.5), tip).map(::opaque)
    }

    fun preset(preset: ThemePreset, dark: Boolean): FilebeamPalette =
        assemble(preset.seed, preset.seed, preset.endpoint, dark, authored(preset, dark), preset == ThemePreset.Purple)

    /** Any sRGB seed. A seed equal to a preset's seed yields that authored preset, as on the web. */
    fun seeded(seed: Int, dark: Boolean): FilebeamPalette {
        val rgb = seed and 0xFFFFFF
        ThemePreset.fromSeed(rgb)?.let { return preset(it, dark) }
        val custom = Custom(rgb)
        return assemble(rgb, custom.anchor, custom.endpoint, dark, custom(rgb, dark), false)
    }

    /** Material You roles handed over from the platform's dynamic colour scheme. */
    class DynamicRoles(
        val surface: Int,
        val lowest: Int,
        val low: Int,
        val container: Int,
        val high: Int,
        val highest: Int,
        val onSurface: Int,
        val onSurfaceVariant: Int,
        val outline: Int,
        val outlineVariant: Int,
        val primary: Int,
        val onPrimary: Int,
        val primaryContainer: Int,
        val tertiary: Int,
        val error: Int,
        val errorContainer: Int,
    )

    /**
     * Maps wallpaper-derived Material roles onto Filebeam roles. Decorative and editor colours
     * run the same hue-following recipes as a custom colour, seeded by the dynamic primary.
     */
    fun dynamic(r: DynamicRoles, dark: Boolean): FilebeamPalette {
        val roles = Roles(
            neutrals = listOf(r.surface, r.surface, r.low, r.container, r.lowest, r.low, r.low, r.outlineVariant, r.outline, r.onSurfaceVariant, r.high, r.highest),
            accentText = r.primary,
            accentSurface = ColorMath.mix(r.primaryContainer, r.low, 0.35),
            selectedSurface = ColorMath.mix(r.primaryContainer, r.low, 0.6),
            action = r.primary,
            actionTop = ColorMath.mix(r.primary, 0xffffff, 0.92),
            actionHover = ColorMath.mix(r.primary, 0x000000, 0.94),
            actionActive = ColorMath.mix(r.primary, 0x000000, 0.88),
            onAction = r.onPrimary,
            actionBorder = if (dark) ColorMath.mix(r.primary, 0xffffff, 0.8) else r.primary,
            choiceBorder = r.primary,
            progressFill = r.primary,
        )
        val base = assemble(r.primary, r.primary, r.tertiary, dark, roles, false)
        return base.copy(
            text = opaque(r.onSurface),
            textMuted = opaque(r.onSurfaceVariant),
            textSubtle = opaque(r.onSurfaceVariant),
            progressTrack = opaque(r.highest),
            disabledBorder = opaque(ColorMath.mix(r.onSurface, r.low, 0.14)),
            disabledSurface = opaque(ColorMath.mix(r.onSurface, r.low, 0.08)),
            disabledText = opaque(ColorMath.mix(r.onSurface, r.low, 0.42)),
            danger = StatusColors(opaque(r.error), opaque(ColorMath.mix(r.errorContainer, r.low, 0.5)), opaque(r.error)),
        )
    }

    private fun assemble(primary: Int, gradientSeed: Int, endpoint: Int, dark: Boolean, roles: Roles, default: Boolean): FilebeamPalette {
        val fixed = Fixed(dark)
        val effects = effects(primary, dark, roles)
        val ink = if (dark) 0xffffff else 0x242033
        fun status(offset: Int) = StatusColors(opaque(fixed.status[offset]), opaque(fixed.status[offset + 1]), opaque(fixed.status[offset + 2]))
        return FilebeamPalette(
            dark = dark,
            chrome = opaque(roles.chrome),
            background = opaque(roles.background),
            surface = opaque(roles.surface),
            raised = opaque(roles.raised),
            sunken = opaque(roles.sunken),
            border = opaque(roles.border),
            controlBorder = opaque(roles.controlBorder),
            controlBorderHover = opaque(roles.controlBorderHover),
            hover = opaque(roles.hover),
            active = opaque(roles.active),
            text = opaque(fixed.text[0]),
            textMuted = opaque(fixed.text[1]),
            textSubtle = opaque(fixed.text[2]),
            disabledBorder = opaque(fixed.text[3]),
            disabledSurface = opaque(fixed.text[4]),
            disabledText = opaque(fixed.text[5]),
            accentText = opaque(roles.accentText),
            accentSurface = opaque(roles.accentSurface),
            selectedSurface = opaque(roles.selectedSurface),
            action = opaque(roles.action),
            actionTop = opaque(roles.actionTop),
            actionPressed = opaque(roles.actionActive),
            onAction = opaque(roles.onAction),
            actionBorder = opaque(roles.actionBorder),
            choiceBorder = opaque(roles.choiceBorder),
            progressFill = opaque(roles.progressFill),
            progressTrack = opaque(if (dark) roles.border else roles.active),
            success = status(0),
            warning = status(3),
            danger = status(6),
            info = status(9),
            scrim = if (dark) translucent(0x08060e, 0.68f) else translucent(0x181420, 0.34f),
            lineSoft = translucent(ink, 0.07f),
            lineStrong = translucent(ink, 0.10f),
            shadow = if (dark) translucent(0x000000, 0.36f) else translucent(0x181420, 0.10f),
            highlight = if (dark) translucent(0xffffff, 0.08f) else Color.Transparent,
            sheen = if (dark) translucent(0xffffff, 0.12f) else Color.Transparent,
            brand = opaque(primary),
            brandGradient = brandGradient(gradientSeed, endpoint, dark, default),
            glowPrimary = effects.glowA,
            glowSecondary = effects.glowB,
            cardBack = opaque(effects.cardBack),
            cardBackEnd = opaque(effects.cardBackEnd),
            cardFront = opaque(effects.cardFront),
            cardFrontEnd = opaque(effects.cardFrontEnd),
            editor = editor(primary, dark, roles, fixed),
        )
    }
}
