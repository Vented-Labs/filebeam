package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.NoteRequest
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.platform.services.TurboDownloadRequest
import kotlinx.coroutines.launch

@Composable
fun AccountScreen(model: FilebeamViewModel, instance: String) {
    val state = model.accounts.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var key by remember { mutableStateOf("") }
    var feedback by remember { mutableStateOf<String?>(null) }
    Text(stringResource(R.string.account), style = MaterialTheme.typography.headlineSmall)
    when (state) {
        is ServiceState.Ready -> Text(stringResource(R.string.signed_in_as, state.value.username))
        is ServiceState.Loading -> Text(stringResource(R.string.loading))
        is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error)
        is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error)
    }
    OutlinedTextField(username, { username = it }, label = { Text(stringResource(R.string.account_username)) }, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.account_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(enabled = username.isNotBlank() && password.isNotBlank(), onClick = { scope.launch {
            runCatching { model.accounts.signIn(instance, username, password) }.onSuccess { password = "" }.onFailure { feedback = it.message }
        } }) { Text(stringResource(R.string.sign_in)) }
        TextButton(onClick = { scope.launch { runCatching { model.accounts.signOut() }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.sign_out)) }
    }
    Text(stringResource(R.string.account_key), style = MaterialTheme.typography.titleMedium)
    OutlinedTextField(key, { key = it }, label = { Text(stringResource(R.string.fbsk1_key)) }, modifier = Modifier.fillMaxWidth(), visualTransformation = PasswordVisualTransformation())
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = { scope.launch { runCatching { key = model.accounts.exportKey() }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.export_key)) }
        TextButton(enabled = key.startsWith("fbsk1."), onClick = { scope.launch { runCatching { model.accounts.importKey(key); key = "" }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.import_key)) }
    }
    feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

@Composable
fun NotesScreen(model: FilebeamViewModel, instance: String) {
    val state = model.notes.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    var body by remember { mutableStateOf("") }
    var link by remember { mutableStateOf("") }
    var burn by remember { mutableStateOf(false) }
    var viewer by remember { mutableStateOf<String?>(null) }
    var feedback by remember { mutableStateOf<String?>(null) }
    Text(stringResource(R.string.notes), style = MaterialTheme.typography.headlineSmall)
    OutlinedTextField(body, { body = it }, label = { Text(stringResource(R.string.note_body)) }, minLines = 5, modifier = Modifier.fillMaxWidth())
    val burnLabel = stringResource(R.string.burn_on_read)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(burn, { burn = it }, modifier = Modifier.semantics { contentDescription = burnLabel })
        Text(burnLabel)
    }
    Button(enabled = body.isNotBlank(), onClick = { scope.launch {
        runCatching { model.notes.create(NoteRequest(instance, body, burn)) }.onSuccess { body = "" }.onFailure { feedback = it.message }
    } }) { Text(stringResource(R.string.create_note)) }
    OutlinedTextField(link, { link = it }, label = { Text(stringResource(R.string.note_link)) }, modifier = Modifier.fillMaxWidth())
    Button(enabled = link.isNotBlank(), onClick = { scope.launch {
        runCatching { model.notes.claim(link) }.onSuccess { viewer = it.text }.onFailure { feedback = it.message }
    } }) { Text(stringResource(R.string.open_note)) }
    viewer?.let { OutlinedCard(Modifier.fillMaxWidth()) { Text(it) } }
    when (state) { is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error); is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error); else -> Unit }
    feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

@Composable
fun TurboScreen(model: FilebeamViewModel, instance: String) {
    val state = model.turbo.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    var transferId by remember { mutableStateOf("") }
    var feedback by remember { mutableStateOf<String?>(null) }
    Text(stringResource(R.string.turbo), style = MaterialTheme.typography.headlineSmall)
    Text(stringResource(R.string.turbo_description))
    OutlinedTextField(transferId, { transferId = it }, label = { Text(stringResource(R.string.turbo_transfer_id)) }, modifier = Modifier.fillMaxWidth())
    Button(enabled = transferId.isNotBlank(), onClick = { scope.launch { runCatching { model.turbo.refresh(instance, transferId) }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.check_availability)) }
    when (state) {
        is ServiceState.Ready -> Text(if (state.value.available) stringResource(R.string.turbo_available) else stringResource(R.string.turbo_not_available))
        is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error)
        is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error)
        is ServiceState.Loading -> Text(stringResource(R.string.loading))
    }
    Button(enabled = transferId.isNotBlank(), onClick = { scope.launch {
        runCatching { model.turbo.open(TurboDownloadRequest(instance, transferId)) }.onSuccess { feedback = it.id }.onFailure { feedback = it.message }
    } }) { Text(stringResource(R.string.open_turbo_download)) }
    feedback?.let { Text(it) }
}

@Composable
fun InboxScreen(model: FilebeamViewModel, instance: String) {
    val state = model.inbox.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    Text(stringResource(R.string.inbox), style = MaterialTheme.typography.headlineSmall)
    Button(onClick = { scope.launch { runCatching { model.inbox.refresh(instance) } } }) { Text(stringResource(R.string.refresh)) }
    when (state) {
        is ServiceState.Ready -> state.value.forEach { item ->
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(item.title, style = MaterialTheme.typography.titleMedium)
                    Text(item.id, style = MaterialTheme.typography.labelMedium)
                    TextButton(onClick = { model.receiveLink(item.link) }) { Text(stringResource(R.string.open_transfer)) }
                }
            }
        }
        is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error)
        is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error)
        is ServiceState.Loading -> Text(stringResource(R.string.loading))
    }
}
