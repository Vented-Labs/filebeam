package io.filebeam.android.ui

import android.view.WindowManager
import androidx.activity.compose.LocalActivity
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
import androidx.compose.runtime.DisposableEffect
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
    var notePassword by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(model.link) {
        inspection = null
        if (model.link.isNotBlank()) model.inspectReceiveLink { inspection = it }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
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
                            model.openReceivedNote(notePassword) { result ->
                                notePassword = null
                                result.onSuccess { note = it }
                                    .onFailure { error = it.message ?: "Unable to open note"; notePassword = "" }
                            }
                        }, icon = ApprovedIcon.File)
                    } else ActionDock(stringResource(R.string.start_download), !busy, receive, icon = ApprovedIcon.Download)
                },
                onFailure = { error -> Text(error.message ?: stringResource(R.string.unknown_error), color = MaterialTheme.colorScheme.error) },
            )
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
    notePassword?.let { password -> AlertDialog(
        onDismissRequest = { notePassword = null }, title = { Text(stringResource(R.string.password)) },
        text = { OutlinedTextField(password, { notePassword = it }, singleLine = true, visualTransformation = PasswordVisualTransformation(), label = { Text(stringResource(R.string.password)) }) },
        confirmButton = { OutlinedButton(enabled = password.isNotBlank(), onClick = {
            model.openReceivedNote(password) { result ->
                notePassword = null
                result.onSuccess { note = it }.onFailure { error = it.message ?: "Unable to open note"; notePassword = "" }
            }
        }) { Text(stringResource(R.string.open_note)) } },
        dismissButton = { OutlinedButton(onClick = { notePassword = null }) { Text(stringResource(R.string.cancel)) } },
    ) }
    note?.let { content -> SecureNoteViewer(content) { note = null } }
}

@Composable
private fun SecureNoteViewer(content: NoteContent, close: () -> Unit) {
    val activity = LocalActivity.current
    DisposableEffect(activity) {
        activity?.window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { activity?.window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }
    AlertDialog(
        onDismissRequest = close,
        title = { Text(content.title ?: stringResource(R.string.open_note)) },
        text = { Text(content.text) },
        confirmButton = { OutlinedButton(onClick = close) { Text(stringResource(R.string.close)) } },
    )
}
