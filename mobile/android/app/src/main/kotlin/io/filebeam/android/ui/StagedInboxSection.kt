package io.filebeam.android.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.Modifier
import io.filebeam.android.ui.design.ProductionGroupCard
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.send.formatFileSize
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
    ProductionGroupCard(Modifier.fillMaxWidth()) {
      Column(Modifier.padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
        Text("Private staging", style = MaterialTheme.typography.titleMedium)
        Text("Downloaded ciphertext stays private until you unlock and verify it.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        TextButton(onClick = { scope.launch { refresh() } }) { ApprovedIcon(ApprovedIcon.Download, null); Text("Refresh staged files", Modifier.padding(start = FilebeamSpace.XSmall)) }
        if (entries.isNotEmpty()) {
            Text("Automatically staged ciphertext", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(password, { password = it }, label = { Text("Receiving-key password, if locked") }, singleLine = true, modifier = Modifier.fillMaxWidth(), visualTransformation = PasswordVisualTransformation())
        }
        entries.forEach { item ->
            Text("${formatFileSize(item.getLong("bytes"))} · ${if (item.getString("state") == "staged-locked") "Encrypted and ready to unlock" else "Waiting for private staging"}", style = MaterialTheme.typography.bodyMedium)
            Button(enabled = !busy && item.getString("state") == "staged-locked", onClick = {
                busy = true
                scope.launch {
                    runCatching { model.accounts.verifyStagedInbox(item.getString("id"), item.getLong("key_bundle_id").toULong(), password.takeIf(String::isNotBlank)) }
                        .onSuccess { paths = it; error = null }.onFailure { error = it.message }
                    password = ""; busy = false
                }
            }, modifier = Modifier.fillMaxWidth()) { ApprovedIcon(ApprovedIcon.Shield, null); Text("Unlock and verify privately", Modifier.padding(start = FilebeamSpace.XSmall)) }
            TextButton(enabled = !busy, onClick = { scope.launch { runCatching { model.accounts.dismissStagedInbox(item.getString("id")); refresh() }.onFailure { error = it.message } } }) { Text("Remove local ciphertext") }
        }
        paths.forEach { path -> TextButton(onClick = { saveFile(path) }) { Text("Save verified file: ${java.io.File(path).name}") } }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
      }
    }
}
