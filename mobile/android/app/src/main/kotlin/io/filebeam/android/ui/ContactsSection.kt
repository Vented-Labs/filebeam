package io.filebeam.android.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.filebeam.android.platform.services.ContactsState
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.OptionRow
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
            runCatching { model.accounts.contactAction(target, operation, canSend, auto) }
                .onSuccess { data = it; error = null }.onFailure { error = it.message }
            busy = false
        }
    }
    fun defaults(policy: String, automatic: Boolean) {
        if (busy) return
        busy = true
        scope.launch {
            runCatching { model.accounts.receivingDefaults(policy, automatic); model.accounts.contacts() }
                .onSuccess { data = it; error = null }.onFailure { error = it.message }
            busy = false
        }
    }
    LaunchedEffect(instance) {
        runCatching { model.accounts.contacts() }.onSuccess { data = it }.onFailure { error = it.message }
        runCatching { model.accounts.automaticReceiving(null) }.onSuccess { localReceiving = JSONObject(it).getBoolean("enabled") }
    }
    Text("Contacts", style = MaterialTheme.typography.headlineSmall)
    ProductionGroupCard(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
            ContactSectionHeading("Add a friend", "Use an exact username on this instance.", ApprovedIcon.Add)
            OutlinedTextField(username, { username = it }, label = { Text("Exact @username") }, enabled = !busy, singleLine = true, modifier = Modifier.fillMaxWidth())
            Button(enabled = !busy && username.isNotBlank(), onClick = { action(username, "request") }, modifier = Modifier.fillMaxWidth()) {
                ApprovedIcon(ApprovedIcon.Add, null); Text("Send friend request", Modifier.padding(start = FilebeamSpace.XSmall))
            }
        }
    }
    data?.let { state ->
        ProductionGroupCard(Modifier.fillMaxWidth()) {
            ContactSectionHeading("Receiving defaults", "Per-contact overrides apply to incoming files.", ApprovedIcon.Shield, Modifier.padding(FilebeamSpace.Medium))
            ContactChoiceRow("Who can send me files", policyLabel(state.policy), listOf("anyone" to "Anyone, including anonymous", "authenticated" to "Signed-in users", "friends" to "Friends only", "nobody" to "Nobody unless allowed"), !busy) { defaults(it, state.autoDownloadFriends) }
            OptionRow("Automatically download from friends", "Eligible files stay encrypted in private staging.", ApprovedIcon.Download, enabled = !busy, onClick = { defaults(state.policy, !state.autoDownloadFriends) }, trailing = { Switch(state.autoDownloadFriends, { defaults(state.policy, it) }, enabled = !busy) })
            OptionRow("Stage files on this device", "Background timing is controlled by Android.", ApprovedIcon.Storage, enabled = !busy, onClick = {
                scope.launch { runCatching { model.accounts.automaticReceiving(!localReceiving) }.onSuccess { localReceiving = !localReceiving }.onFailure { error = it.message } }
            }, trailing = { Switch(localReceiving, { enabled -> scope.launch { runCatching { model.accounts.automaticReceiving(enabled) }.onSuccess { localReceiving = enabled }.onFailure { error = it.message } } }, enabled = !busy) })
        }
        if (state.contacts.isEmpty()) ProductionGroupCard(Modifier.fillMaxWidth()) {
            ContactSectionHeading("No friends or requests yet", "Add a username to get started.", ApprovedIcon.Folder, Modifier.padding(FilebeamSpace.Large))
        }
        state.contacts.forEach { contact ->
            ProductionGroupCard(Modifier.fillMaxWidth()) {
                ContactSectionHeading("@${contact.username}", when (contact.status) { "incoming" -> "Incoming friend request"; "outgoing" -> "Waiting for acceptance"; else -> "Friend on this instance" }, ApprovedIcon.Shield, Modifier.padding(FilebeamSpace.Medium))
                if (contact.status == "accepted") {
                    ContactChoiceRow("Can send me files", overrideLabel(contact.canSend), overrideChoices, !busy) { action(contact.username, "preferences", overrideValue(it), contact.autoDownload) }
                    ContactChoiceRow("Automatic download", overrideLabel(contact.autoDownload), listOf("inherit" to "Inherit account default", "allow" to "On", "deny" to "Off"), !busy) { action(contact.username, "preferences", contact.canSend, overrideValue(it)) }
                    Text(if (contact.effectiveDownload) "Eligible clients will stage incoming files privately." else "Files wait for a manual download.", Modifier.padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.XSmall), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Column(Modifier.padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
                        when (contact.status) {
                            "incoming" -> { Button(enabled = !busy, onClick = { action(contact.username, "accept") }) { Text("Accept") }; OutlinedButton(enabled = !busy, onClick = { action(contact.username, "decline") }) { Text("Decline") } }
                            "outgoing" -> OutlinedButton(enabled = !busy, onClick = { action(contact.username, "cancel") }) { Text("Cancel request") }
                            else -> OutlinedButton(onClick = { model.recipientUsername = contact.username; model.validateRecipient(contact.id.toULong()); model.navigate(Destination.Send) }) { ApprovedIcon(ApprovedIcon.Upload, null); Text("Send files", Modifier.padding(start = FilebeamSpace.XSmall)) }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
                        if (contact.status == "accepted") TextButton(enabled = !busy, onClick = { action(contact.username, "remove") }) { Text("Remove friend") }
                        TextButton(enabled = !busy, onClick = { action(contact.username, "block") }) { Text("Block account", color = MaterialTheme.colorScheme.error) }
                    }
                }
            }
        }
        if (state.blocked.isNotEmpty()) ProductionGroupCard(Modifier.fillMaxWidth()) {
            ContactSectionHeading("Blocked accounts", "New deliveries and requests are blocked.", ApprovedIcon.Shield, Modifier.padding(FilebeamSpace.Medium))
            state.blocked.forEach { target -> OptionRow("@$target", enabled = !busy, onClick = { action(target, "unblock") }, trailing = { TextButton(enabled = !busy, onClick = { action(target, "unblock") }) { Text("Unblock") } }) }
        }
    }
    error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
}

@Composable
private fun ContactSectionHeading(title: String, detail: String, icon: ApprovedIcon, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.Small), verticalAlignment = Alignment.CenterVertically) {
        ApprovedIcon(icon, null, tint = MaterialTheme.colorScheme.primary)
        Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) { Text(title, style = MaterialTheme.typography.titleMedium); Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

@Composable
private fun ContactChoiceRow(title: String, selected: String, choices: List<Pair<String, String>>, enabled: Boolean, onChoose: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OptionRow(title, selected, ApprovedIcon.Settings, enabled = enabled, onClick = { expanded = true })
        DropdownMenu(expanded, { expanded = false }) { choices.forEach { (value, label) -> DropdownMenuItem(text = { Text(label) }, onClick = { expanded = false; onChoose(value) }) } }
    }
}
private val overrideChoices = listOf("inherit" to "Inherit account default", "allow" to "Allow", "deny" to "Deny")
private fun overrideValue(value: String): Boolean? = when (value) { "allow" -> true; "deny" -> false; else -> null }
private fun overrideLabel(value: Boolean?): String = when (value) { null -> "Inherit account default"; true -> "Allow"; false -> "Deny" }
private fun policyLabel(value: String): String = when (value) { "authenticated" -> "Signed-in users"; "friends" -> "Friends only"; "nobody" -> "Nobody unless allowed"; else -> "Anyone, including anonymous" }
