package io.filebeam.android.ui

import android.content.Intent
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.NoteRequest
import io.filebeam.android.platform.services.ServiceState
import kotlinx.coroutines.launch

@Composable
fun NotesScreen(model: FilebeamViewModel, instance: String) {
    val state = model.notes.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var body by remember { mutableStateOf("") }
    var title by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var retention by remember { mutableStateOf("") }
    var link by remember { mutableStateOf("") }
    var burn by remember { mutableStateOf(false) }
    var live by remember { mutableStateOf(false) }
    var viewer by remember { mutableStateOf<String?>(null) }
    var createdLink by remember { mutableStateOf<String?>(null) }
    var feedback by remember { mutableStateOf<String?>(null) }
    Text(stringResource(R.string.notes), style = MaterialTheme.typography.headlineSmall)
    OutlinedTextField(title, { title = it }, label = { Text(stringResource(R.string.note_title)) }, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(body, { body = it }, label = { Text(stringResource(R.string.note_body)) }, minLines = 5, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.optional_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
    OutlinedTextField(retention, { retention = it.filter(Char::isDigit) }, label = { Text(stringResource(R.string.retention_hours)) }, modifier = Modifier.fillMaxWidth())
    val burnLabel = stringResource(R.string.burn_on_read)
    val consumedMessage = stringResource(R.string.note_consumed)
    Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(burn, { burn = it }); Text(burnLabel) }
    Row(verticalAlignment = Alignment.CenterVertically) { Checkbox(live, { live = it }); Text(stringResource(R.string.webrtc_transport)) }
    Button(enabled = body.isNotBlank(), onClick = { scope.launch {
        runCatching { model.notes.create(NoteRequest(instance, body, title = title, password = password.ifBlank { null }, retentionHours = retention.toULongOrNull(), burnAfterRead = burn, live = live)) }
            .onSuccess { body = ""; password = ""; createdLink = it.link }
            .onFailure { feedback = it.message }
    } }) { Text(stringResource(R.string.create_note)) }
    createdLink?.let { created ->
        Text(created, style = MaterialTheme.typography.bodySmall)
        Button(onClick = { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, created), context.getString(R.string.share_note))) }) { Text(stringResource(R.string.share_note)) }
    }
    OutlinedTextField(link, { link = it }, label = { Text(stringResource(R.string.note_link)) }, modifier = Modifier.fillMaxWidth())
    Button(enabled = link.isNotBlank(), onClick = { scope.launch {
        runCatching { model.notes.claim(link, password.ifBlank { null }) }.onSuccess { viewer = it.text; if (it.consumed) feedback = consumedMessage }.onFailure { feedback = it.message }
    } }) { Text(stringResource(R.string.open_note)) }
    viewer?.let { text ->
        OutlinedCard(Modifier.fillMaxWidth()) { Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(text)
            Button(onClick = { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), context.getString(R.string.share_note))) }) { Text(stringResource(R.string.share_note)) }
        } }
    }
    when (state) { is ServiceState.Failed -> Text(state.message, color = MaterialTheme.colorScheme.error); is ServiceState.Unavailable -> Text(state.reason, color = MaterialTheme.colorScheme.error); else -> Unit }
    feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}
