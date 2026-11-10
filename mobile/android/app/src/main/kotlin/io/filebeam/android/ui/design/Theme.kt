package io.filebeam.android.ui.design

import android.content.Context
import android.os.Build
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import io.filebeam.android.ui.design.color.FilebeamPalette
import io.filebeam.android.ui.design.color.PaletteEngine
import io.filebeam.android.ui.design.color.ThemePreset
import io.filebeam.android.ui.motion.LocalReduceMotion
import io.filebeam.android.ui.motion.rememberReduceMotion

enum class ThemeMode(val key: String) {
    System("system"), Light("light"), Dark("dark");

    companion object {
        fun fromKey(key: String?) = entries.firstOrNull { it.key == key } ?: System
    }
}

/** Where the accent and surfaces come from. */
@Immutable
sealed interface ColorSource {
    /** Wallpaper colours from the OS. Only offered where [materialYouAvailable] is true. */
    data object MaterialYou : ColorSource

    /** The colour the connected Filebeam instance is branded with. */
    data object Instance : ColorSource

    data class Preset(val preset: ThemePreset) : ColorSource

    /** A colour picked by the user, packed as 0xRRGGBB. */
    data class Custom(val seed: Int) : ColorSource
}

@Immutable
data class Appearance(val mode: ThemeMode = ThemeMode.System, val source: ColorSource = defaultColorSource()) {
    companion object {
        fun defaultColorSource(): ColorSource = if (materialYouAvailable) ColorSource.MaterialYou else ColorSource.Instance
    }
}

/** Dynamic colour needs Android 12. Older releases always use a Filebeam palette. */
val materialYouAvailable: Boolean get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

val LocalPalette = staticCompositionLocalOf<FilebeamPalette> { error("FilebeamTheme is missing") }
val LocalType = staticCompositionLocalOf { FilebeamType() }

object Fb {
    val palette: FilebeamPalette
        @Composable @ReadOnlyComposable get() = LocalPalette.current
    val type: FilebeamType
        @Composable @ReadOnlyComposable get() = LocalType.current
}

/** Resolves an appearance to concrete colours. [instanceColor] is the connected instance's brand colour. */
fun resolvePalette(context: Context, appearance: Appearance, systemDark: Boolean, instanceColor: Int?): FilebeamPalette {
    val dark = when (appearance.mode) {
        ThemeMode.System -> systemDark
        ThemeMode.Light -> false
        ThemeMode.Dark -> true
    }
    return when (val source = appearance.source) {
        ColorSource.MaterialYou ->
            if (materialYouAvailable) dynamicPalette(context, dark)
            else PaletteEngine.seeded(instanceColor ?: PaletteEngine.DEFAULT_PRIMARY, dark)
        ColorSource.Instance -> PaletteEngine.seeded(instanceColor ?: PaletteEngine.DEFAULT_PRIMARY, dark)
        is ColorSource.Preset -> PaletteEngine.preset(source.preset, dark)
        is ColorSource.Custom -> runCatching { PaletteEngine.seeded(source.seed, dark) }
            .getOrElse { PaletteEngine.preset(ThemePreset.Purple, dark) }
    }
}

private fun dynamicPalette(context: Context, dark: Boolean): FilebeamPalette {
    check(Build.VERSION.SDK_INT >= Build.VERSION_CODES.S)
    val scheme = if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
    fun Color.rgb() = toArgb() and 0xFFFFFF
    return PaletteEngine.dynamic(
        PaletteEngine.DynamicRoles(
            surface = scheme.surface.rgb(),
            lowest = scheme.surfaceContainerLowest.rgb(),
            low = scheme.surfaceContainerLow.rgb(),
            container = scheme.surfaceContainer.rgb(),
            high = scheme.surfaceContainerHigh.rgb(),
            highest = scheme.surfaceContainerHighest.rgb(),
            onSurface = scheme.onSurface.rgb(),
            onSurfaceVariant = scheme.onSurfaceVariant.rgb(),
            outline = scheme.outline.rgb(),
            outlineVariant = scheme.outlineVariant.rgb(),
            primary = scheme.primary.rgb(),
            onPrimary = scheme.onPrimary.rgb(),
            primaryContainer = scheme.primaryContainer.rgb(),
            tertiary = scheme.tertiary.rgb(),
            error = scheme.error.rgb(),
            errorContainer = scheme.errorContainer.rgb(),
        ),
        dark,
    )
}

/** Material components that the app still uses (sheets, menus, text fields) read these roles. */
internal fun FilebeamPalette.toColorScheme(): ColorScheme {
    val base = if (dark) darkColorScheme() else lightColorScheme()
    return base.copy(
        primary = action,
        onPrimary = onAction,
        primaryContainer = selectedSurface,
        onPrimaryContainer = accentText,
        inversePrimary = accentText,
        secondary = accentText,
        onSecondary = background,
        secondaryContainer = accentSurface,
        onSecondaryContainer = text,
        tertiary = accentText,
        onTertiary = background,
        tertiaryContainer = accentSurface,
        onTertiaryContainer = text,
        background = background,
        onBackground = text,
        surface = background,
        onSurface = text,
        surfaceVariant = hover,
        onSurfaceVariant = textMuted,
        surfaceTint = Color.Transparent,
        inverseSurface = text,
        inverseOnSurface = background,
        error = danger.ink,
        onError = background,
        errorContainer = danger.surface,
        onErrorContainer = danger.ink,
        outline = controlBorder,
        outlineVariant = border,
        scrim = scrim,
        surfaceBright = raised,
        surfaceDim = sunken,
        surfaceContainerLowest = sunken,
        surfaceContainerLow = surface,
        surfaceContainer = surface,
        surfaceContainerHigh = raised,
        surfaceContainerHighest = active,
    )
}

/**
 * Provides the Filebeam palette, type and motion preferences to the tree.
 * Callers pass a resolved [palette] so previews and the gallery can force any combination.
 */
@Composable
fun FilebeamTheme(palette: FilebeamPalette, content: @Composable () -> Unit) {
    val type = remember { FilebeamType() }
    val selection = remember(palette) {
        TextSelectionColors(handleColor = palette.accentText, backgroundColor = palette.accentText.copy(alpha = 0.32f))
    }
    val ripple = remember(palette) { ripple(color = palette.accentText) }
    CompositionLocalProvider(
        LocalPalette provides palette,
        LocalType provides type,
        LocalTextSelectionColors provides selection,
        LocalIndication provides ripple,
        LocalReduceMotion provides rememberReduceMotion(),
    ) {
        MaterialTheme(colorScheme = remember(palette) { palette.toColorScheme() }, typography = type.material, content = content)
    }
}

@Composable
fun FilebeamTheme(appearance: Appearance, instanceColor: Int? = null, content: @Composable () -> Unit) {
    val context = LocalContext.current
    val systemDark = isSystemInDarkTheme()
    // Wallpaper changes arrive as a configuration change, so the palette is rebuilt with it.
    val configuration = LocalConfiguration.current
    val palette = remember(appearance, systemDark, instanceColor, configuration) {
        resolvePalette(context, appearance, systemDark, instanceColor)
    }
    FilebeamTheme(palette, content)
}
