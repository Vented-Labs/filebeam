package io.filebeam.android.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.motion.LocalReduceMotion
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.motion.rememberHaptics

enum class ButtonStyle { Primary, Secondary, Quiet, Danger }

/**
 * The app's button. Primary fills with the action gradient; Secondary is outlined;
 * Quiet is text-only; Danger is a quiet destructive action.
 * While [loading] the label gives way to a spinner and presses are ignored.
 */
@Composable
fun FbButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    style: ButtonStyle = ButtonStyle.Primary,
    @DrawableRes icon: Int? = null,
    @DrawableRes trailingIcon: Int? = null,
    enabled: Boolean = true,
    loading: Boolean = false,
    height: Dp = Space.control,
) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val active = enabled && !loading
    val content by animateColorAsState(
        when {
            !enabled -> palette.disabledText
            style == ButtonStyle.Primary -> palette.onAction
            style == ButtonStyle.Danger -> palette.danger.ink
            style == ButtonStyle.Quiet -> palette.accentText
            else -> palette.text
        },
        motionOr(Motion.fade()), label = "content",
    )
    val borderColor by animateColorAsState(
        when {
            !enabled -> palette.disabledBorder
            style == ButtonStyle.Primary -> palette.actionBorder
            style == ButtonStyle.Secondary -> if (pressed) palette.controlBorderHover else palette.controlBorder
            else -> Color.Transparent
        },
        motionOr(Motion.fade()), label = "border",
    )
    val shape = Shapes.control
    val fill = when {
        !enabled && style == ButtonStyle.Primary -> Modifier.background(palette.disabledSurface)
        style == ButtonStyle.Primary -> Modifier.background(Brush.verticalGradient(if (pressed) listOf(palette.actionPressed, palette.actionPressed) else palette.actionGradient))
        style == ButtonStyle.Secondary -> Modifier.background(if (pressed) palette.hover else palette.surface)
        else -> Modifier.background(if (pressed) palette.hover else Color.Transparent)
    }
    Box(
        modifier
            .pressScale(interaction)
            .then(if (style == ButtonStyle.Primary && enabled) Modifier.shadow(6.dp, shape, ambientColor = palette.shadow, spotColor = palette.action.copy(alpha = 0.45f)) else Modifier)
            .clip(shape)
            .then(fill)
            .border(BorderStroke(1.dp, borderColor), shape)
            // A faint top highlight lifts the primary fill in dark themes.
            .drawBehind {
                if (style == ButtonStyle.Primary && enabled) {
                    drawLine(palette.highlight, Offset(size.height / 3, 1.dp.toPx()), Offset(size.width - size.height / 3, 1.dp.toPx()), 1.dp.toPx())
                }
            }
            .clickable(interaction, indication = null, enabled = active, role = Role.Button) {
                haptics.tap()
                onClick()
            }
            .defaultMinSize(minHeight = height)
            .padding(horizontal = Space.xl),
        contentAlignment = Alignment.Center,
    ) {
        AnimatedContent(
            loading,
            transitionSpec = {
                (fadeIn(motionSpec()) + scaleIn(motionSpec(), initialScale = 0.8f)) togetherWith (fadeOut(Motion.exit()) + scaleOut(Motion.exit(), targetScale = 0.8f))
            },
            label = "button",
        ) { busy ->
            if (busy) {
                Spinner(color = content, size = 20.dp)
            } else {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    if (icon != null) FbIcon(icon, tint = content, size = 20.dp)
                    FbText(text, style = Fb.type.label, color = content, maxLines = 1)
                    if (trailingIcon != null) FbIcon(trailingIcon, tint = content, size = 18.dp)
                }
            }
        }
    }
}

private fun <T> motionSpec() = Motion.fade<T>(Motion.CONTROL)

/** A round, touch-sized icon button with an accessible label. */
@Composable
fun IconAction(
    @DrawableRes icon: Int,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tint: Color = Fb.palette.textMuted,
    enabled: Boolean = true,
    bordered: Boolean = false,
    size: Dp = 44.dp,
) {
    val palette = Fb.palette
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val background by animateColorAsState(if (pressed) palette.hover else if (bordered) palette.surface else Color.Transparent, motionOr(Motion.fade()), label = "bg")
    Box(
        modifier
            .pressScale(interaction, 0.9f)
            .size(size)
            .clip(Shapes.pill)
            .background(background)
            .then(if (bordered) Modifier.border(1.dp, palette.border, Shapes.pill) else Modifier)
            .clickable(interaction, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        FbIcon(icon, tint = if (enabled) tint else palette.disabledText, size = 22.dp)
    }
}

/** An indeterminate ring. Holds still when motion is reduced. */
@Composable
fun Spinner(modifier: Modifier = Modifier, color: Color = Fb.palette.accentText, size: Dp = 22.dp, stroke: Dp = 2.dp) {
    val angle = if (LocalReduceMotion.current) 0f else {
        val transition = rememberInfiniteTransition(label = "spinner")
        val value by transition.animateFloat(0f, 360f, infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Restart), label = "angle")
        value
    }
    Box(
        modifier.size(size).rotate(angle).drawWithContent {
            val width = stroke.toPx()
            val inset = width / 2
            drawArc(color.copy(alpha = 0.22f), 0f, 360f, false, Offset(inset, inset), Size(this.size.width - width, this.size.height - width), style = Stroke(width))
            drawArc(color, -90f, 100f, false, Offset(inset, inset), Size(this.size.width - width, this.size.height - width), style = Stroke(width, cap = StrokeCap.Round))
        },
    )
}
