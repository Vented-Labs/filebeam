package io.filebeam.android.ui

import android.os.StatFs
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.BuildConfig
import io.filebeam.android.R
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.OptionRow
import io.filebeam.android.ui.design.ProductionGroupCard
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Model adapter only: the production content below is also used by visual fixtures. */
@Composable
fun SettingsScreen(model: FilebeamViewModel, navigate: (Destination) -> Unit) = SettingsContent(
    transaction = model.instanceTransaction,
    onInstanceChanged = { model.updateInstanceInput(instance = it) },
    onRelayChanged = { model.updateInstanceInput(relayOnly = it) },
    onCheck = model::checkInstanceInput,
    onCommit = model::commitCheckedInstance,
    onConfirmActiveTransfer = model::confirmInstanceChangeAfterActiveTransfer,
    onManageTransfers = { navigate(Destination.Transfers) },
)

@Composable
fun SettingsContent(
    transaction: InstanceSettingsTransaction,
    onInstanceChanged: (String) -> Unit,
    onRelayChanged: (Boolean) -> Unit,
    onCheck: () -> Unit,
    onCommit: () -> Unit,
    onConfirmActiveTransfer: () -> Unit,
    onManageTransfers: () -> Unit,
) {
    val checking = transaction.status is InstanceTransactionStatus.Checking
    val context = LocalContext.current
    var licensesOpen by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(false) }
    val storage by produceState<StoragePresentation>(StoragePresentation.Loading) {
        value = withContext(Dispatchers.IO) {
            runCatching {
                val fs = StatFs(context.filesDir.absolutePath)
                StoragePresentation.Ready(fs.availableBytes, fs.totalBytes)
            }.getOrElse { StoragePresentation.Error }
        }
    }
    Column(
        Modifier.verticalScroll(rememberScrollState()).imePadding().padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium),
        verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium),
    ) {
        Text(stringResource(R.string.settings), style = MaterialTheme.typography.headlineMedium)
        SettingsSection(stringResource(R.string.appearance)) {
            ReadOnlyDetailRow(stringResource(R.string.appearance_system)) {
                ColorSwatches(listOf(MaterialTheme.colorScheme.primary, MaterialTheme.colorScheme.primaryContainer, MaterialTheme.colorScheme.surface))
            }
        }
        SettingsSection(stringResource(R.string.settings_connection)) {
            OutlinedTextField(transaction.draftInstance, onInstanceChanged, singleLine = true, label = { Text(stringResource(R.string.instance)) }, modifier = Modifier.fillMaxWidth(), enabled = !checking)
            OptionRow(stringResource(R.string.relay_only), stringResource(R.string.relay_description), ApprovedIcon.Transfers, enabled = !checking, onClick = { onRelayChanged(!transaction.draftRelayOnly) }, trailing = { Switch(transaction.draftRelayOnly, null, enabled = !checking) })
            Button(enabled = transaction.dirty && !checking, onClick = onCheck, modifier = Modifier.fillMaxWidth()) { Text(stringResource(if (checking) R.string.checking_instance else R.string.check_instance)) }
            if (transaction.status is InstanceTransactionStatus.ReadyToCommit) Button(onClick = onCommit, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.save_settings)) }
            if (transaction.status is InstanceTransactionStatus.ConfirmActiveTransfer) {
                Text(stringResource(R.string.instance_error_active_transfer), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                Button(onClick = onConfirmActiveTransfer, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.confirm_instance_change)) }
            }
            (transaction.status as? InstanceTransactionStatus.Error)?.let { Text(stringResource(it.message), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
        }
        SettingsSection(stringResource(R.string.storage)) {
            Text(stringResource(R.string.storage_categories), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            val detail = when (storage) {
                StoragePresentation.Loading -> stringResource(R.string.loading)
                is StoragePresentation.Ready -> (storage as StoragePresentation.Ready).let { ready ->
                    "${android.text.format.Formatter.formatShortFileSize(context, ready.available)} available of ${android.text.format.Formatter.formatShortFileSize(context, ready.total)}"
                }
                StoragePresentation.Error -> stringResource(R.string.unknown_error)
            }
            OptionRow(stringResource(R.string.manage_local_transfers), detail, ApprovedIcon.Storage, onClick = onManageTransfers)
            Text(stringResource(R.string.local_remote_difference), Modifier.padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        SettingsSection(stringResource(R.string.about)) {
            OptionRow(stringResource(R.string.version_name, BuildConfig.VERSION_NAME), stringResource(R.string.licenses), ApprovedIcon.File, onClick = { licensesOpen = true })
        }
    }
    if (licensesOpen) LicenseNoticeDialog { licensesOpen = false }
}

private sealed interface StoragePresentation {
    data object Loading : StoragePresentation
    data class Ready(val available: Long, val total: Long) : StoragePresentation
    data object Error : StoragePresentation
}

@Composable private fun ReadOnlyDetailRow(detail: String, trailing: @Composable () -> Unit) {
    androidx.compose.foundation.layout.Row(Modifier.fillMaxWidth().padding(FilebeamSpace.Medium), horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
        Text(detail, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        trailing()
    }
}

@Composable private fun ColorSwatches(colors: List<Color>) {
    androidx.compose.foundation.layout.Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        colors.forEach { color -> androidx.compose.material3.Surface(Modifier.size(24.dp), color = color, shape = MaterialTheme.shapes.extraSmall) {} }
    }
}

@Composable private fun LicenseNoticeDialog(dismiss: () -> Unit) {
    val context = LocalContext.current
    val notices = androidx.compose.runtime.remember {
        runCatching { listOf("LICENSE", "icons-NOTICE").joinToString("\n\n") { context.assets.open(it).bufferedReader().use { reader -> reader.readText() } } }.getOrElse { "License notices are unavailable in this build." }
    }
    AlertDialog(onDismissRequest = dismiss, title = { Text(stringResource(R.string.licenses)) }, text = { Text(notices, style = MaterialTheme.typography.bodySmall) }, confirmButton = { Button(onClick = dismiss) { Text(stringResource(R.string.close)) } })
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        ProductionGroupCard(Modifier.fillMaxWidth()) { content() }
    }
}
