package io.filebeam.android.ui.feature.transfers

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.SavedTransferPresentation
import io.filebeam.android.platform.TransferKind
import io.filebeam.android.platform.TransferUiState
import io.filebeam.android.ui.common.formatAbsolute
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.common.parseInstant
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbDialog
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.SmoothProgress
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.design.progressLabel
import io.filebeam.android.ui.feature.account.LinkRow
import io.filebeam.android.ui.feature.send.phaseLabel
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalHost
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.ScreenHeader
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.Tab
import io.filebeam.android.ui.shell.TabScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.JobState
import io.filebeam.rust.SavedTransfer
import kotlinx.coroutines.launch
import java.io.File

/** What is moving right now, what is saved on this device, and the way into account history. */
@Composable
fun TransfersScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val coordinator = app.app.transfers
    val transfer by coordinator.state.collectAsStateCompat()
    val account by app.account.collectAsStateCompat()
    var selected by remember { mutableStateOf<SavedTransfer?>(null) }

    LaunchedEffect(Unit) { coordinator.refresh() }

    TabScreen {
        item(key = "header") { Staggered(0) { ScreenHeader(stringResource(R.string.tab_transfers), subtitle = stringResource(R.string.transfers_subtitle)) } }
        val active = transfer.busy || transfer.snapshot?.state == JobState.PAUSED
        if (active) {
            item(key = "active") { Staggered(1) { ActiveCard(transfer, onPause = coordinator::pause, onResume = coordinator::resumePending, onOpen = { navigator.select(if (isSend(transfer)) Tab.Send else Tab.Receive) }) } }
        }
        if (transfer.saved.isNotEmpty()) {
            item(key = "saved-label") { SectionLabel(stringResource(R.string.transfers_on_device)) }
            items(transfer.saved, key = { it.id }) { saved ->
                SavedRow(saved, Modifier.animateItem(fadeInSpec = Motion.enter(), placementSpec = Motion.spatial(), fadeOutSpec = Motion.exit())) { selected = saved }
            }
        }
        if (!active && transfer.saved.isEmpty()) {
            item(key = "empty") {
                EmptyState(R.drawable.ic_approved_transfers, stringResource(R.string.transfers_empty_title), body = stringResource(R.string.transfers_empty_body)) {
                    FbButton(stringResource(R.string.history_send), { navigator.select(Tab.Send) }, icon = R.drawable.ic_approved_upload)
                }
            }
        }
        item(key = "history") {
            Staggered(2) {
                Panel(Modifier.fillMaxWidth()) {
                    LinkRow(
                        R.drawable.ic_approved_clock, stringResource(R.string.history_title),
                        { navigator.push(if (account != null) Route.History else Route.Auth()) },
                        detail = if (account == null) stringResource(R.string.transfers_history_sign_in) else null,
                    )
                    Hairline()
                    LinkRow(R.drawable.ic_approved_storage, stringResource(R.string.storage_title), { navigator.push(Route.Storage) })
                }
            }
        }
    }
    selected?.let { SavedSheet(it) { selected = null } }
}

private fun isSend(transfer: TransferUiState) = transfer.current?.kind == TransferKind.UPLOAD || transfer.current?.kind == TransferKind.NOTE_LIVE || transfer.snapshot?.direction == "upload"

