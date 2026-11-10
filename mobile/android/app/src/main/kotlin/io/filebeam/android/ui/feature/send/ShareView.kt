package io.filebeam.android.ui.feature.send

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PersistableBundle
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.TransferUiState
import io.filebeam.android.ui.common.formatAbsolute
import io.filebeam.android.ui.common.formatRelative
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbDialog
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.SmoothProgress
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.design.progressLabel
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.motion.rememberHaptics
import io.filebeam.android.ui.shell.InstanceProfile
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.TabScreen
import io.filebeam.rust.JobState
import io.filebeam.rust.TransferActivityRecord
import io.filebeam.rust.formatDownloadWithCli
import kotlinx.coroutines.delay
import java.time.Instant

/** Copies text, flagged as sensitive so keyboards and the system preview do not echo keys or links. */
fun copySensitive(context: Context, label: String, text: String) {
    val clip = ClipData.newPlainText(label, text)
    if (Build.VERSION.SDK_INT >= 33) {
        clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
    }
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
}

@Composable
internal fun phaseLabel(phase: String): String = stringResource(
    when (phase) {
        "connecting" -> R.string.phase_connecting
        "preparing" -> R.string.phase_preparing
        "archiving" -> R.string.phase_archiving
        "encrypting" -> R.string.phase_encrypting
        "sending" -> R.string.phase_sending
        "receiving" -> R.string.phase_receiving
        "verifying" -> R.string.phase_verifying
        "finalizing" -> R.string.phase_finalizing
        "waiting" -> R.string.phase_waiting
        "unlocking" -> R.string.phase_unlocking
        "retrying" -> R.string.phase_retrying
        "reconnecting" -> R.string.phase_reconnecting
        "storing" -> R.string.phase_storing
        "pausing" -> R.string.phase_pausing
        "paused" -> R.string.phase_paused
        "exporting" -> R.string.phase_exporting
        "failed" -> R.string.phase_failed
        "complete" -> R.string.phase_complete
        else -> R.string.phase_preparing
    },
)

