package io.filebeam.android.ui

import android.os.StatFs
import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import io.filebeam.android.R
import io.filebeam.android.platform.storage.StorageUsage
import io.filebeam.android.platform.storage.StorageUsageBucket
import io.filebeam.android.platform.storage.StorageUsageScanner
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

@Composable
fun StorageUsageScreen(onReviewTransfers: () -> Unit) {
    val context = LocalContext.current
    val presentation by produceState<StorageUsagePresentation>(StorageUsagePresentation.Loading) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val usage = StorageUsageScanner.scan(File(context.noBackupFilesDir, "documents"), File(context.noBackupFilesDir, "transfers"))
                val filesystem = StatFs(context.noBackupFilesDir.absolutePath)
                StorageUsagePresentation.Ready(usage, filesystem.availableBytes, filesystem.totalBytes)
            }.getOrElse { StorageUsagePresentation.Error }
        }
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium),
        verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium),
    ) {
        Text(stringResource(R.string.storage_usage), style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.storage_categories), style = MaterialTheme.typography.bodyMedium)
        when (val value = presentation) {
            StorageUsagePresentation.Loading -> Text(stringResource(R.string.loading))
            StorageUsagePresentation.Error -> Text(stringResource(R.string.unknown_error), color = MaterialTheme.colorScheme.error)
            is StorageUsagePresentation.Ready -> {
                ProductionGroupCard(Modifier.fillMaxWidth()) {
                    UsageRow(R.string.source_snapshots, value.usage.snapshots)
                    UsageRow(R.string.encrypted_recovery, value.usage.recovery)
                    UsageRow(R.string.verified_data, value.usage.verified)
                    Text(stringResource(R.string.storage_available, Formatter.formatShortFileSize(context, value.available), Formatter.formatShortFileSize(context, value.total)), modifier = Modifier.padding(FilebeamSpace.Medium), style = MaterialTheme.typography.bodySmall)
                }
                TextButton(onClick = onReviewTransfers, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.review_transfers)) }
            }
        }
    }
}

@Composable
private fun UsageRow(title: Int, bucket: StorageUsageBucket) {
    val context = LocalContext.current
    Column(Modifier.fillMaxWidth().padding(FilebeamSpace.Medium)) {
        Text(stringResource(title), style = MaterialTheme.typography.titleMedium)
        Text(pluralStringResource(R.plurals.storage_files, bucket.files, bucket.files, Formatter.formatShortFileSize(context, bucket.bytes)), style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
fun ReviewTransfersScreen(model: FilebeamViewModel, start: (() -> Unit) -> Unit) {
    val state by model.transfers.collectAsState()
    val clipboard = LocalClipboardManager.current
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
        Text(stringResource(R.string.review_transfers), style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.review_transfers_detail), style = MaterialTheme.typography.bodyMedium)
        if (state.pending && !state.busy) TextButton(onClick = { start(model::resumePendingTransfer) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.resume)) }
        state.saved.forEach { transfer ->
            ProductionGroupCard(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(FilebeamSpace.Medium)) {
                    Text(transfer.direction.replaceFirstChar(Char::uppercase), style = MaterialTheme.typography.titleMedium)
                    Text(transfer.id, style = MaterialTheme.typography.bodySmall)
                    if (transfer.state.equals("paused", ignoreCase = true)) TextButton(onClick = { start { model.resumeSavedTransfer(transfer.id) } }) { Text(stringResource(R.string.resume)) }
                    TextButton(onClick = { clipboard.setText(AnnotatedString(transfer.id)) }) { Text(stringResource(R.string.copy_transfer_id)) }
                }
            }
        }
        if (state.saved.isEmpty()) Text(stringResource(R.string.no_transfers))
    }
}

private sealed interface StorageUsagePresentation {
    data object Loading : StorageUsagePresentation
    data class Ready(val usage: StorageUsage, val available: Long, val total: Long) : StorageUsagePresentation
    data object Error : StorageUsagePresentation
}
