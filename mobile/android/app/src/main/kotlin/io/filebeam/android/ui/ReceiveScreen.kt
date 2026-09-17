package io.filebeam.android.ui

import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import io.filebeam.android.R
import io.filebeam.android.platform.services.NoteContent
import io.filebeam.android.ui.design.ActionDock
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import io.filebeam.rust.LinkInspection

@Composable
fun ReceiveScreen(model: FilebeamViewModel, busy: Boolean, receive: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    var inspection by remember { mutableStateOf<Result<LinkInspection>?>(null) }
    var note by remember { mutableStateOf<NoteContent?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var noteLink by remember { mutableStateOf<String?>(null) }
    var noteKey by remember { mutableStateOf<String?>(null) }
    var notePassword by remember { mutableStateOf<String?>(null) }
    var requestGeneration by remember { mutableStateOf(0) }
    val unknownError = stringResource(R.string.unknown_error)
    LaunchedEffect(model.link) {
        requestGeneration++
        inspection = null
        noteLink = null
        noteKey = null
        notePassword = null
        error = null
        if (model.link.isNotBlank()) model.inspectReceiveLink { inspection = it }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
        Text(stringResource(R.string.receive_title), style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.receive_description), style = MaterialTheme.typography.bodyLarge)
        ProductionGroupCard(Modifier.fillMaxWidth()) {
            OutlinedTextField(
                value = model.link,
                onValueChange = model::updateReceiveLink,
                label = { Text(stringResource(R.string.transfer_link)) },
                placeholder = { Text(stringResource(R.string.receive_paste_hint)) },
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedButton(
                enabled = !busy,
                onClick = { clipboard.getText()?.text?.let(model::receiveLink) },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.paste_link)) }
        }
        when (val result = inspection) {
            null -> if (model.link.isNotBlank()) Text(stringResource(R.string.loading), style = MaterialTheme.typography.bodySmall)
            else -> result.fold(
                onSuccess = { link ->
                    Text(stringResource(R.string.receive_inspection, link.kind, link.driver), style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                    if (link.kind == "note") {
                        ActionDock(stringResource(R.string.open_note), !busy, {
                            error = null
                            if (!model.noteLinkHasKey(link)) noteKey = ""
                            else if (link.passwordRequired) notePassword = ""
                            else claimNote(model, model.link, null, requestGeneration, { requestGeneration }, { note = it }, { error = it })
                        }, icon = ApprovedIcon.File)
                    } else ActionDock(stringResource(R.string.start_download), !busy, receive, icon = ApprovedIcon.Download)
                },
                onFailure = { error -> Text(error.message ?: stringResource(R.string.unknown_error), color = MaterialTheme.colorScheme.error) },
            )
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
    noteKey?.let { key -> AlertDialog(
        onDismissRequest = { noteKey = null }, title = { Text(stringResource(R.string.decryption_key)) },
        text = {
            SecureWindowEffect()
            OutlinedTextField(key, { noteKey = it }, singleLine = true, visualTransformation = PasswordVisualTransformation(), label = { Text(stringResource(R.string.decryption_key)) })
            OutlinedButton(onClick = { clipboard.getText()?.text?.let { noteKey = it } }) { Text(stringResource(R.string.paste_link)) }
        },
        confirmButton = { OutlinedButton(enabled = key.isNotBlank(), onClick = {
            val inspected = inspection?.getOrNull() ?: return@OutlinedButton
            model.combineReceivedNoteKey(inspected, key).onSuccess { combined ->
                noteKey = null
                noteLink = combined
                if (inspected.passwordRequired) notePassword = "" else claimNote(model, combined, null, requestGeneration, { requestGeneration }, { note = it }, { error = it })
            }.onFailure { error = it.message ?: unknownError }
        }) { Text(stringResource(R.string.continue_action)) } },
        dismissButton = { OutlinedButton(onClick = { noteKey = null }) { Text(stringResource(R.string.cancel)) } },
    ) }
    notePassword?.let { password -> AlertDialog(
        onDismissRequest = { notePassword = null }, title = { Text(stringResource(R.string.password)) },
        text = { SecureWindowEffect(); OutlinedTextField(password, { notePassword = it }, singleLine = true, visualTransformation = PasswordVisualTransformation(), label = { Text(stringResource(R.string.password)) }) },
        confirmButton = { OutlinedButton(enabled = password.isNotBlank(), onClick = {
            val target = noteLink ?: model.link
            val generation = requestGeneration
            model.openReceivedNote(target, password) { result ->
                if (generation != requestGeneration) return@openReceivedNote
                result.onSuccess { notePassword = null; note = it }
                    .onFailure { error = it.message ?: "Unable to open note"; notePassword = "" }
            }
        }) { Text(stringResource(R.string.open_note)) } },
        dismissButton = { OutlinedButton(onClick = { notePassword = null }) { Text(stringResource(R.string.cancel)) } },
    ) }
    note?.let { content -> SecureNoteViewer(content) { note = null } }
}

@Composable
private fun SecureNoteViewer(content: NoteContent, close: () -> Unit) {
    SecureWindowEffect()
    AlertDialog(
        onDismissRequest = close,
        title = { Text(content.title ?: stringResource(R.string.open_note)) },
        text = { Text(content.text) },
        confirmButton = { OutlinedButton(onClick = close) { Text(stringResource(R.string.close)) } },
    )
}

private fun claimNote(model: FilebeamViewModel, link: String, password: String?, generation: Int, currentGeneration: () -> Int, opened: (NoteContent) -> Unit, failed: (String) -> Unit) {
    model.openReceivedNote(link, password) { result ->
        if (generation != currentGeneration()) return@openReceivedNote
        result.onSuccess(opened).onFailure { failed(it.message ?: "Unable to open note") }
    }
}
