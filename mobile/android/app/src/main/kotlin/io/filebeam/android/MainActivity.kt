package io.filebeam.android

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.core.net.toUri
import io.filebeam.android.ui.FilebeamScreen
import io.filebeam.android.ui.FilebeamViewModel
import kotlinx.coroutines.launch
import java.io.InputStream

class MainActivity : ComponentActivity() {
    private val model: FilebeamViewModel by viewModels()
    private var pendingStart: (() -> Unit)? = null
    private val notifications = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        pendingStart?.invoke()
        pendingStart = null
    }
    private val files = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        uris.forEach { uri ->
            try { contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
            catch (_: SecurityException) { /* Transient providers are materialized while access is held. */ }
        }
        if (uris.isNotEmpty()) model.appendFiles(uris)
    }
    private val tree = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let {
            try { contentResolver.takePersistableUriPermission(it, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
            catch (_: SecurityException) { /* Providers may offer only a transient read grant. */ }
            lifecycleScope.launch(kotlinx.coroutines.Dispatchers.IO) {
                runCatching { model.coordinator.storage.flattenDocumentTree(it) }
                    .onSuccess { sources -> kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Main) {
                        model.appendFiles(sources.map { source -> source.uri }, sources.associate { source -> source.uri.toString() to source.relativePath })
                    } }
                    .onFailure { error -> model.coordinator.message(error.message ?: getString(R.string.source_unavailable)) }
            }
        }
    }
    private val save = registerForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val source = model.exportSource
        if (uri != null && source != null) model.coordinator.export(source, uri)
        model.exportSource = null
    }
    private val exportAccountKey = registerForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri ->
        if (uri != null) lifecycleScope.launch { runCatching { (application as FilebeamApplication).accounts.exportKeyTo(uri) }.onFailure { model.coordinator.message(it.message) } }
    }
    private val importAccountKey = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) lifecycleScope.launch(kotlinx.coroutines.Dispatchers.IO) {
            runCatching { contentResolver.openInputStream(uri)?.use(InputStream::readAccountKey) ?: error("Could not open the selected key file")
            }.onSuccess { value -> runCatching { (application as FilebeamApplication).accounts.importKey(value) }.onFailure { model.coordinator.message(it.message) } }
                .onFailure { model.coordinator.message(it.message) }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) acceptIntent(intent)
        setContent {
            FilebeamScreen(
                model,
                ::startWithNotificationPermission,
                pickFiles = { files.launch(arrayOf("*/*")) },
                pickTree = { tree.launch(null) },
                saveFile = { source -> model.exportSource = source; save.launch(java.io.File(source).name) },
                exportAccountKey = { exportAccountKey.launch("filebeam-account-key.fbsk1") },
                importAccountKey = { importAccountKey.launch(arrayOf("text/plain", "application/octet-stream")) },
            )
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        acceptIntent(intent)
    }

    private fun startWithNotificationPermission(start: () -> Unit) {
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            pendingStart = start
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else start()
    }

    private fun acceptIntent(intent: Intent) {
        when (intent.action) {
            Intent.ACTION_VIEW -> intent.dataString?.let { value ->
                val route = value.toUri().pathSegments.firstOrNull()
                if (route in setOf("notes", "turbo", "account", "settings", "receive", "transfers", "inbox")) model.navigateLegacy(route)
                else model.receiveLink(value)
            }
            Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE -> {
                val uris = buildList {
                    IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::add)
                    IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::addAll)
                    intent.clipData?.let { clip -> repeat(clip.itemCount) { clip.getItemAt(it).uri?.let(::add) } }
                }.filter { it.scheme == "content" }.distinct()
                uris.forEach { uri ->
                    try { contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
                    catch (_: SecurityException) { /* The coordinator snapshots transient grants while available. */ }
                }
                if (uris.isNotEmpty()) model.appendFiles(uris)
                else intent.getStringExtra(Intent.EXTRA_TEXT)?.let(model::receiveLink)
            }
        }
        intent.getStringExtra("route")?.let(model::navigateLegacy)
    }
}

private fun InputStream.readAccountKey(): String {
    val maximumBytes = 16 * 1024
    val bytes = ByteArray(maximumBytes + 1)
    var length = 0
    while (length < bytes.size) {
        val read = read(bytes, length, bytes.size - length)
        if (read < 0) break
        length += read
    }
    require(length <= maximumBytes) { "Key file is too large" }
    return bytes.copyOf(length).toString(Charsets.US_ASCII)
}
