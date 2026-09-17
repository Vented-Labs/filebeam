package io.filebeam.android.ui

import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.ReceiptState
import io.filebeam.android.platform.SavedTransferPresentation
import io.filebeam.android.platform.TransferUiState
import io.filebeam.android.ui.design.ActionDock
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.EmptyErrorState
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import io.filebeam.rust.JobState

@Composable
fun TransfersScreen(model: FilebeamViewModel, state: TransferUiState, start: (() -> Unit) -> Unit, saveFile: (String) -> Unit, retrySave: (String) -> Unit) {
    var history by remember { mutableStateOf(false) }
    var direction by remember { mutableStateOf<String?>(null) }
    var expandedId by remember { mutableStateOf<String?>(null) }
    var confirmation by remember { mutableStateOf<TransferConfirmation?>(null) }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
        Text(stringResource(R.string.transfers), style = MaterialTheme.typography.headlineMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
            FilterChip(!history, { history = false }, label = { Text(stringResource(R.string.active_transfers)) })
            FilterChip(history, { history = true }, label = { Text(stringResource(R.string.transfer_history)) })
        }
        if (history) Row(horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
            FilterChip(direction == null, { direction = null }, label = { Text(stringResource(R.string.all_transfers)) })
            FilterChip(direction == "send", { direction = "send" }, label = { Text(stringResource(R.string.sent_transfers)) })
            FilterChip(direction == "receive", { direction = "receive" }, label = { Text(stringResource(R.string.received_transfers)) })
        }
        if (!history) {
            if (state.busy || state.snapshot != null) TransferReceiptContent(state, model::pauseTransfer, saveFile)
            else EmptyErrorState(stringResource(R.string.no_active_transfers), stringResource(R.string.no_active_transfers_detail),
                stringResource(R.string.transfer_history), false, { history = true })
            if (state.pending && !state.busy) OutlinedButton(onClick = { start(model::resumePendingTransfer) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.resume)) }
        } else if (state.saved.isEmpty()) {
            EmptyErrorState(stringResource(R.string.no_transfers), stringResource(R.string.transfer_history_detail),
                stringResource(R.string.active_transfers), false, { history = false })
        } else state.saved.filter { direction == null || it.direction.equals(direction, ignoreCase = true) || (direction == "send" && it.direction.equals("upload", ignoreCase = true)) || (direction == "receive" && it.direction.equals("download", ignoreCase = true)) }.forEach { saved ->
            val expanded = expandedId == saved.id
            var details by remember(saved.id) { mutableStateOf<Result<SavedTransferPresentation>?>(null) }
            LaunchedEffect(expanded) {
                if (expanded && details == null) model.savedTransferDetails(saved.id) { details = it }
            }
            ProductionGroupCard(Modifier.fillMaxWidth()) {
                TextButton(onClick = { expandedId = if (expanded) null else saved.id }, modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
                        Text(saved.direction.replaceFirstChar(Char::uppercase), style = MaterialTheme.typography.titleMedium)
                        Text(saved.id, style = MaterialTheme.typography.labelMedium)
                    }
                    AssistChip(onClick = {}, label = { Text(saved.state) })
                }
                if (expanded) Column(Modifier.padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small),
                    verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
                    Text("${bytes(saved.done)} / ${bytes(saved.total)}", style = MaterialTheme.typography.bodyMedium)
                    when (val result = details) {
                        null -> Text(stringResource(R.string.loading), style = MaterialTheme.typography.bodySmall)
                        else -> if (result.isFailure) {
                            Text(result.exceptionOrNull()?.message ?: stringResource(R.string.unknown_error), color = MaterialTheme.colorScheme.error)
                        } else SavedDetailActions(result.getOrThrow(), model, start, retrySave) { confirmation = it }
                    }
                }
            }
        }
    }
    confirmation?.let { pending -> AlertDialog(
        onDismissRequest = { confirmation = null },
        title = { Text(stringResource(pending.title)) },
        text = { Text(stringResource(pending.detail)) },
        confirmButton = { TextButton(onClick = { pending.action(); confirmation = null }) { Text(stringResource(R.string.continue_action)) } },
        dismissButton = { TextButton(onClick = { confirmation = null }) { Text(stringResource(R.string.cancel)) } },
    ) }
}