/** The Send tab while a transfer started here is in flight or finished. */
@Composable
internal fun ShareView(model: SendViewModel, state: SendUiState, transfer: TransferUiState, profile: InstanceProfile) {
    val stage = state.stage as? SendStage.Sharing ?: return
    val app = LocalAppModel.current
    val context = LocalContext.current
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val note = stage.kind == ShareKind.Note || stage.kind == ShareKind.LiveNote
    val live = stage.kind == ShareKind.LiveFiles || stage.kind == ShareKind.LiveNote
    val snapshot = transfer.snapshot
    val link = if (note) state.createdNote?.link else transfer.sharePresentation?.link
    val separateKey = if (note) state.createdNote?.separateKey else transfer.sharePresentation?.separateKey
    val running = if (note && !live) state.noteBusy else transfer.busy
    val failed = !running && snapshot?.state == JobState.FAILED && !note
    val paused = !running && snapshot?.state == JobState.PAUSED && !note
    val complete = if (note) state.createdNote != null && (!live || !transfer.busy) else !running && snapshot?.state == JobState.COMPLETE
    val fraction = snapshot?.total?.takeIf { it > 0uL }?.let { (snapshot.done.toDouble() / it.toDouble()).toFloat() }
    val hasLink = link != null
    var confirm by remember { mutableStateOf<String?>(null) }
    var completedAt by remember { mutableLongStateOf(0L) }
    LaunchedEffect(complete) { if (complete && completedAt == 0L) completedAt = System.currentTimeMillis() }
    // One confirmation pulse when the link first appears.
    LaunchedEffect(hasLink) { if (hasLink) haptics.confirm() }
    if (hasLink) SecureWindowEffect()

    TabScreen {
        item(key = "share") {
            Staggered(0) {
                Panel(Modifier.fillMaxWidth()) {
                    Column(
                        Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.xxl),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(Space.lg),
                    ) {
                        ShareBadge(running = running, done = hasLink || complete, failed = failed)
                        if (stage.turbo || live) {
                            FbText(stringResource(if (stage.turbo) R.string.send_turbo else R.string.share_live_eyebrow).uppercase(), style = Fb.type.eyebrow, color = palette.accentText)
                        }
                        AnimatedContent(
                            when {
                                failed -> stringResource(R.string.share_failed_title)
                                paused -> stringResource(R.string.share_paused_title)
                                stage.kind == ShareKind.Inbox && complete -> stringResource(R.string.share_inbox_done_title)
                                stage.kind == ShareKind.Inbox -> stringResource(R.string.share_inbox_sending_title)
                                hasLink && live -> stringResource(R.string.share_live_title)
                                hasLink -> stringResource(R.string.share_ready_title)
                                else -> phaseLabel(transfer.phase.takeUnless { note && !live } ?: "encrypting")
                            },
                            transitionSpec = {
                                (fadeIn(Motion.enter()) + slideInVertically(Motion.spatial()) { it / 3 }) togetherWith (fadeOut(Motion.exit()) + slideOutVertically(Motion.exit()) { -it / 3 })
                            },
                            label = "title",
                        ) { title ->
                            FbText(title, Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }, style = Fb.type.headline, textAlign = TextAlign.Center)
                        }
                        val subtitle = when {
                            failed -> snapshot?.error ?: stringResource(R.string.share_failed_body)
                            paused -> stringResource(R.string.share_paused_body)
                            stage.kind == ShareKind.Inbox -> stringResource(R.string.share_inbox_body, state.files.recipient?.username.orEmpty())
                            live && hasLink -> stringResource(R.string.share_live_body)
                            stage.turbo && running && hasLink -> stringResource(R.string.share_turbo_body)
                            hasLink && separateKey != null && (state.files.password != null || state.note.password != null) -> stringResource(R.string.share_key_and_password_note)
                            hasLink && separateKey != null -> stringResource(R.string.share_key_separate_note)
                            hasLink -> stringResource(R.string.share_key_fragment_note)
                            else -> stringResource(R.string.share_preparing_body)
                        }
                        FbText(subtitle, style = Fb.type.small, color = if (failed) palette.danger.ink else palette.textMuted, textAlign = TextAlign.Center)

                        AnimatedVisibility(running && !(note && !live), enter = expandVertically(Motion.spatial()) + fadeIn(Motion.fade()), exit = shrinkVertically() + fadeOut(Motion.exit())) {
                            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                    FbText(phaseLabel(transfer.phase), Modifier.weight(1f), style = Fb.type.small, color = palette.textMuted, maxLines = 1)
                                    if (fraction != null) FbText(progressLabel(fraction), style = Fb.type.numeric, color = palette.textMuted)
                                }
                                SmoothProgress(if (live && hasLink) null else fraction)
                                if (snapshot != null && snapshot.fileCount > 1uL && snapshot.fileName.isNotEmpty()) {
                                    FbText(stringResource(R.string.share_file_progress, snapshot.fileIndex.toInt() + 1, snapshot.fileCount.toInt(), snapshot.fileName), style = Fb.type.caption, color = palette.textSubtle, maxLines = 1)
                                }
                            }
                        }

                        AnimatedVisibility(hasLink && stage.kind != ShareKind.Inbox, enter = expandVertically(Motion.spatialSlow()) + fadeIn(Motion.enter()) + scaleIn(Motion.spatialSlow(), initialScale = 0.96f), exit = fadeOut(Motion.exit())) {
                            Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
                                CopyField(R.drawable.ic_approved_link, link.orEmpty(), stringResource(R.string.share_copy_link), primary = true) {
                                    copySensitive(context, context.getString(R.string.share_link_label), link.orEmpty())
                                    app.toast(context.getString(R.string.share_link_copied), Tone.Success)
                                }
                                if (separateKey != null) {
                                    CopyField(R.drawable.ic_approved_key, separateKey, stringResource(R.string.share_copy_key), primary = false) {
                                        copySensitive(context, context.getString(R.string.share_key_label), separateKey)
                                        app.toast(context.getString(R.string.share_key_copied), Tone.Success)
                                    }
                                }
                                FbButton(stringResource(R.string.share_share), {
                                    context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link), null))
                                }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_arrow_up_right)
                            }
                        }

                        if (hasLink && stage.kind != ShareKind.Inbox && !live) {
                            val hours = (if (note) state.note.retentionHours ?: profile.noteRetention.defaultHours else state.files.retentionHours ?: profile.fileRetention.defaultHours)
                            if (complete && completedAt > 0L) ExpiryChip(Instant.ofEpochMilli(completedAt + hours * 3_600_000L))
                            else if (stage.turbo) FbText(stringResource(R.string.share_retention_starts), style = Fb.type.caption, color = palette.textSubtle, textAlign = TextAlign.Center)
                        }
                        if (note && state.note.burnOnRead && hasLink) StatusPill(stringResource(R.string.share_burn_pill), tone = Tone.Warning, icon = R.drawable.ic_approved_trash)
                    }
                }
            }
        }

        if (hasLink && stage.kind != ShareKind.Inbox) {
            item(key = "cli") { Staggered(1) { CliCard(link.orEmpty(), available = !note && !live, reason = if (note) R.string.cli_unavailable_notes else R.string.cli_unavailable_live) } }
        }
        val checkpoint = snapshot?.checkpointId
        if ((stage.turbo || live) && hasLink && checkpoint != null && !note) {
            item(key = "activity") { Staggered(2) { DownloadActivity(model, checkpoint, live) } }
        }
        item(key = "actions") {
            Staggered(3) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    when {
                        failed || paused -> {
                            FbButton(stringResource(R.string.share_back_to_composer), model::backToComposer, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_arrow_left)
                        }
                        running -> {
                            if (hasLink || complete) FbButton(stringResource(R.string.share_new_transfer), model::newTransfer, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_plus, enabled = !running || stage.turbo.not() && live.not())
                            FbButton(
                                stringResource(if (live) R.string.share_stop_live else R.string.share_cancel_upload),
                                { confirm = if (live) "stop" else "cancel" }, Modifier.fillMaxWidth(), style = ButtonStyle.Danger,
                            )
                        }
                        else -> {
                            FbButton(stringResource(if (stage.kind == ShareKind.Inbox) R.string.share_send_more else R.string.share_new_transfer), model::newTransfer, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_plus)
                            if (!note && checkpoint != null && stage.kind != ShareKind.Inbox) {
                                FbButton(stringResource(R.string.share_delete_now), { confirm = "delete" }, Modifier.fillMaxWidth(), style = ButtonStyle.Danger, icon = R.drawable.ic_approved_trash)
                            }
                        }
                    }
                }
            }
        }
    }

    when (confirm) {
        "delete" -> FbDialog(
            { confirm = null }, stringResource(R.string.share_delete_title), stringResource(R.string.share_delete_now),
            {
                snapshot?.checkpointId?.let(app.app.transfers::revoke)
                app.toast(context.getString(R.string.share_deleted), Tone.Success)
                confirm = null
                model.newTransfer()
            },
            body = stringResource(R.string.share_delete_body), dismissLabel = stringResource(R.string.cancel), destructive = true,
        )
        "cancel" -> FbDialog(
            { confirm = null }, stringResource(R.string.share_cancel_title), stringResource(R.string.share_cancel_upload),
            {
                confirm = null
                model.cancelUpload()
            },
            body = stringResource(R.string.share_cancel_body), dismissLabel = stringResource(R.string.share_keep_uploading), destructive = true,
        )
        "stop" -> FbDialog(
            { confirm = null }, stringResource(R.string.share_stop_title), stringResource(R.string.share_stop_live),
            {
                confirm = null
                if (note) model.endLiveNote() else snapshot?.checkpointId?.let(app.app.transfers::endLive) ?: model.cancelUpload()
            },
            body = stringResource(R.string.share_stop_body), dismissLabel = stringResource(R.string.cancel), destructive = true,
        )
    }
}

