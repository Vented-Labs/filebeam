package io.filebeam.android.ui

import android.content.Intent
import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.TransferUiState
import io.filebeam.rust.JobState
import java.io.File

@Composable
fun TransfersScreen(model: FilebeamViewModel, state: TransferUiState, start: (() -> Unit) -> Unit, saveFile: (String) -> Unit) {
    Text(stringResource(R.string.transfers), style = MaterialTheme.typography.headlineSmall)
    if (state.busy || state.snapshot != null) TransferCard(state, model.coordinator::pause, model.coordinator::endLive, model.coordinator::revoke, saveFile)
    if (state.pending && !state.busy) OutlinedButton(onClick = { start(model.coordinator::resumePending) }) { Text(stringResource(R.string.resume)) }
    if (state.saved.isEmpty() && !state.busy) Text(stringResource(R.string.no_transfers))
    state.saved.forEach { saved ->
        OutlinedCard(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(saved.id, style = MaterialTheme.typography.labelMedium)
                Text("${bytes(saved.done)} / ${bytes(saved.total)}")
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(enabled = !state.busy, onClick = { start { model.coordinator.resume(saved.id) } }) { Text(stringResource(R.string.resume)) }
                    TextButton(enabled = !state.busy, onClick = { model.coordinator.discard(saved.id) }) { Text(stringResource(R.string.remove_local)) }
                }
            }
        }
    }
}

@Composable
private fun TransferCard(state: TransferUiState, pause: () -> Unit, endLive: (String) -> Unit, revoke: (String) -> Unit, saveFile: (String) -> Unit) {
    val context = LocalContext.current
    val snapshot = state.snapshot
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(phaseLabel(state)), style = MaterialTheme.typography.titleLarge)
            snapshot?.fileName?.takeIf(String::isNotBlank)?.let { Text(it) }
            val total = snapshot?.total
            if (total != null && total > 0u) {
                LinearProgressIndicator(progress = { (snapshot.done.toDouble() / total.toDouble()).toFloat().coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                Text("${bytes(snapshot.done)} / ${bytes(total)}")
            } else if (state.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            snapshot?.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            snapshot?.peerWarning?.let { Text(it) }
            val link = snapshot?.shareUrl ?: snapshot?.results?.firstOrNull { it.startsWith("https://") || it.startsWith("http://") }
            if (link != null) Button(onClick = {
                context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, link) }, null))
            }) { Text(stringResource(R.string.share_link)) }
            if (snapshot?.state == JobState.COMPLETE) snapshot.results.filter { it.startsWith('/') }.forEach { path ->
                OutlinedButton(onClick = { saveFile(path) }) { Text(stringResource(R.string.save_file, File(path).name)) }
            }
            if (state.busy) TextButton(onClick = pause) { Text(stringResource(R.string.pause)) }
            snapshot?.checkpointId?.let { checkpoint ->
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = { endLive(checkpoint) }) { Text(stringResource(R.string.end_live_share)) }
                    TextButton(onClick = { revoke(checkpoint) }) { Text(stringResource(R.string.revoke_remote)) }
                }
            }
        }
    }
}

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
