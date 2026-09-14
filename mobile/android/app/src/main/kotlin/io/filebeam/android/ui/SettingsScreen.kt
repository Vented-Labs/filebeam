package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.AppSettings

@Composable
fun SettingsScreen(config: AppSettings, save: (AppSettings) -> Unit, discover: (String) -> Unit, navigate: (Destination) -> Unit) {
    var instance by remember(config.instance) { mutableStateOf(config.instance) }
    var relay by remember(config.relayOnly) { mutableStateOf(config.relayOnly) }
    Text(stringResource(R.string.settings), style = MaterialTheme.typography.headlineSmall)
    OutlinedTextField(instance, { instance = it }, singleLine = true, label = { Text(stringResource(R.string.instance)) }, modifier = Modifier.fillMaxWidth())
    Row(verticalAlignment = Alignment.CenterVertically) {
        Switch(relay, { relay = it })
        Text(stringResource(R.string.relay_only), Modifier.padding(start = 12.dp))
    }
    Text(stringResource(R.string.relay_description), style = MaterialTheme.typography.bodySmall)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = { save(config.copy(instance = instance, relayOnly = relay)) }) { Text(stringResource(R.string.save_settings)) }
        TextButton(onClick = { discover(instance) }) { Text(stringResource(R.string.check_instance)) }
    }
    HorizontalDivider()
    Text(stringResource(R.string.appearance), style = MaterialTheme.typography.titleMedium)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Switch(config.dynamicColor, { save(config.copy(dynamicColor = it)) })
        Text(stringResource(R.string.dynamic_color), Modifier.padding(start = 12.dp))
    }
    Text(stringResource(R.string.native_description), style = MaterialTheme.typography.bodyMedium)
    HorizontalDivider()
    Text(stringResource(R.string.native_services), style = MaterialTheme.typography.titleMedium)
    Column {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = { navigate(Destination.Notes) }) { Text(stringResource(R.string.notes)) }
            TextButton(onClick = { navigate(Destination.Inbox) }) { Text(stringResource(R.string.inbox)) }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = { navigate(Destination.Turbo) }) { Text(stringResource(R.string.turbo)) }
            TextButton(onClick = { navigate(Destination.Account) }) { Text(stringResource(R.string.account)) }
        }
    }
}
