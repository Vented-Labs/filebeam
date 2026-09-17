package io.filebeam.android.ui

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.AnnotatedString
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.NoteRequest
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ActionDock
import io.filebeam.android.ui.send.NoteEditor
import io.filebeam.android.ui.send.NoteOptionsCard
import kotlinx.coroutines.launch

/** Create-only note composer. Claiming and decrypted viewing belong to Receive. */
@Composable
fun NotesScreen(model: FilebeamViewModel, instance: String) = NotesComposer(model, instance, busy = false)

@Composable
fun NotesComposer(model: FilebeamViewModel, instance: String, busy: Boolean, header: @Composable () -> Unit = {}) {
    val draft = model.noteDraft
    val state = model.notes.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var optionsOpen by remember { mutableStateOf(false) }
    var askPassword by remember { mutableStateOf(false) }
    var password by remember { mutableStateOf("") }
    var createdLink by remember { mutableStateOf<String?>(null) }
    var createdLive by remember { mutableStateOf(false) }
    var separateKey by remember { mutableStateOf<String?>(null) }
    var feedback by remember { mutableStateOf<String?>(null) }
    val shareNote = stringResource(R.string.share_note)
    val unknownError = stringResource(R.string.unknown_error)

    fun submit(secret: String?) {
        scope.launch {
            try {
                retentionHours(draft.retentionHours).fold(
                    onSuccess = { retention ->
                        runCatching {
                            model.notes.create(NoteRequest(
                                instance = instance,
                                text = draft.body,
                                title = draft.title,
                                language = draft.language,
                                password = secret?.takeIf(String::isNotBlank),
                                retentionHours = retention,
                                burnAfterRead = draft.burnAfterRead,
                                live = draft.live,
                                includeKeyInLink = draft.includeKeyInLink,
                            ))
                        }.onSuccess {
                            createdLink = it.link
                            separateKey = it.separateKey
                            createdLive = draft.live
                            model.updateNote { NoteDraft() }
                        }.onFailure { feedback = it.message ?: unknownError }
                    },
                    onFailure = { feedback = it.message },
                )
            } finally { password = "" }
        }
    }

    Column(Modifier.fillMaxSize().imePadding()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
            header()
        NoteEditor(
            title = draft.title,
            body = draft.body,
            language = draft.language,
            enabled = !busy,
            onTitleChange = { value -> model.updateNote { it.copy(title = value) } },
            onBodyChange = { value -> model.updateNote { it.copy(body = value) } },
            onLanguageChange = { value -> model.updateNote { it.copy(language = value) } },
        )
        NoteOptionsCard(
            draft = draft,
            passwordEnabled = draft.passwordEnabled,
            enabled = !busy && model.canSubmitNote(),
            onClick = { optionsOpen = true },
        )
        createdLink?.let { link ->
            Text(stringResource(R.string.note_ready), style = MaterialTheme.typography.titleMedium)
            Text(link, style = MaterialTheme.typography.bodySmall)
            Button(onClick = {
                context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link), shareNote))
            }) { Text(shareNote) }
            separateKey?.let { key ->
                Text("Share key separately after reviewing it.", style = MaterialTheme.typography.bodySmall)
                Button(onClick = { clipboard.setText(AnnotatedString(key)) }) { Text("Copy separate key") }
            }
            if (createdLive) Button(onClick = { model.notes.endLive(link) }) { Text(stringResource(R.string.end_live_share)) }
        }
        when (state) {
            is ServiceState.Loading -> Text(stringResource(R.string.loading))
            is ServiceState.Unavailable -> Text("${stringResource(R.string.notes_unavailable)}: ${state.reason}", color = MaterialTheme.colorScheme.error)
            is ServiceState.Failed -> Text("${stringResource(R.string.notes_error)}: ${state.message}", color = MaterialTheme.colorScheme.error)
            is ServiceState.Ready -> Unit
        }
        feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        }
        Text(stringResource(R.string.encrypted_on_device), Modifier.align(androidx.compose.ui.Alignment.CenterHorizontally), color = MaterialTheme.colorScheme.onSurfaceVariant)
        ActionDock(
            stringResource(R.string.encrypt_and_share),
            enabled = !busy && draft.body.isNotBlank() && retentionHours(draft.retentionHours).isSuccess && model.canSubmitNote(),
            onClick = { if (draft.passwordEnabled) askPassword = true else submit(null) },
            modifier = Modifier.padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Small),
        )
    }
    if (optionsOpen) {
        io.filebeam.android.ui.send.NoteOptionsSheet(
            draft = draft,
            passwordEnabled = draft.passwordEnabled,
            onPasswordChange = { enabled -> model.updateNote { it.copy(passwordEnabled = enabled) } },
            onUpdate = { transform -> model.updateNote(transform) },
            onDismiss = { optionsOpen = false },
        )
    }
    if (askPassword) AlertDialog(
        onDismissRequest = { password = ""; askPassword = false },
        title = { Text(stringResource(R.string.note_password_title)) },
        text = { SecureWindowEffect(); OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.optional_password)) }, visualTransformation = PasswordVisualTransformation()) },
        confirmButton = { Button(onClick = { askPassword = false; submit(password) }) { Text(stringResource(R.string.continue_action)) } },
        dismissButton = { Button(onClick = { password = ""; askPassword = false }) { Text(stringResource(R.string.cancel)) } },
    )
}
