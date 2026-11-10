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
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import io.filebeam.android.platform.background.TransferScheduler
import io.filebeam.android.ui.shell.AppModel
import io.filebeam.android.ui.shell.FilebeamApp
import io.filebeam.android.ui.shell.HostActions
import io.filebeam.android.ui.shell.Incoming
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.Tab

class MainActivity : ComponentActivity(), HostActions {
    private val model: AppModel by viewModels()

    // Each launcher serves one request at a time; its callback is cleared once delivered.
    private var onFiles: ((List<Uri>) -> Unit)? = null
    private var onFolder: ((Uri?) -> Unit)? = null
    private var onCreated: ((Uri?) -> Unit)? = null
    private var onOpened: ((Uri?) -> Unit)? = null
    private var afterNotifications: (() -> Unit)? = null

    private val filePicker = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        uris.forEach(::persistRead)
        onFiles?.invoke(uris)
        onFiles = null
    }
    private val folderPicker = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        uri?.let(::persistRead)
        onFolder?.invoke(uri)
        onFolder = null
    }
    private var createMimeType = "application/octet-stream"
    private val documentCreator = registerForActivityResult(
        object : ActivityResultContracts.CreateDocument("application/octet-stream") {
            override fun createIntent(context: android.content.Context, input: String) = super.createIntent(context, input).setType(createMimeType)
        },
    ) { uri ->
        onCreated?.invoke(uri)
        onCreated = null
    }
    private val documentOpener = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        onOpened?.invoke(uri)
        onOpened = null
    }
    private val notificationPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        afterNotifications?.invoke()
        afterNotifications = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) acceptIntent(intent)
        setContent { FilebeamApp(model, this) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        acceptIntent(intent)
    }

    override fun pickFiles(onResult: (List<Uri>) -> Unit) {
        onFiles = onResult
        filePicker.launch(arrayOf("*/*"))
    }

    override fun pickFolder(onResult: (Uri?) -> Unit) {
        onFolder = onResult
        folderPicker.launch(null)
    }

    override fun createDocument(suggestedName: String, mimeType: String, onResult: (Uri?) -> Unit) {
        onCreated = onResult
        createMimeType = mimeType
        documentCreator.launch(suggestedName)
    }

    override fun openDocument(mimeTypes: Array<String>, onResult: (Uri?) -> Unit) {
        onOpened = onResult
        documentOpener.launch(mimeTypes)
    }

    override fun withNotifications(then: () -> Unit) {
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            afterNotifications = then
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            then()
        }
    }

    private fun persistRead(uri: Uri) {
        // Providers may offer only a transient grant; sources are snapshotted while access is held.
        try { contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) } catch (_: SecurityException) {}
    }

    private fun acceptIntent(intent: Intent) {
        when (intent.action) {
            Intent.ACTION_VIEW -> intent.data?.let(::acceptLink)
            Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE -> {
                val uris = buildList {
                    IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::add)
                    IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(::addAll)
                    intent.clipData?.let { clip -> repeat(clip.itemCount) { clip.getItemAt(it).uri?.let(::add) } }
                }.filter { it.scheme == "content" }.distinct()
                uris.forEach(::persistRead)
                val text = intent.getStringExtra(Intent.EXTRA_TEXT)
                when {
                    uris.isNotEmpty() -> {
                        model.deliver(Incoming.Files(uris))
                        model.navigate(tab = Tab.Send)
                    }
                    !text.isNullOrBlank() -> {
                        model.deliver(Incoming.Text(text))
                        model.navigate(tab = Tab.Send)
                    }
                }
            }
        }
        // Transfer notifications carry an opaque route; they always land on Transfers.
        if (intent.hasExtra(TransferScheduler.EXTRA_ROUTE)) model.navigate(tab = Tab.Transfers)
    }

    /** Routes an https link on the app-link host to the screen that understands it. */
    private fun acceptLink(uri: Uri) {
        val segments = uri.pathSegments
        when (segments.firstOrNull()) {
            "u" -> segments.getOrNull(1)?.let { model.navigate(route = Route.Recipient(it.lowercase())) }
            "invitations" -> segments.getOrNull(1)?.let {
                model.deliver(Incoming.Invitation(it))
                model.navigate(route = Route.Auth("invitation"))
            }
            "verify-email" -> {
                model.deliver(Incoming.VerifyEmail(uri.toString()))
                model.navigate(route = Route.Auth("verify"))
            }
            "reset-password" -> segments.getOrNull(1)?.let {
                model.deliver(Incoming.ResetPassword(it, uri.getQueryParameter("email")))
                model.navigate(route = Route.Auth("reset"))
            }
            "account" -> model.navigate(route = Route.Account)
            "features" -> model.navigate(route = Route.Tour)
            "changelog" -> model.navigate(route = Route.Changelog)
            else -> {
                model.deliver(Incoming.TransferLink(uri.toString()))
                model.navigate(tab = Tab.Receive)
            }
        }
    }
}
