package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.ServiceState
import kotlinx.coroutines.launch

@Composable
fun AccountScreen(model: FilebeamViewModel, instance: String) {
    val state = model.accounts.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    var username by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var key by remember { mutableStateOf("") }
    var feedback by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(instance) { runCatching { model.accounts.resume(instance) }.onFailure { feedback = it.message } }
    Text(stringResource(R.string.account), style = MaterialTheme.typography.headlineSmall)
    when (state) {
        is ServiceState.Ready -> Text(stringResource(R.string.signed_in_as, state.value.username))
        is ServiceState.Loading -> Text(stringResource(R.string.loading))
        is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error)
        is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error)
    }
    OutlinedTextField(username, { username = it }, label = { Text(stringResource(R.string.account_username)) }, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(email, { email = it }, label = { Text(stringResource(R.string.account_email)) }, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.account_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(enabled = username.isNotBlank() && password.isNotBlank(), onClick = { scope.launch {
            runCatching { model.accounts.signIn(instance, username, password) }.onSuccess { password = "" }.onFailure { feedback = it.message }
        } }) { Text(stringResource(R.string.sign_in)) }
        TextButton(enabled = username.isNotBlank() && email.isNotBlank() && password.isNotBlank(), onClick = { scope.launch {
            runCatching { model.accounts.signUp(instance, username, name, email, password) }.onSuccess { password = "" }.onFailure { feedback = it.message }
        } }) { Text(stringResource(R.string.sign_up)) }
        TextButton(onClick = { scope.launch { runCatching { model.accounts.signOut() }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.sign_out)) }
    }
    Text(stringResource(R.string.account_key), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.account_key_caution), style = MaterialTheme.typography.bodySmall)
    OutlinedTextField(key, { key = it }, label = { Text(stringResource(R.string.fbsk1_key)) }, modifier = Modifier.fillMaxWidth(), visualTransformation = PasswordVisualTransformation())
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = { scope.launch { runCatching { key = model.accounts.generateKey() }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.generate_key)) }
        TextButton(onClick = { scope.launch { runCatching { key = model.accounts.exportKey() }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.export_key)) }
        TextButton(enabled = key.startsWith("fbsk1."), onClick = { scope.launch { runCatching { model.accounts.importKey(key); key = "" }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.import_key)) }
    }
    feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

@Composable
fun TurboScreen(model: FilebeamViewModel, instance: String) {
    var link by remember { mutableStateOf("") }
    Text(stringResource(R.string.turbo), style = MaterialTheme.typography.headlineSmall)
    Text(stringResource(R.string.turbo_description))
    OutlinedTextField(link, { link = it }, label = { Text(stringResource(R.string.turbo_descriptor)) }, modifier = Modifier.fillMaxWidth())
    Button(enabled = link.isNotBlank(), onClick = { model.receiveLink(link) }) { Text(stringResource(R.string.open_turbo_download)) }
}

@Composable
fun InboxScreen(model: FilebeamViewModel, instance: String) {
    val state = model.inbox.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    var password by remember { mutableStateOf("") }
    Text(stringResource(R.string.inbox), style = MaterialTheme.typography.headlineSmall)
    OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.account_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
    Button(onClick = { scope.launch { runCatching { model.inbox.refresh(instance, password.ifBlank { null }) }; password = "" } }) { Text(stringResource(R.string.refresh)) }
    when (state) {
        is ServiceState.Ready -> state.value.forEach { item ->
            OutlinedCard(Modifier.fillMaxWidth()) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(item.title, style = MaterialTheme.typography.titleMedium)
                    Text(item.id, style = MaterialTheme.typography.labelMedium)
                    item.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                    TextButton(enabled = item.link.isNotBlank(), onClick = {
                        model.coordinator.download(item.link)
                        model.destination = Destination.Transfers
                    }) { Text(stringResource(R.string.open_transfer)) }
                }
            }
        }
        is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error)
        is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error)
        is ServiceState.Loading -> Text(stringResource(R.string.loading))
    }
}
