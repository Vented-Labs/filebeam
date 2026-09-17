package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.filebeam.android.BuildConfig
import io.filebeam.android.R
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.OptionRow
import io.filebeam.android.ui.design.ProductionGroupCard

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
    Column(
        Modifier.verticalScroll(rememberScrollState()).imePadding().padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium),
        verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium),
    ) {
        Text(stringResource(R.string.settings), style = MaterialTheme.typography.headlineMedium)
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
        SettingsSection(stringResource(R.string.appearance)) {
            OptionRow(stringResource(R.string.appearance), stringResource(R.string.appearance_system), ApprovedIcon.Settings, onClick = {})
        }
        SettingsSection(stringResource(R.string.storage)) {
            OptionRow(stringResource(R.string.manage_local_transfers), stringResource(R.string.storage_categories), ApprovedIcon.Folder, onClick = onManageTransfers)
            Text(stringResource(R.string.local_remote_difference), Modifier.padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        SettingsSection(stringResource(R.string.about)) {
            OptionRow(stringResource(R.string.version_name, BuildConfig.VERSION_NAME), stringResource(R.string.licenses), ApprovedIcon.File, onClick = {})
        }
    }
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        ProductionGroupCard(Modifier.fillMaxWidth()) { content() }
    }
}
