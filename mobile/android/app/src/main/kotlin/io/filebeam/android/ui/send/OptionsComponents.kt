package io.filebeam.android.ui.send

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.filebeam.android.R
import io.filebeam.android.ui.NoteDraft
import io.filebeam.android.ui.SendDraft
import io.filebeam.android.ui.SendInstancePolicy
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.OptionRow
import io.filebeam.android.ui.design.ProductionGroupCard
import io.filebeam.rust.Transport

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TransferOptionsSheet(draft: SendDraft, policy: SendInstancePolicy, onUpdate: ((SendDraft) -> SendDraft) -> Unit, onDismiss: () -> Unit) {
    var edited by remember(draft) { mutableStateOf(draft) }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Small), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
            Text(stringResource(R.string.transfer_options), style = androidx.compose.material3.MaterialTheme.typography.headlineSmall)
            Text(stringResource(if (edited.transport == Transport.HTTP) R.string.for_http_transfer else R.string.for_live_transfer))
            ProductionGroupCard {
                if (edited.transport == Transport.HTTP) ToggleOption(stringResource(R.string.turbo_transfer), stringResource(R.string.turbo_detail), edited.turbo) { edited = edited.copy(turbo = it) }
                ToggleOption(stringResource(R.string.zip_title), stringResource(R.string.zip_detail), edited.archive) { edited = edited.copy(archive = it) }
                ToggleOption(stringResource(R.string.optional_password), stringResource(R.string.password_detail), edited.passwordProtected) { edited = edited.copy(passwordProtected = it) }
                OutlinedTextField(edited.retentionHours, { edited = edited.copy(retentionHours = it) }, label = { Text(stringResource(R.string.retention_hours)) }, supportingText = { Text(stringResource(if (validRetention(edited.retentionHours)) R.string.retention_default_detail else R.string.retention_invalid)) }, isError = !validRetention(edited.retentionHours), modifier = Modifier.fillMaxWidth().padding(FilebeamSpace.Medium))
                ToggleOption(stringResource(R.string.include_key), stringResource(R.string.separate_key_detail), edited.includeKeyInLink) { edited = edited.copy(includeKeyInLink = it) }
                policy.drivers.values.forEach { driver ->
                    ToggleOption(driver.name, stringResource(R.string.driver_detail), edited.driver == driver.name) { selected -> if (selected) edited = edited.copy(driver = driver.name) }
                }
            }
            Button(enabled = validRetention(edited.retentionHours), onClick = { onUpdate { edited }; onDismiss() }, modifier = Modifier.fillMaxWidth().padding(bottom = FilebeamSpace.Large)) { Text(stringResource(R.string.done)) }
        }
    }
}

@Composable
private fun ToggleOption(title: String, detail: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) =
    OptionRow(title, detail, selected = checked, onClick = { onCheckedChange(!checked) }, trailing = { Switch(checked = checked, onCheckedChange = null) })

@Composable
fun NoteOptionsCard(draft: NoteDraft, passwordEnabled: Boolean, enabled: Boolean, onClick: () -> Unit) = ProductionGroupCard {
    val password = stringResource(if (passwordEnabled) R.string.password_on else R.string.password_off)
    OptionRow(stringResource(R.string.note_options), "${draft.retentionHours.ifBlank { stringResource(R.string.default_retention) }} · $password", enabled = enabled, onClick = onClick)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NoteOptionsSheet(
    draft: NoteDraft,
    passwordEnabled: Boolean,
    onPasswordChange: (Boolean) -> Unit,
    onUpdate: ((NoteDraft) -> NoteDraft) -> Unit,
    onDismiss: () -> Unit,
) {
    var edited by remember(draft) { mutableStateOf(draft) }
    var password by remember(passwordEnabled) { mutableStateOf(passwordEnabled) }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Small), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
            Text(stringResource(R.string.note_options), style = androidx.compose.material3.MaterialTheme.typography.headlineSmall)
            ProductionGroupCard {
                ToggleOption(stringResource(R.string.note_password_title), stringResource(R.string.note_password_detail), password) { password = it }
                ToggleOption(stringResource(R.string.burn_on_read), stringResource(R.string.burn_detail), edited.burnAfterRead) { edited = edited.copy(burnAfterRead = it) }
                ToggleOption(stringResource(R.string.live_title), stringResource(R.string.live_detail), edited.live) { edited = edited.copy(live = it) }
                OutlinedTextField(edited.retentionHours, { edited = edited.copy(retentionHours = it) }, label = { Text(stringResource(R.string.retention_hours)) }, supportingText = { Text(stringResource(if (validRetention(edited.retentionHours)) R.string.retention_default_detail else R.string.retention_invalid)) }, isError = !validRetention(edited.retentionHours), modifier = Modifier.fillMaxWidth().padding(FilebeamSpace.Medium))
                ToggleOption(stringResource(R.string.include_key), stringResource(R.string.separate_key_detail), edited.includeKeyInLink) { edited = edited.copy(includeKeyInLink = it) }
            }
            Button(enabled = validRetention(edited.retentionHours), onClick = { onPasswordChange(password); onUpdate { edited }; onDismiss() }, modifier = Modifier.fillMaxWidth().padding(bottom = FilebeamSpace.Large)) { Text(stringResource(R.string.done)) }
        }
    }
}
