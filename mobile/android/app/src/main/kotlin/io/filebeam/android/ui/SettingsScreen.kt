package io.filebeam.android.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.BuildConfig

@Composable
fun SettingsScreen(model: FilebeamViewModel, navigate: (Destination) -> Unit) {
    val transaction = model.instanceTransaction
    androidx.compose.foundation.layout.Column(Modifier.verticalScroll(rememberScrollState()).imePadding()) {
    Text(stringResource(R.string.settings), style = MaterialTheme.typography.headlineSmall)
    Text(stringResource(R.string.settings_connection), style = MaterialTheme.typography.titleMedium)
    OutlinedTextField(transaction.draftInstance, { model.updateInstanceInput(instance = it) }, singleLine = true, label = { Text(stringResource(R.string.instance)) }, modifier = Modifier.fillMaxWidth(), enabled = transaction.status !is InstanceTransactionStatus.Checking)
    Switch(transaction.draftRelayOnly, { model.updateInstanceInput(relayOnly = it) }, enabled = transaction.status !is InstanceTransactionStatus.Checking)
    Text(stringResource(R.string.relay_only), Modifier.padding(top = 4.dp))
    Text(stringResource(R.string.relay_description), style = MaterialTheme.typography.bodySmall)
    Button(enabled = transaction.dirty && transaction.status !is InstanceTransactionStatus.Checking, onClick = model::checkInstanceInput) { Text(stringResource(if (transaction.status is InstanceTransactionStatus.Checking) R.string.checking_instance else R.string.check_instance)) }
    if (transaction.status is InstanceTransactionStatus.ReadyToCommit) Button(onClick = model::commitCheckedInstance) { Text(stringResource(R.string.save_settings)) }
    if (transaction.status is InstanceTransactionStatus.ConfirmActiveTransfer) { Text(stringResource(R.string.instance_error_active_transfer), color = MaterialTheme.colorScheme.error); Button(onClick = model::confirmInstanceChangeAfterActiveTransfer) { Text(stringResource(R.string.confirm_instance_change)) } }
    (transaction.status as? InstanceTransactionStatus.Error)?.let { Text(stringResource(it.message), color = MaterialTheme.colorScheme.error) }
    HorizontalDivider()
    Text(stringResource(R.string.appearance), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.appearance_system), style = MaterialTheme.typography.bodyMedium)
    HorizontalDivider()
    Text(stringResource(R.string.storage), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.storage_categories), style = MaterialTheme.typography.bodyMedium)
    TextButton(onClick = { navigate(Destination.Transfers) }) { Text(stringResource(R.string.manage_local_transfers)) }
    Text(stringResource(R.string.local_remote_difference), style = MaterialTheme.typography.bodySmall)
    HorizontalDivider()
    Text(stringResource(R.string.about), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.version_name, BuildConfig.VERSION_NAME))
    Text(stringResource(R.string.licenses))
    }
}
