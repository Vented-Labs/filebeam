package io.filebeam.android.ui.design

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.motion.LocalReduceMotion
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import kotlin.math.PI
import kotlin.math.sin

/** Geometry of the folded "F and beam" mark, from `backend/public/brand/filebeam-mark.svg` (168 x 244 units). */
private object MarkGeometry {
    const val WIDTH = 168f
    const val HEIGHT = 244f
    private fun parse(data: String): Path = PathParser().parsePathString(data).toPath()
    val body by lazy { parse("M14 65L140 3C153 -3 168 5 168 20L168 42C168 57 165 63 153 69L81 105L145 134L79 166C90 171 93 178 93 191L93 225C93 239 84 248 70 242L16 216C5 211 0 203 0 191L0 87C0 77 5 69 14 65Z") }
    val fold by lazy { parse("M14 65C5 69 0 77 0 87L0 158C0 150 4 145 12 141L81 105L14 73C8 70 8 68 14 65Z") }
    val top by lazy { parse("M14 65L140 3C153 -3 168 5 168 20L168 42C168 57 165 63 153 69L81 105L14 73C8 70 8 68 14 65Z") }
    val beam by lazy { parse("M12 141L81 105L145 134L79 166Z") }
    val stem by lazy { parse("M12 141L79 166C90 171 93 178 93 191L93 225C93 239 84 248 70 242L16 216C5 211 0 203 0 191L0 158C0 150 4 145 12 141Z") }
}

/**
 * The Filebeam mark, drawn from the brand gradient of the active palette so it follows
 * presets, custom colours and Material You alike.
 */
@Composable
fun FilebeamMark(modifier: Modifier = Modifier, height: Dp = 28.dp) {
    val gradient = Fb.palette.brandGradient
    Canvas(modifier.size(width = height * (MarkGeometry.WIDTH / MarkGeometry.HEIGHT), height = height).clearAndSetSemantics {}) {
        val factor = size.height / MarkGeometry.HEIGHT
        scale(factor, factor, Offset.Zero) {
            val seed = gradient[2]
            val tip = gradient[4]
            drawPath(MarkGeometry.body, seed)
            drawPath(MarkGeometry.fold, Brush.linearGradient(listOf(lerp(tip, Color.White, 0.12f), lerp(tip, Color.White, 0.45f)), Offset(0f, 91f), Offset(81f, 87f)))
            drawPath(MarkGeometry.top, Brush.linearGradient(listOf(gradient[1], seed, tip), Offset(8f, 76f), Offset(150f, 5f)))
            drawPath(MarkGeometry.beam, Brush.linearGradient(listOf(gradient[0], gradient[1], seed), Offset(13f, 135f), Offset(116f, 106f)))
            drawPath(MarkGeometry.stem, Brush.linearGradient(listOf(seed, lerp(seed, tip, 0.25f)), Offset(-8f, 129f), Offset(63f, 216f)))
        }
    }
}

/**
 * The stacked-card emblem from the web drop zone: an image card and a code card fanned behind
 * a front card carrying the mark, sealed with a small lock.
 *
 * [engaged] fans the cards wider and lifts them (press, drag-over). [sealed] draws the lock closed
 * and pulls the stack together, used once a transfer is protected.
 */