@Composable
private fun ActiveCard(transfer: TransferUiState, onPause: () -> Unit, onResume: () -> Unit, onOpen: () -> Unit) {
    val palette = Fb.palette
    val snapshot = transfer.snapshot
    val fraction = snapshot?.total?.takeIf { it > 0uL }?.let { (snapshot.done.toDouble() / it.toDouble()).toFloat() }
    Panel(Modifier.fillMaxWidth().clip(Shapes.panel).clickable(role = Role.Button, onClick = onOpen), border = palette.choiceBorder.copy(alpha = 0.6f)) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                Box(Modifier.size(44.dp).clip(Shapes.control), contentAlignment = Alignment.Center) {
                    if (transfer.busy) Spinner(size = 26.dp) else FbIcon(R.drawable.ic_approved_clock, tint = palette.accentText, size = 26.dp)
                }
                Column(Modifier.weight(1f)) {
                    AnimatedContent(phaseLabel(transfer.phase), transitionSpec = { fadeIn(Motion.enter()) togetherWith fadeOut(Motion.exit()) }, label = "phase") { label ->
                        FbText(label, style = Fb.type.bodyStrong, maxLines = 1)
                    }
                    FbText(
                        snapshot?.fileName?.takeIf(String::isNotBlank) ?: stringResource(if (isSend(transfer)) R.string.transfers_sending else R.string.transfers_receiving),
                        style = Fb.type.small, color = palette.textMuted, maxLines = 1,
                    )
                }
                if (fraction != null) FbText(progressLabel(fraction), style = Fb.type.numeric, color = palette.textMuted)
            }
            SmoothProgress(if (transfer.busy) fraction else fraction ?: 0f)
            if (snapshot?.prompt != null) Notice(stringResource(R.string.transfers_needs_input), tone = Tone.Warning)
            snapshot?.peerWarning?.let { Notice(it, tone = Tone.Warning) }
            if (snapshot != null && snapshot.total != null) {
                FbText(stringResource(R.string.transfers_bytes, formatBytes(snapshot.done.toLong()), formatBytes(snapshot.total!!.toLong())), style = Fb.type.caption, color = palette.textSubtle)
            }
            if (transfer.busy) FbButton(stringResource(R.string.pause), onPause, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, enabled = transfer.current?.actions?.pause != false)
            else FbButton(stringResource(R.string.transfers_resume), onResume, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_arrow_right)
        }
    }
}

@Composable
private fun SavedRow(saved: SavedTransfer, modifier: Modifier, onClick: () -> Unit) {
    val palette = Fb.palette
    val upload = saved.direction == "upload"
    val fraction = if (saved.total > 0uL) (saved.done.toDouble() / saved.total.toDouble()).toFloat() else null
    Panel(modifier.fillMaxWidth().clip(Shapes.card).clickable(role = Role.Button, onClick = onClick), shape = Shapes.card) {
        Row(Modifier.padding(Space.md), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            Box(Modifier.size(44.dp).clip(Shapes.control), contentAlignment = Alignment.Center) {
                FbIcon(if (upload) R.drawable.ic_approved_upload else R.drawable.ic_approved_download, tint = palette.accentText, size = 24.dp)
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                FbText(stringResource(if (upload) R.string.transfers_saved_sent else R.string.transfers_saved_received), style = Fb.type.bodyStrong, maxLines = 1)
                FbText(
                    if (saved.total > 0uL) stringResource(R.string.transfers_bytes, formatBytes(saved.done.toLong()), formatBytes(saved.total.toLong())) else stringResource(R.string.send_size_unknown),
                    style = Fb.type.small, color = palette.textMuted, maxLines = 1,
                )
                if (fraction != null && fraction < 1f) SmoothProgress(fraction, height = 4.dp)
            }
            StatusPill(savedStateLabel(saved.state), tone = savedTone(saved.state))
        }
    }
}

@Composable
private fun savedStateLabel(state: String) = stringResource(
    when (state.lowercase()) {
        "complete", "completed" -> R.string.phase_complete
        "paused" -> R.string.phase_paused
        "failed" -> R.string.phase_failed
        "live" -> R.string.history_status_live
        else -> R.string.transfers_state_saved
    },
)

private fun savedTone(state: String) = when (state.lowercase()) {
    "complete", "completed" -> Tone.Success
    "failed" -> Tone.Danger
    "paused" -> Tone.Warning
    else -> Tone.Neutral
}

