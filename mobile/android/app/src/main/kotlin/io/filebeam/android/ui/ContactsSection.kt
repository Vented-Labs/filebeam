package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import io.filebeam.android.platform.services.ContactsState
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import kotlinx.coroutines.launch
import org.json.JSONObject

@Composable
fun ContactsSection(model: FilebeamViewModel, instance: String) {
    var data by remember(instance) { mutableStateOf<ContactsState?>(null) }
    var username by remember(instance) { mutableStateOf("") }
    var error by remember(instance) { mutableStateOf<String?>(null) }
    var busy by remember(instance) { mutableStateOf(false) }
    var localReceiving by remember(instance) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun action(target: String, operation: String, canSend: Boolean? = null, auto: Boolean? = null) {
        if (busy) return
        busy = true
        scope.launch {
            runCatching { model.accounts.contactAction(target, operation, canSend, auto) }.onSuccess { data = it; error = null }.onFailure { error = it.message }
            busy = false
        }
    }
    LaunchedEffect(instance) {
        runCatching { model.accounts.contacts() }.onSuccess { data = it }.onFailure { error = it.message }
        runCatching { model.accounts.automaticReceiving(null) }.onSuccess { localReceiving = JSONObject(it).getBoolean("enabled") }
    }
    ProductionGroupCard(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
            Text("Contacts and receiving", style = MaterialTheme.typography.titleLarge)
            Text("Mutual friends on this instance. Overrides control your incoming files.")
            OutlinedTextField(username, { username = it }, label = { Text("Exact @username") }, modifier = Modifier.fillMaxWidth())
            Button(enabled = !busy && username.isNotBlank(), onClick = { action(username, "request") }) { Text("Send friend request") }
            data?.let { state ->
                TextButton(enabled = !busy, onClick = {
                    val policies = listOf("anyone", "authenticated", "friends", "nobody")
                    val next = policies[(policies.indexOf(state.policy) + 1) % policies.size]
                    scope.launch { runCatching { model.accounts.receivingDefaults(next, state.autoDownloadFriends); model.accounts.contacts() }.onSuccess { data = it }.onFailure { error = it.message } }
                }) { Text("Who can send: ${state.policy} (change)") }
                Row { Text("Automatically download from friends", Modifier.weight(1f)); Switch(state.autoDownloadFriends, { enabled -> scope.launch { runCatching { model.accounts.receivingDefaults(state.policy, enabled); model.accounts.contacts() }.onSuccess { data = it }.onFailure { error = it.message } } }) }
                Row { Text("Stage eligible deliveries on this device", Modifier.weight(1f)); Switch(localReceiving, { enabled -> scope.launch { runCatching { model.accounts.automaticReceiving(enabled) }.onSuccess { localReceiving = enabled }.onFailure { error = it.message } } }) }
                state.contacts.forEach { contact ->
                    Text("@${contact.username} — ${contact.status}", style = MaterialTheme.typography.titleMedium)
                    when (contact.status) {
                        "incoming" -> Row { TextButton(enabled = !busy, onClick = { action(contact.username, "accept") }) { Text("Accept") }; TextButton(enabled = !busy, onClick = { action(contact.username, "decline") }) { Text("Decline") } }
                        "outgoing" -> TextButton(enabled = !busy, onClick = { action(contact.username, "cancel") }) { Text("Cancel request") }
                        else -> {
                            TextButton(enabled = !busy, onClick = { action(contact.username, "preferences", nextOverride(contact.canSend), contact.autoDownload) }) { Text("Can send: ${overrideLabel(contact.canSend)} (change)") }
                            TextButton(enabled = !busy, onClick = { action(contact.username, "preferences", contact.canSend, nextOverride(contact.autoDownload)) }) { Text("Auto-download: ${overrideLabel(contact.autoDownload)} (change)") }
                            TextButton(onClick = { model.recipientUsername = contact.username; model.validateRecipient(contact.id.toULong()); model.navigate(Destination.Send) }) { Text("Send files") }
                            TextButton(enabled = !busy, onClick = { action(contact.username, "remove") }) { Text("Remove friend") }
                        }
                    }
                    TextButton(enabled = !busy, onClick = { action(contact.username, "block") }) { Text("Block") }
                }
                state.blocked.forEach { target -> TextButton(enabled = !busy, onClick = { action(target, "unblock") }) { Text("Unblock @$target") } }
            }
            error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        }
    }
}
private fun nextOverride(value: Boolean?): Boolean? = when (value) { null -> true; true -> false; false -> null }
private fun overrideLabel(value: Boolean?): String = when (value) { null -> "Inherit"; true -> "Allow"; false -> "Deny" }