@Composable
fun Emblem(modifier: Modifier = Modifier, engaged: Boolean = false, sealed: Boolean = false, size: Dp = 132.dp) {
    val palette = Fb.palette
    val reduce = LocalReduceMotion.current
    val fan by animateFloatAsState(
        when {
            sealed -> 0.25f
            engaged -> 1f
            else -> 0.62f
        },
        motionOr(Motion.bouncy()), label = "fan",
    )
    val lift by animateFloatAsState(if (engaged) 1f else 0f, motionOr(Motion.spatial()), label = "lift")
    val drift = if (reduce) 0f else {
        val transition = rememberInfiniteTransition(label = "emblem")
        val phase by transition.animateFloat(0f, 1f, infiniteRepeatable(tween(6400, easing = LinearEasing), RepeatMode.Restart), label = "phase")
        sin(phase * 2 * PI).toFloat()
    }
    val cardShape = remember { RoundedCornerShape(16.dp) }
    val cardWidth = size * 0.52f
    val cardHeight = size * 0.62f
    Box(modifier.size(width = size * 1.35f, height = size).clearAndSetSemantics {}, contentAlignment = Alignment.Center) {
        @Composable
        fun BackCard(rotation: Float, shiftX: Dp, icon: Int, phase: Float) {
            Box(
                Modifier
                    .graphicsLayer {
                        rotationZ = rotation * fan
                        translationX = shiftX.toPx() * fan
                        translationY = 6.dp.toPx() - lift * 5.dp.toPx() + drift * phase * 2.dp.toPx()
                        transformOrigin = TransformOrigin(0.5f, 1f)
                    }
                    .size(cardWidth, cardHeight)
                    .clip(cardShape)
                    .background(Brush.linearGradient(listOf(palette.cardBack, palette.cardBackEnd)))
                    .border(1.dp, palette.controlBorder.copy(alpha = 0.55f), cardShape),
                contentAlignment = Alignment.Center,
            ) {
                FbIcon(icon, tint = palette.textMuted.copy(alpha = 0.7f), size = size * 0.2f)
            }
        }
        BackCard(-21f, -size * 0.36f, R.drawable.ic_approved_image, 1f)
        BackCard(23f, size * 0.36f, R.drawable.ic_approved_code, -1f)
        Box(
            Modifier
                .graphicsLayer { translationY = -lift * 7.dp.toPx() - drift * 2.5.dp.toPx() }
                .shadow(18.dp, cardShape, ambientColor = palette.shadow, spotColor = palette.shadow)
                .size(cardWidth * 1.08f, cardHeight * 1.08f)
                .clip(cardShape)
                .background(Brush.linearGradient(listOf(palette.cardFront, palette.cardFrontEnd)))
                .border(1.dp, palette.controlBorder.copy(alpha = 0.7f), cardShape),
            contentAlignment = Alignment.Center,
        ) {
            FilebeamMark(height = size * 0.26f)
        }
        // The seal sits on the front card's lower corner.
        val sealScale by animateFloatAsState(if (sealed) 1.15f else 1f, motionOr(Motion.bouncy()), label = "seal")
        Box(
            Modifier
                .align(Alignment.Center)
                .offset(x = cardWidth * 0.5f, y = cardHeight * 0.44f)
                .graphicsLayer {
                    scaleX = sealScale
                    scaleY = sealScale
                    translationY = -lift * 7.dp.toPx() - drift * 2.5.dp.toPx()
                }
                .size(size * 0.2f)
                .clip(Shapes.pill)
                .background(if (sealed) palette.action else palette.raised)
                .border(1.dp, if (sealed) palette.actionBorder else palette.controlBorder, Shapes.pill),
            contentAlignment = Alignment.Center,
        ) {
            FbIcon(R.drawable.ic_approved_lock, tint = if (sealed) palette.onAction else palette.textMuted, size = size * 0.11f)
        }
    }
}

/**
 * The page backdrop: the palette background lit by two soft ellipses, as on the web.
 * [energy] in 0..1 brightens the glow while work is in flight.
 */
@Composable
fun AmbientBackground(modifier: Modifier = Modifier, energy: Float = 0f, content: @Composable BoxScope.() -> Unit) {
    val palette = Fb.palette
    val reduce = LocalReduceMotion.current
    val level by animateFloatAsState(energy.coerceIn(0f, 1f), motionOr(Motion.spatialSlow()), label = "energy")
    val drift = if (reduce) 0f else {
        val transition = rememberInfiniteTransition(label = "ambient")
        val phase by transition.animateFloat(0f, 1f, infiniteRepeatable(tween(18000, easing = LinearEasing), RepeatMode.Restart), label = "phase")
        sin(phase * 2 * PI).toFloat()
    }
    Box(
        modifier.fillMaxSize().background(palette.background).drawBehind {
            val boost = 1f + level * 1.4f
            fun glow(color: Color, centerX: Float, centerY: Float, radiusX: Float, radiusY: Float) {
                val center = Offset(size.width * centerX, size.width * centerY)
                // Radial brushes are circular; scaling the canvas vertically yields the ellipse.
                val ratio = radiusY / radiusX
                translate(center.x, center.y) {
                    scale(1f, ratio, Offset.Zero) {
                        drawCircle(
                            Brush.radialGradient(listOf(color.copy(alpha = (color.alpha * boost).coerceAtMost(1f)), Color.Transparent), Offset.Zero, size.width * radiusX),
                            size.width * radiusX, Offset.Zero,
                        )
                    }
                }
            }
            glow(palette.glowPrimary, 0.49f + drift * 0.05f, 0.24f, 0.95f, 0.8f)
            glow(palette.glowSecondary, 0.92f - drift * 0.06f, 0.78f + drift * 0.04f, 0.8f, 0.62f)
        },
        content = content,
    )
}

/** Keeps a square aspect for decorative boxes that should scale with width. */
internal fun Modifier.square() = aspectRatio(1f)
