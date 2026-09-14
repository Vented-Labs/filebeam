package io.filebeam.android.ui

import androidx.compose.material3.AlertDialog
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
import io.filebeam.android.R
import io.filebeam.rust.PendingPrompt
import io.filebeam.rust.PromptType

@Composable
fun PromptDialog(prompt: PendingPrompt, respond: (ULong, String) -> Unit, pause: () -> Unit) {
    var value by remember(prompt.id) { mutableStateOf("") }
    val consent = prompt.kind == PromptType.PEER_CONSENT
    val secret = prompt.kind == PromptType.PASSWORD || prompt.kind == PromptType.SHARE_KEY
    val directory = prompt.kind == PromptType.DIRECTORY
    AlertDialog(onDismissRequest = pause, title = { Text(stringResource(when (prompt.kind) {
        PromptType.PASSWORD -> R.string.password; PromptType.SHARE_KEY -> R.string.decryption_key; PromptType.PEER_CONSENT -> R.string.peer_consent_title; else -> R.string.continue_action
    })) }, text = {
        if (consent) Text(stringResource(R.string.peer_consent_body))
        else if (secret) OutlinedTextField(value, { value = it }, singleLine = true, visualTransformation = PasswordVisualTransformation())
        else Text(stringResource(R.string.prompt_unavailable))
    }, confirmButton = {
        if (directory) TextButton(onClick = pause) { Text(stringResource(R.string.pause)) }
        else TextButton(enabled = !secret || value.isNotBlank(), onClick = { respond(prompt.id, if (secret) value else "yes"); value = "" }) { Text(stringResource(R.string.continue_action)) }
    }, dismissButton = { TextButton(onClick = pause) { Text(stringResource(R.string.cancel)) } })
}
