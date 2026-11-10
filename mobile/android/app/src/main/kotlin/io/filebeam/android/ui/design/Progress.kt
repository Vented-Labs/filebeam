package io.filebeam.android.ui.design

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.progressSemantics
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.motion.LocalReduceMotion
import io.filebeam.android.ui.motion.Motion
import kotlin.math.abs
import kotlin.math.max

/**
 * A determinate bar that eases towards its target instead of jumping, as the web's
 * SmoothProgress does: bigger jumps take longer (600 to 1800 ms). Dark themes add a slow sheen.
 * A null [fraction] renders an indeterminate sweep.
 */
@Composable
fun SmoothProgress(
    fraction: Float?,
    modifier: Modifier = Modifier,
    height: Dp = 6.dp,
    fill: Color = Fb.palette.progressFill,
    track: Color = Fb.palette.progressTrack,
) {
    val palette = Fb.palette
    val reduce = LocalReduceMotion.current
    val shown = remember { Animatable(fraction?.coerceIn(0f, 1f) ?: 0f) }
    LaunchedEffect(fraction) {
        val target = fraction?.coerceIn(0f, 1f) ?: return@LaunchedEffect
        if (reduce || target < shown.value) shown.snapTo(target)
        else shown.animateTo(target, tween((600 + 1200 * abs(target - shown.value)).toInt(), easing = Motion.ease))
    }
    val sweep = if (reduce) 0f else {
        val transition = rememberInfiniteTransition(label = "sheen")
        val value by transition.animateFloat(0f, 1f, infiniteRepeatable(tween(if (fraction == null) 1400 else 4800, easing = LinearEasing), RepeatMode.Restart), label = "sweep")
        value
    }
    val semantics = if (fraction == null) Modifier.progressSemantics() else Modifier.progressSemantics(fraction.coerceIn(0f, 1f))
    Canvas(modifier.fillMaxWidth().height(height).then(semantics)) {
        val radius = CornerRadius(size.height / 2)
        drawRoundRect(track, cornerRadius = radius)
        if (fraction == null) {
            val width = size.width * 0.32f
            val start = -width + (size.width + width) * sweep
            clipRect {
                drawRoundRect(
                    Brush.horizontalGradient(listOf(fill.copy(alpha = 0f), fill, fill.copy(alpha = 0f)), start, start + width),
                    Offset(start, 0f), Size(width, size.height), radius,
                )
            }
            return@Canvas
        }
        // A sliver stays visible below 1% so a started transfer never looks empty.
        val width = max(size.width * shown.value, if (shown.value > 0f) size.height else 0f)
        if (width <= 0f) return@Canvas
        drawRoundRect(fill, size = Size(width, size.height), cornerRadius = radius)
        if (palette.dark && shown.value < 1f && !reduce) {
            val band = size.width * 0.28f
            val start = -band + (width + band) * sweep
            clipRect(right = width) {
                drawRect(
                    Brush.horizontalGradient(listOf(Color.Transparent, palette.sheen, Color.Transparent), start, start + band),
                    Offset(start, 0f), Size(band, size.height), blendMode = BlendMode.Plus,
                )
            }
        }
    }
}

/** "<1%" until one percent is reached, then whole percentages. */
fun progressLabel(fraction: Float): String {
    val percent = (fraction.coerceIn(0f, 1f) * 100).toInt()
    return if (fraction > 0f && percent < 1) "<1%" else "$percent%"
}
