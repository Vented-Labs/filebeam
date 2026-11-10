package io.filebeam.android.ui.design.color

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/** Colour presets shared with the web app. [seed] and [endpoint] anchor the brand gradient. */
enum class ThemePreset(val key: String, val seed: Int, val endpoint: Int) {
    Purple("purple", 0x8b35ff, 0xc05ada),
    Blue("blue", 0x3d78d8, 0x4a9dbd),
    Teal("teal", 0x168f85, 0x48a778),
    Green("green", 0x388b59, 0x76a543),
    Amber("amber", 0xcf9b35, 0xd78135),
    Orange("orange", 0xda7b40, 0xd46369),
    Rose("rose", 0xcf5d78, 0xb570c0);

    companion object {
        fun fromKey(key: String?): ThemePreset? = entries.firstOrNull { it.key == key }
        fun fromSeed(seed: Int): ThemePreset? = entries.firstOrNull { it.seed == seed and 0xFFFFFF }
    }
}

@Immutable
data class StatusColors(val ink: Color, val surface: Color, val border: Color)

/** Syntax and surface colours for the note editor and viewer. */
@Immutable
data class EditorColors(
    val background: Color,
    val gutterBackground: Color,
    val gutterText: Color,
    val gutterActiveBackground: Color,
    val gutterActiveText: Color,
    val border: Color,
    val activeLine: Color,
    val selection: Color,
    val caret: Color,
    val text: Color,
    val keyword: Color,
    val string: Color,
    val number: Color,
    val constant: Color,
    val function: Color,
    val property: Color,
    val type: Color,
    val tag: Color,
    val attribute: Color,
    val comment: Color,
    val punctuation: Color,
    val operator: Color,
    val meta: Color,
    val regexp: Color,
    val escape: Color,
    val link: Color,
    val heading: Color,
    val invalid: Color,
)

/**
 * Every colour role the app draws with. Roles mirror the web tokens (`--fb-*`) so a screen
 * looks the same whether the palette came from a preset, a custom colour or Material You.
 */
@Immutable
data class FilebeamPalette(
    val dark: Boolean,
    val chrome: Color,
    val background: Color,
    val surface: Color,
    val raised: Color,
    val sunken: Color,
    val border: Color,
    val controlBorder: Color,
    val controlBorderHover: Color,
    val hover: Color,
    val active: Color,
    val text: Color,
    val textMuted: Color,
    val textSubtle: Color,
    val disabledBorder: Color,
    val disabledSurface: Color,
    val disabledText: Color,
    val accentText: Color,
    val accentSurface: Color,
    val selectedSurface: Color,
    val action: Color,
    val actionTop: Color,
    val actionPressed: Color,
    val onAction: Color,
    val actionBorder: Color,
    val choiceBorder: Color,
    val progressFill: Color,
    val progressTrack: Color,
    val success: StatusColors,
    val warning: StatusColors,
    val danger: StatusColors,
    val info: StatusColors,
    val scrim: Color,
    val lineSoft: Color,
    val lineStrong: Color,
    val shadow: Color,
    val highlight: Color,
    val sheen: Color,
    val brand: Color,
    val brandGradient: List<Color>,
    val glowPrimary: Color,
    val glowSecondary: Color,
    val cardBack: Color,
    val cardBackEnd: Color,
    val cardFront: Color,
    val cardFrontEnd: Color,
    val editor: EditorColors,
) {
    /** Dark themes use a subtle top-lit gradient on primary actions; light themes stay flat. */
    val actionGradient: List<Color> get() = if (dark) listOf(actionTop, action) else listOf(action, action)

    companion object {
        /** Stops for [brandGradient], matching the web's 135 degree mark gradient. */
        val BrandStops = floatArrayOf(0f, 0.275f, 0.55f, 0.775f, 1f)
    }
}

internal fun opaque(rgb: Int) = Color(0xFF000000.toInt() or (rgb and 0xFFFFFF))
internal fun translucent(rgb: Int, alpha: Float) = opaque(rgb).copy(alpha = alpha)
