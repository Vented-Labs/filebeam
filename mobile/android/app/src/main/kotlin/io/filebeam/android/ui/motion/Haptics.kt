package io.filebeam.android.ui.motion

import android.os.Build
import android.view.HapticFeedbackConstants
import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView

/** One vocabulary of touch feedback, mapped to the best constant each Android version offers. */
class Haptics(private val view: View) {
    private fun perform(constant: Int) {
        view.performHapticFeedback(constant)
    }

    /** A light acknowledgement for presses on primary controls. */
    fun tap() = perform(if (Build.VERSION.SDK_INT >= 27) HapticFeedbackConstants.KEYBOARD_PRESS else HapticFeedbackConstants.VIRTUAL_KEY)

    /** Moving between discrete options: tabs, segments, chips. */
    fun tick() = perform(
        when {
            Build.VERSION.SDK_INT >= 34 -> HapticFeedbackConstants.SEGMENT_TICK
            Build.VERSION.SDK_INT >= 27 -> HapticFeedbackConstants.TEXT_HANDLE_MOVE
            else -> HapticFeedbackConstants.CLOCK_TICK
        },
    )

    fun toggle(on: Boolean) = perform(
        when {
            Build.VERSION.SDK_INT >= 34 -> if (on) HapticFeedbackConstants.TOGGLE_ON else HapticFeedbackConstants.TOGGLE_OFF
            else -> HapticFeedbackConstants.CLOCK_TICK
        },
    )

    /** A gesture crossed the point where releasing will act. */
    fun threshold() = perform(if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.GESTURE_START else HapticFeedbackConstants.LONG_PRESS)

    fun confirm() = perform(if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY)

    fun reject() = perform(if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS)

    fun longPress() = perform(HapticFeedbackConstants.LONG_PRESS)
}

@Composable
fun rememberHaptics(): Haptics {
    val view = LocalView.current
    return remember(view) { Haptics(view) }
}
