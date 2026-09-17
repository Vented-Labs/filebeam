package io.filebeam.android.ui

import androidx.compose.material3.AlertDialog
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.rust.PendingPrompt
import io.filebeam.rust.PromptType
import io.filebeam.rust.DirectoryChoice

@Composable
fun PromptDialog(prompt: PendingPrompt, respond: (ULong, String) -> Unit, respondDirectory: (ULong, DirectoryChoice) -> Unit, respondConsent: (ULong, Boolean) -> Unit, pause: () -> Unit) {
    var value by remember(prompt.id) { mutableStateOf("") }
    val consent = prompt.kind == PromptType.PEER_CONSENT
    val secret = prompt.kind == PromptType.PASSWORD || prompt.kind == PromptType.SHARE_KEY
    val directory = prompt.kind == PromptType.DIRECTORY
    AlertDialog(onDismissRequest = pause, title = { Text(stringResource(when (prompt.kind) {
        PromptType.PASSWORD -> R.string.password; PromptType.SHARE_KEY -> R.string.decryption_key; PromptType.PEER_CONSENT -> R.string.peer_consent_title; else -> R.string.continue_action
    })) }, text = {
        if (consent) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.peer_consent_body))
            prompt.peer?.let { Text(it) }
        }
        else if (secret) OutlinedTextField(value, { value = it }, singleLine = true, visualTransformation = PasswordVisualTransformation())
        else if (directory) Text(stringResource(R.string.directory_choice_body))
        else if (prompt.kind == PromptType.SHARE_READY) Text(stringResource(R.string.share_ready_body))
        else Text(stringResource(R.string.prompt_unavailable))
    }, confirmButton = {
        when {
            directory -> Row {
                TextButton(onClick = { respondDirectory(prompt.id, DirectoryChoice.ZIP) }) { Text(stringResource(R.string.directory_zip)) }
                TextButton(onClick = { respondDirectory(prompt.id, DirectoryChoice.INDIVIDUAL_FILES) }) { Text(stringResource(R.string.directory_individual)) }
            }
            consent -> TextButton(onClick = { respondConsent(prompt.id, true) }) { Text(stringResource(R.string.allow)) }
            prompt.kind == PromptType.SHARE_READY -> TextButton(onClick = { respond(prompt.id, "continue") }) { Text(stringResource(R.string.continue_action)) }
            secret -> TextButton(enabled = value.isNotBlank(), onClick = { respond(prompt.id, value); value = "" }) { Text(stringResource(R.string.continue_action)) }
        }
    }, dismissButton = {
        if (consent) TextButton(onClick = { respondConsent(prompt.id, false) }) { Text(stringResource(R.string.deny)) }
        else TextButton(onClick = pause) { Text(stringResource(R.string.cancel)) }
    })
}
