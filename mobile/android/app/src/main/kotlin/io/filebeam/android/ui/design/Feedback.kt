package io.filebeam.android.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.design.color.StatusColors
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import kotlinx.coroutines.delay

enum class Tone { Neutral, Accent, Success, Warning, Danger, Info }

@Composable
private fun Tone.colors(): StatusColors {
    val palette = Fb.palette
    return when (this) {
        Tone.Neutral -> StatusColors(palette.textMuted, palette.hover, palette.border)
        Tone.Accent -> StatusColors(palette.accentText, palette.accentSurface, palette.choiceBorder)
        Tone.Success -> palette.success
        Tone.Warning -> palette.warning
        Tone.Danger -> palette.danger
        Tone.Info -> palette.info
    }
}

/** A small rounded status label. */
@Composable
fun StatusPill(text: String, modifier: Modifier = Modifier, tone: Tone = Tone.Neutral, @DrawableRes icon: Int? = null) {
    val colors = tone.colors()
    Row(
        modifier.clip(Shapes.pill).background(colors.surface).border(1.dp, colors.border.copy(alpha = 0.5f), Shapes.pill).padding(horizontal = Space.sm, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        if (icon != null) FbIcon(icon, tint = colors.ink, size = 14.dp)
        FbText(text, style = Fb.type.caption, color = colors.ink, maxLines = 1)
    }
}

/** An inline notice. Danger and warning notices are announced to screen readers. */
@Composable
fun Notice(text: String, modifier: Modifier = Modifier, tone: Tone = Tone.Info, @DrawableRes icon: Int? = null, action: (@Composable () -> Unit)? = null) {
    val colors = tone.colors()
    val defaultIcon = when (tone) {
        Tone.Danger, Tone.Warning -> R.drawable.ic_approved_alert
        Tone.Success -> R.drawable.ic_approved_check
        else -> R.drawable.ic_approved_info
    }
    Row(
        modifier
            .fillMaxWidth()
            .clip(Shapes.control)
            .background(colors.surface)
            .border(1.dp, colors.border.copy(alpha = 0.55f), Shapes.control)
            .padding(Space.md)
            .semantics { if (tone == Tone.Danger || tone == Tone.Warning) liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        FbIcon(icon ?: defaultIcon, tint = colors.ink, size = 20.dp)
        FbText(text, Modifier.weight(1f), style = Fb.type.small, color = colors.ink)
        action?.invoke()
    }
}

/** A centred illustration, statement and optional action for empty and error states. */
@Composable
fun EmptyState(
    @DrawableRes icon: Int,
    title: String,
    modifier: Modifier = Modifier,
    body: String? = null,
    tone: Tone = Tone.Accent,
    action: (@Composable () -> Unit)? = null,
) {
    val palette = Fb.palette
    val colors = tone.colors()
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val rise by animateFloatAsState(if (shown) 0f else 1f, motionOr(Motion.spatialSlow()), label = "rise")
    Column(
        modifier.fillMaxWidth().padding(horizontal = Space.xxl, vertical = Space.xxxl).graphicsLayer {
            alpha = 1f - rise
            translationY = rise * 16.dp.toPx()
        },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(
            Modifier
                .size(72.dp)
                .clip(Shapes.panel)
                .background(Brush.linearGradient(listOf(colors.surface, palette.surface)))
                .border(1.dp, colors.border.copy(alpha = 0.5f), Shapes.panel),
            contentAlignment = Alignment.Center,
        ) {
            FbIcon(icon, tint = colors.ink, size = 30.dp)
        }
        FbText(title, style = Fb.type.title, textAlign = TextAlign.Center)
        if (body != null) FbText(body, Modifier.widthIn(max = 320.dp), style = Fb.type.small, color = palette.textMuted, textAlign = TextAlign.Center)
        if (action != null) Box(Modifier.padding(top = Space.sm)) { action() }
    }
}

data class ToastMessage(val text: String, val tone: Tone = Tone.Neutral, val id: Long = System.nanoTime())

/** Shows the latest message as a floating pill that rises in, lingers, and drops away. */
@Composable
fun ToastHost(message: ToastMessage?, onDismissed: (ToastMessage) -> Unit, modifier: Modifier = Modifier) {
    val palette = Fb.palette
    var current by remember { mutableStateOf<ToastMessage?>(null) }
    var visible by remember { mutableStateOf(false) }
    LaunchedEffect(message?.id) {
        if (message == null) return@LaunchedEffect
        current = message
        visible = true
        delay(2600L + message.text.length * 28L)
        visible = false
        delay(Motion.TOAST_OUT.toLong())
        onDismissed(message)
    }
    AnimatedVisibility(
        visible,
        modifier,
        enter = fadeIn(Motion.fade(Motion.TOAST_IN)) + slideInVertically(Motion.spatial()) { it / 2 } + scaleIn(Motion.spatial(), initialScale = 0.94f),
        exit = fadeOut(Motion.exit(Motion.TOAST_OUT)) + slideOutVertically(Motion.exit(Motion.TOAST_OUT)) { it / 3 },
    ) {
        val shown = current ?: return@AnimatedVisibility
        val colors = shown.tone.colors()
        Row(
            Modifier
                .padding(horizontal = Space.gutter)
                .shadow(16.dp, Shapes.control, ambientColor = palette.shadow, spotColor = palette.shadow)
                .clip(Shapes.control)
                .background(palette.raised)
                .border(1.dp, palette.border, Shapes.control)
                .padding(horizontal = Space.lg, vertical = Space.md)
                .semantics { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Space.md),
        ) {
            if (shown.tone != Tone.Neutral) {
                FbIcon(
                    when (shown.tone) {
                        Tone.Success -> R.drawable.ic_approved_check
                        Tone.Danger, Tone.Warning -> R.drawable.ic_approved_alert
                        else -> R.drawable.ic_approved_info
                    },
                    tint = colors.ink, size = 20.dp,
                )
            }
            FbText(shown.text, style = Fb.type.smallStrong)
        }
    }
}