@Composable
fun TransferReceiptContent(state: TransferUiState, pause: () -> Unit, saveFile: (String) -> Unit) {
    val snapshot = state.snapshot
    val clipboard = LocalClipboardManager.current
    ProductionGroupCard(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(FilebeamSpace.Large), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
            Text(stringResource(phaseLabel(state)), style = MaterialTheme.typography.titleLarge)
            snapshot?.fileName?.takeIf(String::isNotBlank)?.let { Text(it) }
            val total = snapshot?.total
            if (total != null && total > 0u) {
                LinearProgressIndicator(progress = { (snapshot.done.toDouble() / total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                Text("${bytes(snapshot.done)} / ${bytes(total)}")
            } else if (state.busy) {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                Text(stringResource(R.string.progress_size_unknown), style = MaterialTheme.typography.bodySmall)
            }
            snapshot?.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            snapshot?.peerWarning?.let { Text(it) }
            state.sharePresentation?.let { share ->
                Text(share.link, style = MaterialTheme.typography.bodySmall)
                share.separateKey?.let { key ->
                    Text(stringResource(R.string.share_key_review), style = MaterialTheme.typography.bodySmall)
                    TextButton(onClick = { clipboard.setText(AnnotatedString(key)) }) { Text(stringResource(R.string.copy_separate_key)) }
                }
            }
            val completed = state.phase == "complete" && !state.busy
            if (completed && state.receipt?.state == ReceiptState.VERIFIED_PRIVATE) {
                Text(stringResource(R.string.verified_on_device), color = MaterialTheme.colorScheme.primary)
                state.receipt.result?.let { source ->
                    ActionDock(stringResource(R.string.export_verified_file), !state.busy, { saveFile(source) }, icon = ApprovedIcon.Download)
                }
            }
            if (completed && state.receipt?.state == ReceiptState.SAF_EXPORTED) {
                Text(stringResource(R.string.saved_to_selected_location), color = MaterialTheme.colorScheme.primary)
            }
            if (state.current?.actions?.pause == true) TextButton(onClick = pause) { Text(stringResource(R.string.pause)) }
        }
    }
}

@Composable
private fun SavedDetailActions(detail: SavedTransferPresentation, model: FilebeamViewModel, start: (() -> Unit) -> Unit, retrySave: (String) -> Unit, confirm: (TransferConfirmation) -> Unit) {
    when (detail.receiptState) {
        ReceiptState.VERIFIED_PRIVATE -> Text(stringResource(R.string.verified_on_device), color = MaterialTheme.colorScheme.primary)
        ReceiptState.APP_PRIVATE_PUBLISHED -> Text(stringResource(R.string.published_in_private_output), style = MaterialTheme.typography.bodySmall)
        else -> Unit
    }
    Row(horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
        if (detail.actions.resume) TextButton(onClick = { start { model.resumeSavedTransfer(detail.details.id) } }) { Text(stringResource(R.string.resume)) }
        if (detail.actions.endLive) TextButton(onClick = { confirm(TransferConfirmation(R.string.end_live_share, R.string.confirm_end_live) { model.endSavedLiveTransfer(detail.details.id) }) }) { Text(stringResource(R.string.end_live_share)) }
        if (detail.actions.revokeRemote) TextButton(onClick = { confirm(TransferConfirmation(R.string.revoke_remote, R.string.confirm_revoke) { model.revokeSavedTransfer(detail.details.id) }) }) { Text(stringResource(R.string.revoke_remote)) }
        if (detail.actions.removeLocal) TextButton(onClick = { confirm(TransferConfirmation(R.string.remove_local, R.string.confirm_remove_local) { model.discardSavedTransfer(detail.details.id) }) }) { Text(stringResource(R.string.remove_local)) }
    }
    if (detail.actions.retrySafExport) {
        Text(stringResource(R.string.retry_save_requires_destination), style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        TextButton(onClick = { retrySave(detail.details.id) }) { Text(stringResource(R.string.export_verified_file)) }
    }
}

private data class TransferConfirmation(val title: Int, val detail: Int, val action: () -> Unit)

@Composable
fun phaseLabel(state: TransferUiState): Int {
    if (state.phase == "exporting") return R.string.phase_exporting
    val phase = when (state.snapshot?.state) {
        JobState.PAUSING -> "pausing"; JobState.PAUSED -> "paused"; JobState.COMPLETE -> "complete"; JobState.FAILED -> "failed"; else -> state.phase
    }
    return when (phase) {
        "preparing" -> R.string.phase_preparing; "archiving" -> R.string.phase_archiving; "encrypting" -> R.string.phase_encrypting
        "sending" -> R.string.phase_sending; "receiving" -> R.string.phase_receiving; "verifying" -> R.string.phase_verifying
        "finalizing" -> R.string.phase_finalizing; "waiting" -> R.string.phase_waiting; "unlocking" -> R.string.phase_unlocking
        "retrying" -> R.string.phase_retrying; "reconnecting" -> R.string.phase_reconnecting; "storing" -> R.string.phase_storing
        "pausing" -> R.string.phase_pausing; "paused" -> R.string.phase_paused; "complete" -> R.string.phase_complete; "failed" -> R.string.phase_failed
        else -> R.string.phase_connecting
    }
}

@Composable private fun bytes(value: ULong): String = Formatter.formatShortFileSize(LocalContext.current, value.coerceAtMost(Long.MAX_VALUE.toULong()).toLong())
