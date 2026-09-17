package io.filebeam.android.ui

import android.app.UiModeManager
import android.content.res.Configuration
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect

@Composable
fun FilebeamTheme(content: @Composable () -> Unit) {
    val context = LocalContext.current
    val dark = isSystemInDarkTheme()
    val configuration = LocalConfiguration.current
    var refresh by remember { mutableIntStateOf(0) }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { refresh++ }
    if (Build.VERSION.SDK_INT >= 34) DisposableEffect(context) {
        val manager = context.getSystemService(UiModeManager::class.java)
        val listener = UiModeManager.ContrastChangeListener { refresh++ }
        manager.addContrastChangeListener(context.mainExecutor, listener)
        onDispose { manager.removeContrastChangeListener(listener) }
    }
    val colors = resolveFilebeamColorScheme(context, dark, configuration, refresh)
    MaterialTheme(colorScheme = colors, typography = FilebeamTypography, shapes = FilebeamShapes, content = content)
}

@Composable
private fun resolveFilebeamColorScheme(
    context: android.content.Context,
    dark: Boolean,
    @Suppress("UNUSED_PARAMETER") configuration: Configuration,
    @Suppress("UNUSED_PARAMETER") refresh: Int,
) = when {
    Build.VERSION.SDK_INT >= 31 -> if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
    dark -> FilebeamDarkFallback
    else -> FilebeamLightFallback
}

/** Compatibility bridge for screens being migrated to the mandatory device appearance contract. */
@Composable
fun FilebeamTheme(@Suppress("UNUSED_PARAMETER") dynamic: Boolean, content: @Composable () -> Unit) = FilebeamTheme(content)

/** Complete paired schemes for API 26-30; API 31+ always uses Android-resolved colors. */
val FilebeamLightFallback = lightColorScheme(
    primary = Color(0xFF006A60), onPrimary = Color(0xFFFFFFFF), primaryContainer = Color(0xFF75F8E9), onPrimaryContainer = Color(0xFF00201C),
    secondary = Color(0xFF4A635F), onSecondary = Color(0xFFFFFFFF), secondaryContainer = Color(0xFFCCE8E2), onSecondaryContainer = Color(0xFF06201D),
    tertiary = Color(0xFF456179), onTertiary = Color(0xFFFFFFFF), tertiaryContainer = Color(0xFFCDE5FF), onTertiaryContainer = Color(0xFF001E30),
    error = Color(0xFFBA1A1A), onError = Color(0xFFFFFFFF), errorContainer = Color(0xFFFFDAD6), onErrorContainer = Color(0xFF410002),
    background = Color(0xFFF7FBF8), onBackground = Color(0xFF171D1B), surface = Color(0xFFF7FBF8), onSurface = Color(0xFF171D1B),
    surfaceVariant = Color(0xFFDAE5E1), onSurfaceVariant = Color(0xFF3F4946), outline = Color(0xFF6F7975), outlineVariant = Color(0xFFBEC9C5),
)

val FilebeamDarkFallback = darkColorScheme(
    primary = Color(0xFF53DBC9), onPrimary = Color(0xFF003730), primaryContainer = Color(0xFF005048), onPrimaryContainer = Color(0xFF75F8E9),
    secondary = Color(0xFFB0CCC6), onSecondary = Color(0xFF1B3531), secondaryContainer = Color(0xFF324B47), onSecondaryContainer = Color(0xFFCCE8E2),
    tertiary = Color(0xFFACC9E5), onTertiary = Color(0xFF153348), tertiaryContainer = Color(0xFF2D4960), onTertiaryContainer = Color(0xFFCDE5FF),
    error = Color(0xFFFFB4AB), onError = Color(0xFF690005), errorContainer = Color(0xFF93000A), onErrorContainer = Color(0xFFFFDAD6),
    background = Color(0xFF0F1513), onBackground = Color(0xFFDEE4E0), surface = Color(0xFF0F1513), onSurface = Color(0xFFDEE4E0),
    surfaceVariant = Color(0xFF3F4946), onSurfaceVariant = Color(0xFFBEC9C5), outline = Color(0xFF89938F), outlineVariant = Color(0xFF3F4946),
)

val FilebeamTypography = Typography(
    headlineMedium = androidx.compose.ui.text.TextStyle(fontSize = 28.sp, lineHeight = 36.sp),
    titleLarge = androidx.compose.ui.text.TextStyle(fontSize = 22.sp, lineHeight = 28.sp),
    titleMedium = androidx.compose.ui.text.TextStyle(fontSize = 16.sp, lineHeight = 24.sp),
    bodyLarge = androidx.compose.ui.text.TextStyle(fontSize = 16.sp, lineHeight = 24.sp),
    bodyMedium = androidx.compose.ui.text.TextStyle(fontSize = 14.sp, lineHeight = 20.sp),
    bodySmall = androidx.compose.ui.text.TextStyle(fontSize = 12.sp, lineHeight = 16.sp),
)

val FilebeamShapes = Shapes(
    extraSmall = androidx.compose.foundation.shape.RoundedCornerShape(8.dp),
    small = androidx.compose.foundation.shape.RoundedCornerShape(12.dp),
    medium = androidx.compose.foundation.shape.RoundedCornerShape(20.dp),
    large = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
    extraLarge = androidx.compose.foundation.shape.RoundedCornerShape(28.dp),
)

internal fun contrastRatio(foreground: Color, background: Color): Float {
    fun channel(value: Float) = if (value <= 0.04045f) value / 12.92f else ((value + 0.055f) / 1.055f).let { it * it * it * 1f }
    fun luminance(color: Color) = 0.2126f * channel(color.red) + 0.7152f * channel(color.green) + 0.0722f * channel(color.blue)
    val light = maxOf(luminance(foreground), luminance(background))
    val dark = minOf(luminance(foreground), luminance(background))
    return (light + 0.05f) / (dark + 0.05f)
}