/** The round status emblem above the share title: a ring while working, a lock once protected. */
@Composable
private fun ShareBadge(running: Boolean, done: Boolean, failed: Boolean) {
    val palette = Fb.palette
    val scale by animateFloatAsState(if (done) 1f else 0.92f, motionOr(Motion.bouncy()), label = "badge")
    val border by animateColorAsState(if (failed) palette.danger.border else if (done) palette.actionBorder else palette.border, motionOr(Motion.fade()), label = "border")
    Box(
        Modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .size(76.dp)
            .clip(Shapes.pill)
            .background(Brush.radialGradient(listOf(palette.selectedSurface, palette.accentSurface)))
            .border(1.dp, border, Shapes.pill),
        contentAlignment = Alignment.Center,
    ) {
        AnimatedContent(
            when {
                failed -> 2
                done -> 1
                else -> 0
            },
            transitionSpec = { (scaleIn(Motion.bouncy(), initialScale = 0.4f) + fadeIn(Motion.fade())) togetherWith (scaleOut(Motion.exit(), targetScale = 0.6f) + fadeOut(Motion.exit())) },
            label = "badge-icon",
        ) { state ->
            when (state) {
                0 -> if (running) Spinner(size = 30.dp, stroke = 2.5.dp) else FbIcon(R.drawable.ic_approved_clock, tint = palette.accentText, size = 30.dp)
                1 -> FbIcon(R.drawable.ic_approved_lock, tint = palette.accentText, size = 32.dp)
                else -> FbIcon(R.drawable.ic_approved_alert, tint = palette.danger.ink, size = 30.dp)
            }
        }
    }
}

