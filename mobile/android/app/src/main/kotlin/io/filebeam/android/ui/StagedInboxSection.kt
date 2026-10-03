package io.filebeam.android.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.text.input.PasswordVisualTransformation
import kotlinx.coroutines.launch
import org.json.JSONObject

@Composable
fun StagedInboxSection(model: FilebeamViewModel, instance: String, saveFile: (String) -> Unit) {
    var entries by remember(instance) { mutableStateOf<List<JSONObject>>(emptyList()) }
    var password by remember(instance) { mutableStateOf("") }
    var paths by remember(instance) { mutableStateOf<List<String>>(emptyList()) }
    var error by remember(instance) { mutableStateOf<String?>(null) }
    var busy by remember(instance) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    suspend fun refresh() {
        runCatching { JSONObject(model.accounts.automaticReceiving(null)).getJSONArray("entries") }
            .onSuccess { array -> entries = (0 until array.length()).map { array.getJSONObject(it) }.filter { it.getString("state") != "dismissed" } }
            .onFailure { error = it.message }
    }
    LaunchedEffect(instance) { refresh() }
    Column {
        TextButton(onClick = { scope.launch { refresh() } }) { Text("Refresh staged files") }
        if (entries.isNotEmpty()) {
            Text("Automatically staged ciphertext", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(password, { password = it }, label = { Text("Receiving-key password, if locked") }, visualTransformation = PasswordVisualTransformation())
        }
        entries.forEach { item ->
            Text("${item.getLong("bytes")} encrypted bytes — ${item.getString("state")}")
            Button(enabled = !busy && item.getString("state") == "staged-locked", onClick = {
                busy = true
                scope.launch {
                    runCatching { model.accounts.verifyStagedInbox(item.getString("id"), item.getLong("key_bundle_id").toULong(), password.takeIf(String::isNotBlank)) }
                        .onSuccess { paths = it; error = null }.onFailure { error = it.message }
                    password = ""; busy = false
                }
            }) { Text("Unlock and verify privately") }
            TextButton(enabled = !busy, onClick = { scope.launch { runCatching { model.accounts.dismissStagedInbox(item.getString("id")); refresh() }.onFailure { error = it.message } } }) { Text("Remove local ciphertext") }
        }
        paths.forEach { path -> TextButton(onClick = { saveFile(path) }) { Text("Save verified file: ${java.io.File(path).name}") } }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
}
