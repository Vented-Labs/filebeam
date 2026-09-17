package io.filebeam.android.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.InboxItem
import io.filebeam.android.platform.services.InboxItemState
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import kotlinx.coroutines.launch

private enum class InboxFilter { ALL, UNREAD, SAVED }

@Composable
fun InboxDestination(model: FilebeamViewModel, instance: String) {
    val state = model.inbox.state.collectAsStateWithLifecycle().value
    val account = model.accounts.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    var selected by remember { mutableStateOf<InboxItem?>(null) }
    var filter by remember { mutableStateOf(InboxFilter.ALL) }
    var saved by remember { mutableStateOf(setOf<String>()) }
    var locallyRead by remember { mutableStateOf(setOf<String>()) }
    var feedback by remember { mutableStateOf<String?>(null) }
    Column(Modifier.verticalScroll(rememberScrollState()).imePadding().padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
        Text(stringResource(R.string.inbox), style = MaterialTheme.typography.headlineSmall)
        Text(stringResource(R.string.inbox_opaque_description), style = MaterialTheme.typography.bodySmall)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            InboxFilter.entries.forEach { value -> TextButton(onClick = { filter = value }) { Text(stringResource(value.label)) } }
        }
        when {
            account !is ServiceState.Ready -> Text(stringResource(R.string.inbox_sign_in_required))
            !account.value.inboxEnabled -> Text(stringResource(R.string.inbox_user_disabled))
            state is ServiceState.Loading -> {
                Button(onClick = { scope.launch { runCatching { model.inbox.refresh(instance) }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.refresh)) }
                Text(stringResource(R.string.inbox_refresh_to_load))
            }
            state is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error)
            state is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error)
            state is ServiceState.Ready -> {
                Button(onClick = { scope.launch { runCatching { model.inbox.refresh(instance) }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.refresh)) }
                val visible = when (filter) {
                    InboxFilter.ALL -> state.value
                    InboxFilter.UNREAD -> state.value.filterNot { it.id in locallyRead }
                    InboxFilter.SAVED -> state.value.filter { it.id in saved }
                }
                if (filter == InboxFilter.UNREAD) Text(stringResource(R.string.inbox_unread_local))
                if (visible.isEmpty()) Text(stringResource(R.string.inbox_empty))
                visible.forEach { item -> InboxRow(item, item.id in saved, { selected = item }, { saved = saved.toggle(item.id) }) }
            }
        }
        feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
    selected?.let { item -> InboxDetail(item, item.id in saved, item.id in locallyRead, { saved = saved.toggle(item.id) }, { locallyRead = locallyRead.toggle(item.id) }, { password ->
        scope.launch { runCatching { model.inbox.download(instance, item.id, password) }.onSuccess { selected = null; model.navigate(Destination.Transfers) }.onFailure { feedback = it.message } }
    }, { selected = null }) }
}

@Composable fun InboxRow(item: InboxItem, saved: Boolean, open: () -> Unit, save: () -> Unit) {
    ProductionGroupCard(Modifier.fillMaxWidth().clickable(onClick = open)) {
        Column(Modifier.fillMaxWidth().padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
            Text(item.title, style = MaterialTheme.typography.titleMedium)
            Text(stringResource(item.state.label), style = MaterialTheme.typography.bodySmall)
            item.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            TextButton(onClick = save) { Text(stringResource(if (saved) R.string.remove_bookmark else R.string.bookmark_on_device)) }
        }
    }
}

@Composable private fun InboxDetail(item: InboxItem, saved: Boolean, read: Boolean, save: () -> Unit, markRead: () -> Unit, download: (String?) -> Unit, dismiss: () -> Unit) {
    var password by remember { mutableStateOf("") }
    AlertDialog(onDismissRequest = dismiss, title = { Text(item.title) }, text = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.inbox_detail_opaque))
            Text(stringResource(item.state.label))
            if (item.state is InboxItemState.Locked || item.state is InboxItemState.NotSetup) OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.account_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
        }
    }, confirmButton = { if (item.state is InboxItemState.Locked || item.state is InboxItemState.NotSetup) Button(onClick = { download(password.ifBlank { null }) }) { Text(stringResource(R.string.open_transfer)) } }, dismissButton = { Row { TextButton(onClick = save) { Text(stringResource(if (saved) R.string.remove_bookmark else R.string.bookmark_on_device)) }; TextButton(onClick = markRead) { Text(stringResource(if (read) R.string.mark_unread_local else R.string.mark_read_local)) }; TextButton(onClick = dismiss) { Text(stringResource(R.string.cancel)) } } })
}

private fun Set<String>.toggle(value: String) = if (value in this) this - value else this + value
private val InboxFilter.label get() = when (this) { InboxFilter.ALL -> R.string.inbox_all; InboxFilter.UNREAD -> R.string.inbox_unread; InboxFilter.SAVED -> R.string.inbox_saved }
private val InboxItemState.label get() = when (this) { InboxItemState.Locked -> R.string.inbox_locked; InboxItemState.NotSetup -> R.string.inbox_setup_required; InboxItemState.Expired -> R.string.inbox_expired; InboxItemState.UserDisabled -> R.string.inbox_user_disabled; InboxItemState.InstanceDisabled -> R.string.inbox_instance_disabled; is InboxItemState.Error -> R.string.inbox_error }
