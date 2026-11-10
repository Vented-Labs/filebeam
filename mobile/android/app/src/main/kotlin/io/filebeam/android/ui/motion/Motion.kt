package io.filebeam.android.ui.motion

import android.provider.Settings
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext

/**
 * The single source of timing for the app. Spatial motion (position, size, shape) uses springs
 * so it stays interruptible; effects (opacity, colour) use the web's easing curve and durations.
 * No screen defines its own durations.
 */
object Motion {
    /** `--fb-ease`: a long, soft deceleration. */
    val ease = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)

    /** `--fb-ease-hover`: the standard symmetric curve for small state changes. */
    val easeStandard = CubicBezierEasing(0.4f, 0f, 0.2f, 1f)
    val easeExit = CubicBezierEasing(0.4f, 0f, 1f, 1f)

    const val CONTROL = 180
    const val SWITCH = 300
    const val SELECTION = 380
    const val MENU_IN = 280
    const val MENU_OUT = 200
    const val PANE = 460
    const val DIALOG_IN = 360
    const val DIALOG_OUT = 210
    const val TOAST_IN = 280
    const val TOAST_OUT = 180

    /** Delay between consecutive items in a staggered entrance. */
    const val STAGGER = 45

    /** Small, quick moves: presses, toggles, chips. */
    fun <T> spatialFast(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.86f, stiffness = 900f)

    /** The default for elements that travel: indicators, cards, sheets. Settles with a faint overshoot. */
    fun <T> spatial(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.8f, stiffness = 380f)

    /** Large surfaces and hero pieces. */
    fun <T> spatialSlow(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.84f, stiffness = 180f)

    /** Playful emphasis for confirmations and badges. */
    fun <T> bouncy(): FiniteAnimationSpec<T> = spring(dampingRatio = 0.55f, stiffness = 420f)

    /** Opacity and colour never overshoot. */
    fun <T> effects(): FiniteAnimationSpec<T> = spring(dampingRatio = Spring.DampingRatioNoBouncy, stiffness = 520f)

    fun <T> fade(duration: Int = CONTROL, delay: Int = 0): FiniteAnimationSpec<T> = tween(duration, delay, easeStandard)
    fun <T> enter(duration: Int = PANE, delay: Int = 0): FiniteAnimationSpec<T> = tween(duration, delay, ease)
    fun <T> exit(duration: Int = MENU_OUT): FiniteAnimationSpec<T> = tween(duration, 0, easeExit)
}

/**
 * True when the user turned animations off. Finite animations already collapse because Compose
 * honours the system animator scale; this flag is for looping and decorative motion, which
 * must be skipped outright.
 */
val LocalReduceMotion = staticCompositionLocalOf { false }

@Composable
@ReadOnlyComposable
internal fun rememberReduceMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    return Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
}

/** Picks [spec] normally and an instant change when motion is reduced. */
@Composable
@ReadOnlyComposable
fun <T> motionOr(spec: FiniteAnimationSpec<T>): FiniteAnimationSpec<T> = if (LocalReduceMotion.current) snap() else spec