/** A read-only value with its copy action. */
@Composable
internal fun CopyField(icon: Int, value: String, action: String, primary: Boolean, onCopy: () -> Unit) {
    val palette = Fb.palette
    Row(
        Modifier.fillMaxWidth().clip(Shapes.control).background(palette.sunken).border(1.dp, palette.border, Shapes.control).padding(start = Space.md, top = Space.sm, bottom = Space.sm, end = Space.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        FbIcon(icon, size = 18.dp)
        FbText(value, Modifier.weight(1f), style = Fb.type.codeSmall, color = palette.textMuted, maxLines = 1)
        FbButton(action, onCopy, style = if (primary) ButtonStyle.Primary else ButtonStyle.Secondary, icon = R.drawable.ic_approved_copy, height = 40.dp)
    }
}

/** Shows when a transfer expires, refreshed every 30 seconds; the absolute time is its accessible description. */
@Composable
internal fun ExpiryChip(expiresAt: Instant) {
    val context = LocalContext.current
    var now by remember { mutableStateOf(Instant.now()) }
    LaunchedEffect(expiresAt) {
        while (true) {
            delay(30_000)
            now = Instant.now()
        }
    }
    val expired = !expiresAt.isAfter(now)
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        StatusPill(
            if (expired) stringResource(R.string.expiry_expired) else stringResource(R.string.expiry_expires, formatRelative(context, expiresAt, now)),
            tone = if (expired) Tone.Danger else Tone.Neutral, icon = R.drawable.ic_approved_clock,
        )
        FbText(formatAbsolute(expiresAt), style = Fb.type.caption, color = Fb.palette.textSubtle)
    }
}

/** The `beam down` command for people who prefer a terminal, or why there is none. */
@Composable
internal fun CliCard(link: String, available: Boolean, reason: Int) {
    val palette = Fb.palette
    val context = LocalContext.current
    val app = LocalAppModel.current
    val command = remember(link, available) { if (available) runCatching { formatDownloadWithCli(link) }.getOrNull() else null }
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                Box(Modifier.size(40.dp).clip(Shapes.control).background(palette.accentSurface), contentAlignment = Alignment.Center) {
                    FbIcon(R.drawable.ic_approved_terminal, tint = palette.accentText, size = 20.dp)
                }
                Column(Modifier.weight(1f)) {
                    FbText(stringResource(R.string.cli_title), style = Fb.type.bodyStrong)
                    FbText(stringResource(R.string.cli_subtitle), style = Fb.type.small, color = palette.textMuted)
                }
            }
            if (command != null) {
                CopyField(R.drawable.ic_approved_code, command, stringResource(R.string.copy), primary = false) {
                    copySensitive(context, context.getString(R.string.cli_title), command)
                    app.toast(context.getString(R.string.cli_copied), Tone.Success)
                }
            } else {
                FbText(stringResource(if (available) R.string.cli_unavailable else reason), style = Fb.type.small, color = palette.textMuted)
            }
        }
    }
}

/** Who is downloading a Turbo or live transfer right now. Polls gently and backs off when idle. */
@Composable
private fun DownloadActivity(model: SendViewModel, checkpoint: String, live: Boolean) {
    val palette = Fb.palette
    val coordinator = LocalAppModel.current.app.transfers
    var records by remember { mutableStateOf<List<TransferActivityRecord>>(emptyList()) }
    LaunchedEffect(checkpoint) {
        var wait = 2_000L
        while (true) {
            val next = coordinator.activity(checkpoint)
            wait = if (next == records) (wait * 3 / 2).coerceAtMost(15_000L) else 2_000L
            records = next
            delay(wait)
        }
    }
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                FbText(stringResource(if (live) R.string.activity_recipients else R.string.activity_title), Modifier.weight(1f), style = Fb.type.bodyStrong)
                FbText(stringResource(R.string.activity_anonymous), style = Fb.type.caption, color = palette.textMuted)
            }
            if (records.isEmpty()) {
                FbText(stringResource(R.string.activity_empty), style = Fb.type.small, color = palette.textMuted)
            } else {
                records.forEachIndexed { index, record ->
                    Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            FbText(stringResource(R.string.activity_download, (record.number ?: (index + 1).toULong()).toInt()), Modifier.weight(1f), style = Fb.type.smallStrong)
                            val selection = when {
                                record.allFiles == true -> stringResource(R.string.activity_all_files)
                                record.selectionCount != null -> pluralStringResource(R.plurals.activity_files, record.selectionCount!!.toInt(), record.selectionCount!!.toInt())
                                else -> null
                            }
                            if (selection != null) FbText(selection, Modifier.padding(end = Space.sm), style = Fb.type.caption, color = palette.textMuted)
                            StatusPill(
                                stringResource(
                                    when (record.state) {
                                        "downloading" -> R.string.activity_downloading
                                        "waiting" -> R.string.activity_waiting
                                        "verifying" -> R.string.activity_verifying
                                        "completed" -> R.string.activity_completed
                                        "cancelled" -> R.string.activity_cancelled
                                        "failed" -> R.string.activity_failed
                                        else -> R.string.activity_lost
                                    },
                                ),
                                tone = when (record.state) {
                                    "completed" -> Tone.Success
                                    "failed", "cancelled" -> Tone.Danger
                                    "downloading", "verifying" -> Tone.Accent
                                    else -> Tone.Neutral
                                },
                            )
                        }
                        record.progress?.let { SmoothProgress(it.toFloat(), height = 4.dp) }
                    }
                }
            }
        }
    }
}

private fun shrinkVertically() = androidx.compose.animation.shrinkVertically(Motion.spatialFast())
