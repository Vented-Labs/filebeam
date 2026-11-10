package io.filebeam.android.ui.shell

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.pressScale
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.motion.rememberHaptics
import kotlin.math.roundToInt

@get:StringRes
val Tab.label: Int
    get() = when (this) {
        Tab.Send -> R.string.tab_send
        Tab.Receive -> R.string.tab_receive
        Tab.Transfers -> R.string.tab_transfers
        Tab.Inbox -> R.string.tab_inbox
    }

@get:DrawableRes
val Tab.icon: Int
    get() = when (this) {
        Tab.Send -> R.drawable.ic_approved_upload
        Tab.Receive -> R.drawable.ic_approved_download
        Tab.Transfers -> R.drawable.ic_approved_transfers
        Tab.Inbox -> R.drawable.ic_approved_archive
    }

/** Height reserved at the bottom of tab screens so content clears the floating bar. */
val NavBarClearance = 96.dp

/**
 * The floating navigation bar. A selection pill slides between destinations and the
 * chosen icon gives a small bounce; the inbox shows an unread badge that pops in.
 */
@Composable
fun FloatingNavBar(selected: Tab, onSelect: (Tab) -> Unit, inboxUnread: Int, modifier: Modifier = Modifier) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val density = LocalDensity.current
    var width by remember { mutableIntStateOf(0) }
    val count = Tab.entries.size
    // Measured inside the bar's padding, so each destination owns exactly one slot.
    val slot = if (count == 0) 0f else width.toFloat() / count
    val x by animateFloatAsState(slot * selected.ordinal, motionOr(Motion.spatial()), label = "indicator")
    Box(
        modifier
            .widthIn(max = 440.dp)
            .fillMaxWidth()
            .height(68.dp)
            .shadow(20.dp, Shapes.pill, ambientColor = palette.shadow, spotColor = palette.shadow)
            .clip(Shapes.pill)
            .background(palette.raised)
            .border(1.dp, palette.border, Shapes.pill)
            .padding(6.dp)
            .onSizeChanged { width = it.width },
    ) {
        if (slot > 0f) {
            Box(
                Modifier
                    .offset { IntOffset(x.roundToInt(), 0) }
                    .width(with(density) { slot.toDp() })
                    .fillMaxHeight()
                    .clip(Shapes.pill)
                    .background(palette.selectedSurface)
                    .border(1.dp, palette.choiceBorder.copy(alpha = 0.6f), Shapes.pill),
            )
        }
        Row(Modifier.fillMaxWidth().fillMaxHeight().selectableGroup()) {
            Tab.entries.forEach { tab ->
                val isSelected = tab == selected
                val interaction = remember { MutableInteractionSource() }
                val ink by animateColorAsState(if (isSelected) palette.accentText else palette.textMuted, motionOr(Motion.fade(Motion.SELECTION)), label = "ink")
                val pop by animateFloatAsState(if (isSelected) 1.08f else 1f, motionOr(Motion.bouncy()), label = "pop")
                val label = stringResource(tab.label)
                val description = if (tab == Tab.Inbox && inboxUnread > 0) pluralStringResource(R.plurals.inbox_unread_description, inboxUnread, inboxUnread) else label
                Column(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .pressScale(interaction, 0.94f)
                        .clip(Shapes.pill)
                        .selectable(isSelected, interaction, indication = null, role = Role.Tab) {
                            if (!isSelected) haptics.tick()
                            onSelect(tab)
                        }
                        .semantics { contentDescription = description },
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Box {
                        FbIcon(tab.icon, Modifier.graphicsLayer { scaleX = pop; scaleY = pop }, tint = ink, size = 22.dp)
                        androidx.compose.animation.AnimatedVisibility(
                            tab == Tab.Inbox && inboxUnread > 0,
                            Modifier.align(Alignment.TopEnd).offset(x = 8.dp, y = (-6).dp),
                            enter = scaleIn(Motion.bouncy()) + fadeIn(Motion.fade()),
                            exit = scaleOut(Motion.exit()) + fadeOut(Motion.exit()),
                        ) {
                            Box(Modifier.size(18.dp).clip(Shapes.pill).background(palette.action).border(1.5.dp, palette.raised, Shapes.pill), contentAlignment = Alignment.Center) {
                                FbText(if (inboxUnread > 9) "9+" else inboxUnread.toString(), style = Fb.type.caption.copy(fontSize = androidx.compose.ui.unit.TextUnit(10f, androidx.compose.ui.unit.TextUnitType.Sp)), color = palette.onAction, maxLines = 1)
                            }
                        }
                    }
                    FbText(label, Modifier.padding(top = 2.dp), style = Fb.type.caption, color = ink, maxLines = 1)
                }
            }
        }
    }
}