/** Details and actions for one saved transfer. Only actions the checkpoint still supports are offered. */
@Composable
private fun SavedSheet(saved: SavedTransfer, onDismiss: () -> Unit) {
    val app = LocalAppModel.current
    val host = LocalHost.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val coordinator = app.app.transfers
    var details by remember { mutableStateOf<SavedTransferPresentation?>(null) }
    var files by remember { mutableStateOf<List<File>>(emptyList()) }
    var error by remember { mutableStateOf<String?>(null) }
    var confirm by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(saved.id) {
        coordinator.savedDetails(saved.id) { result ->
            result.onSuccess { details = it }.onFailure { error = it.message }
        }
        files = coordinator.savedOutputs(saved.id)
    }
    FbSheet(onDismiss, title = stringResource(if (saved.direction == "upload") R.string.transfers_saved_sent else R.string.transfers_saved_received)) {
        val loaded = details
        error?.let { Notice(it, tone = Tone.Danger) }
        if (loaded == null && error == null) Box(Modifier.fillMaxWidth().padding(Space.xl), contentAlignment = Alignment.Center) { Spinner() }
        if (loaded != null) {
            val info = loaded.details
            Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                FbText(
                    listOf(
                        stringResource(if (info.kind.startsWith("note")) R.string.history_note else R.string.send_mode_files),
                        stringResource(if (info.transport == "webrtc") R.string.history_live_webrtc else R.string.history_stored_http),
                        if (info.total > 0uL) formatBytes(info.total.toLong()) else null,
                    ).filterNotNull().joinToString(" · "),
                    style = Fb.type.body,
                )
                parseInstant(info.expiresAt)?.let { FbText(stringResource(R.string.history_expires, formatAbsolute(it)), style = Fb.type.small, color = Fb.palette.textMuted) }
                if (info.verifiedPrivately) StatusPill(stringResource(R.string.receive_verified), tone = Tone.Success, icon = R.drawable.ic_approved_shield)
            }
            files.forEach { file ->
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                    FbIcon(R.drawable.ic_approved_file)
                    Column(Modifier.weight(1f)) {
                        FbText(file.name, style = Fb.type.bodyStrong, maxLines = 1)
                        FbText(formatBytes(file.length()), style = Fb.type.small, color = Fb.palette.textMuted)
                    }
                    FbButton(stringResource(R.string.receive_save), {
                        host.createDocument(file.name, "application/octet-stream") { uri -> if (uri != null) coordinator.export(file.absolutePath, uri) }
                    }, style = ButtonStyle.Secondary, height = 40.dp)
                }
            }
            val actions = loaded.actions
            if (actions.resume) FbButton(stringResource(R.string.transfers_resume), {
                host.withNotifications { coordinator.resume(saved.id) }
                onDismiss()
                navigator.select(Tab.Transfers)
            }, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_arrow_right)
            if (actions.endLive) FbButton(stringResource(R.string.share_stop_live), { confirm = "end" }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary)
            if (actions.revokeRemote) FbButton(stringResource(R.string.share_delete_now), { confirm = "revoke" }, Modifier.fillMaxWidth(), style = ButtonStyle.Danger, icon = R.drawable.ic_approved_trash)
            if (actions.removeLocal) FbButton(stringResource(R.string.transfers_remove_local), { confirm = "remove" }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
        }
    }
    when (confirm) {
        "revoke" -> FbDialog({ confirm = null }, stringResource(R.string.share_delete_title), stringResource(R.string.share_delete_now), {
            coordinator.revoke(saved.id)
            confirm = null
            onDismiss()
        }, body = stringResource(R.string.share_delete_body), dismissLabel = stringResource(R.string.cancel), destructive = true)
        "end" -> FbDialog({ confirm = null }, stringResource(R.string.share_stop_title), stringResource(R.string.share_stop_live), {
            coordinator.endLive(saved.id)
            confirm = null
            onDismiss()
        }, body = stringResource(R.string.share_stop_body), dismissLabel = stringResource(R.string.cancel), destructive = true)
        "remove" -> FbDialog({ confirm = null }, stringResource(R.string.transfers_remove_title), stringResource(R.string.transfers_remove_local), {
            coordinator.discard(saved.id)
            confirm = null
            onDismiss()
            scope.launch { app.toast(resources.getString(R.string.transfers_removed), Tone.Success) }
        }, body = stringResource(R.string.transfers_remove_body), dismissLabel = stringResource(R.string.cancel), destructive = true)
    }
}
