package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.Checkbox
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.material3.OutlinedTextField
import io.filebeam.android.R
import io.filebeam.rust.Transport

@Composable
fun SendScreen(model: FilebeamViewModel, busy: Boolean, pick: () -> Unit, pickTree: () -> Unit, send: () -> Unit) {
    val archiveLabel = stringResource(R.string.archive)
    val passwordLabel = stringResource(R.string.optional_password)
    Text(stringResource(R.string.send_title), style = MaterialTheme.typography.headlineSmall)
    Text(stringResource(R.string.send_description))
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(enabled = !busy, onClick = pick) { Text(stringResource(R.string.select_files)) }
        OutlinedButton(enabled = !busy, onClick = pickTree) { Text(stringResource(R.string.select_folder)) }
    }
    if (model.selectedFiles.isNotEmpty()) Text(pluralStringResource(R.plurals.files_selected, model.selectedFiles.size, model.selectedFiles.size))
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(model.transport == Transport.HTTP, { model.transport = Transport.HTTP }, enabled = !busy, label = { Text(stringResource(R.string.http_transport)) })
        FilterChip(model.transport == Transport.WEB_RTC, { model.transport = Transport.WEB_RTC }, enabled = !busy, label = { Text(stringResource(R.string.webrtc_transport)) })
    }
    if (model.transport == Transport.WEB_RTC) Text(stringResource(R.string.live_description), style = MaterialTheme.typography.bodyMedium)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(model.archive, { model.archive = it }, enabled = !busy, modifier = Modifier.semantics { contentDescription = archiveLabel })
        Text(archiveLabel)
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(model.passwordProtected, { model.passwordProtected = it }, enabled = !busy, modifier = Modifier.semantics { contentDescription = passwordLabel })
        Text(passwordLabel)
    }
    OutlinedTextField(model.retentionHours, { model.retentionHours = it.filter(Char::isDigit) }, enabled = !busy,
        label = { Text(stringResource(R.string.retention_hours)) }, modifier = Modifier.fillMaxWidth())
    Button(enabled = !busy && model.selectedFiles.isNotEmpty(), onClick = send, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.start_upload)) }
}
