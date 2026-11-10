package io.filebeam.android.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.motion.rememberHaptics
import kotlin.math.roundToInt

/** A switch whose thumb stretches while pressed and springs across on change. */
@Composable
fun FbSwitch(checked: Boolean, onCheckedChange: (Boolean) -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    SwitchVisual(
        checked, enabled, pressed,
        modifier.clip(Shapes.pill).toggleable(checked, interaction, indication = null, enabled = enabled, role = Role.Switch) {
            haptics.toggle(it)
            onCheckedChange(it)
        },
    )
}

/** The drawn switch without behaviour, for rows that own the toggle themselves. */
@Composable
private fun SwitchVisual(checked: Boolean, enabled: Boolean, pressed: Boolean, modifier: Modifier = Modifier) {
    val palette = Fb.palette
    val track by animateColorAsState(
        when {
            !enabled -> palette.disabledSurface
            checked -> palette.action
            else -> palette.controlBorder
        },
        motionOr(Motion.fade(Motion.SWITCH)), label = "track",
    )
    val thumbWidth by animateDpAsState(if (pressed) 26.dp else 20.dp, motionOr(Motion.spatialFast()), label = "stretch")
    val offset by animateDpAsState(if (checked) 42.dp - thumbWidth else 0.dp, motionOr(Motion.spatial()), label = "thumb")
    Box(
        modifier.size(width = 48.dp, height = 28.dp).clip(Shapes.pill).background(track).padding(3.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(
            Modifier
                .offset(x = offset)
                .size(width = thumbWidth, height = 22.dp)
                .shadow(2.dp, Shapes.pill)
                .clip(Shapes.pill)
                .background(if (checked) palette.onAction else Color.White),
        )
    }
}

data class Segment<T>(val value: T, val label: String, @DrawableRes val icon: Int? = null, val caption: String? = null, val enabled: Boolean = true)

/**
 * A row of mutually exclusive options with a selection pill that slides between them.
 * With captions it renders the taller two-line "transport" variant.
 */
@Composable
fun <T> SegmentedRail(
    segments: List<Segment<T>>,
    selected: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val density = LocalDensity.current
    val offsets = remember(segments.size) { mutableStateListOf<Int>().apply { repeat(segments.size) { add(0) } } }
    val widths = remember(segments.size) { mutableStateListOf<Int>().apply { repeat(segments.size) { add(0) } } }
    val index = segments.indexOfFirst { it.value == selected }.coerceAtLeast(0)
    val tall = segments.any { it.caption != null }
    val x by animateFloatAsState(offsets.getOrElse(index) { 0 }.toFloat(), motionOr(Motion.spatial()), label = "x")
    val width by animateFloatAsState(widths.getOrElse(index) { 0 }.toFloat(), motionOr(Motion.spatial()), label = "width")
    val railHeight: Dp = if (tall) 64.dp else 48.dp
    Box(
        modifier
            .fillMaxWidth()
            .height(railHeight)
            .clip(Shapes.card)
            .background(palette.sunken)
            .border(1.dp, palette.border, Shapes.card)
            .padding(4.dp),
    ) {
        if (width > 0f) {
            Box(
                Modifier
                    .offset { IntOffset(x.roundToInt(), 0) }
                    .width(with(density) { width.toDp() })
                    .height(railHeight - 8.dp)
                    .clip(Shapes.control)
                    .background(palette.selectedSurface)
                    .border(1.dp, palette.choiceBorder, Shapes.control),
            )
        }
        Row(Modifier.fillMaxWidth().selectableGroup()) {
            segments.forEachIndexed { position, segment ->
                val isSelected = position == index
                val available = enabled && segment.enabled
                val ink by animateColorAsState(
                    when {
                        !available -> palette.disabledText
                        isSelected -> palette.text
                        else -> palette.textMuted
                    },
                    motionOr(Motion.fade(Motion.SELECTION)), label = "ink",
                )
                Row(
                    Modifier
                        .weight(1f)
                        .height(railHeight - 8.dp)
                        .onGloballyPositioned {
                            offsets[position] = it.positionInParent().x.roundToInt()
                            widths[position] = it.size.width
                        }
                        .clip(Shapes.control)
                        .selectable(isSelected, enabled = available, role = Role.Tab) {
                            if (!isSelected) {
                                haptics.tick()
                                onSelect(segment.value)
                            }
                        }
                        .padding(horizontal = Space.md),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = if (tall) Arrangement.Start else Arrangement.Center,
                ) {
                    if (segment.icon != null) {
                        FbIcon(segment.icon, tint = if (isSelected && available) palette.accentText else ink, size = if (tall) 24.dp else 20.dp)
                        Box(Modifier.width(if (tall) Space.md else Space.sm))
                    }
                    Column {
                        FbText(segment.label, style = if (tall) Fb.type.smallStrong else Fb.type.label, color = ink, maxLines = 1)
                        if (segment.caption != null) FbText(segment.caption, style = Fb.type.caption, color = if (available) palette.textMuted else palette.disabledText, maxLines = 1)
                    }
                }
            }
        }
    }
}

/** A compact selectable pill for filters. */
@Composable
fun FilterChip(label: String, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, @DrawableRes icon: Int? = null) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val background by animateColorAsState(if (selected) palette.selectedSurface else palette.surface, motionOr(Motion.fade()), label = "bg")
    val border by animateColorAsState(if (selected) palette.choiceBorder else palette.border, motionOr(Motion.fade()), label = "border")
    val ink by animateColorAsState(if (selected) palette.accentText else palette.textMuted, motionOr(Motion.fade()), label = "ink")
    Row(
        modifier
            .pressScale(interaction, 0.95f)
            .height(36.dp)
            .clip(Shapes.pill)
            .background(background)
            .border(1.dp, border, Shapes.pill)
            .selectable(selected, interaction, indication = null, role = Role.Tab) {
                haptics.tick()
                onClick()
            }
            .padding(horizontal = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.xs),
    ) {
        if (icon != null) FbIcon(icon, tint = ink, size = 16.dp)
        FbText(label, style = Fb.type.smallStrong, color = ink, maxLines = 1)
    }
}

/** A labelled row with a trailing switch; the whole row toggles. */
@Composable
fun SwitchRow(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    description: String? = null,
    @DrawableRes icon: Int? = null,
    enabled: Boolean = true,
) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Row(
        modifier
            .fillMaxWidth()
            .clip(Shapes.control)
            .toggleable(checked, interaction, indication = null, enabled = enabled, role = Role.Switch) {
                haptics.toggle(it)
                onCheckedChange(it)
            }
            .padding(vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        if (icon != null) FbIcon(icon, tint = if (enabled) palette.textMuted else palette.disabledText)
        Column(Modifier.weight(1f)) {
            FbText(title, style = Fb.type.bodyStrong, color = if (enabled) palette.text else palette.disabledText)
            if (description != null) FbText(description, style = Fb.type.small, color = if (enabled) palette.textMuted else palette.disabledText)
        }
        SwitchVisual(checked, enabled, pressed)
    }
}
