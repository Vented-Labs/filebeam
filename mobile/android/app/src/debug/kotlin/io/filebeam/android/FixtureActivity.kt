package io.filebeam.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.core.net.toUri
import io.filebeam.android.platform.CurrentTransfer
import io.filebeam.android.platform.ReceiptState
import io.filebeam.android.platform.TransferActions
import io.filebeam.android.platform.TransferKind
import io.filebeam.android.platform.TransferReceipt
import io.filebeam.android.platform.TransferUiState
import io.filebeam.android.platform.services.AccountKeySituation
import io.filebeam.android.platform.services.InboxItem
import io.filebeam.android.platform.services.InboxItemState
import io.filebeam.android.platform.services.KeyCustody
import io.filebeam.android.ui.AccountReadyContent
import io.filebeam.android.ui.FilebeamScreen
import io.filebeam.android.ui.FilebeamTheme
import io.filebeam.android.ui.InstanceSettingsTransaction
import io.filebeam.android.ui.InstanceTransactionStatus
import io.filebeam.android.ui.SendDiscoveryKey
import io.filebeam.android.ui.SendDriverPolicy
import io.filebeam.android.ui.SendContent
import io.filebeam.android.ui.SendInstancePolicy
import io.filebeam.android.ui.SettingsContent
import io.filebeam.android.ui.TransferReceiptContent
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.send.TransferOptionsSheet
import io.filebeam.rust.JobState
import io.filebeam.rust.TransferSnapshot
import io.filebeam.rust.Transport
import java.io.File
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Debug-only evidence host. Root Send/Receive routes use the production ViewModel and
 * FilebeamScreen. Named `visual-*` routes render only existing stateless production
 * content with non-secret presentation values; they are never transport evidence.
 *
 * Capture plan: API 26/35, system light/dark and three OS palettes, compact 414x892
 * and expanded 1280x800dp, plus font 200%, landscape, IME, and motion recordings.
 */
class FixtureActivity : ComponentActivity() {
    private val model: io.filebeam.android.ui.FilebeamViewModel by viewModels()
    private var route by mutableStateOf("")

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        seedProviderFile()
        configureDisposableInstanceThenShow(intent.getStringExtra(EXTRA_ROUTE))
        setContent {
            when (route) {
                "visual-settings" -> FixtureSurface { SettingsFixture() }
                "visual-settings-key-error" -> FixtureSurface { SettingsFixture(error = true) }
                "visual-account" -> FixtureSurface { AccountFixture() }
                "visual-inbox" -> FixtureSurface { InboxFixture() }
                "visual-receipt-verified" -> FixtureSurface { ReceiptFixture("verified") }
                "visual-receipt-waiting" -> FixtureSurface { ReceiptFixture("waiting") }
                "visual-receipt-secret" -> FixtureSurface { ReceiptFixture("secret") }
                "visual-receipt-error" -> FixtureSurface { ReceiptFixture("error") }
                "visual-options" -> FixtureSurface { OptionsFixture() }
                else -> FilebeamScreen(
                    model = model,
                    start = { action -> action() },
                    pickFiles = { model.appendFiles(listOf(providerUri("added-source.txt"))) },
                    pickTree = {},
                    saveFile = {},
                    exportAccountKey = {},
                    importAccountKey = {},
                )
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        configureDisposableInstanceThenShow(intent.getStringExtra(EXTRA_ROUTE))
    }

    /** Route Send only after the disposable peer's real policy has been discovered and committed. */
    private fun configureDisposableInstanceThenShow(route: String?) {
        model.updateInstanceInput(instance = DISPOSABLE_INSTANCE)
        model.checkInstanceInput()
        lifecycleScope.launch {
            repeat(100) {
                when (model.instanceTransaction.status) {
                    InstanceTransactionStatus.ReadyToCommit -> {
                        model.commitCheckedInstance()
                        showFixture(route)
                        return@launch
                    }
                    is InstanceTransactionStatus.Error -> {
                        showFixture(route)
                        return@launch
                    }
                    else -> delay(100)
                }
            }
            showFixture(route)
        }
    }

    /** Test-only semantic routing, avoiding coordinate-dependent capture setup. */
    fun showFixture(value: String?) {
        route = value.orEmpty()
        when (route) {
            "send-selected" -> {
                model.showSend(SendContent.FILES)
                model.appendFiles(listOf(providerUri("selected-source.txt")))
            }
            "notes" -> model.showSend(SendContent.NOTES)
            else -> if (!route.startsWith("visual-")) model.navigateLegacy(route)
        }
    }

    private fun seedProviderFile() {
        TestDocumentsProvider.file = File(filesDir, "fixture-source.bin").apply {
            if (!exists()) writeBytes(ByteArray(1024) { 7 })
        }
    }

    private fun providerUri(name: String) = "content://io.filebeam.android.testdocuments/$name".toUri()

    companion object {
        const val EXTRA_ROUTE = "io.filebeam.android.fixture.ROUTE"
        private const val DISPOSABLE_INSTANCE = "http://127.0.0.1:8019"
    }
}

@Composable
private fun FixtureSurface(content: @Composable () -> Unit) = FilebeamTheme {
    Column(
        Modifier.fillMaxSize().padding(FilebeamSpace.Gutter),
    ) {
        content()
    }
}

@Composable
private fun SettingsFixture(error: Boolean = false) {
    var transaction by remember { mutableStateOf(
        InstanceSettingsTransaction(
            draftInstance = "http://127.0.0.1:8027",
            status = if (error) InstanceTransactionStatus.Error(R.string.instance_error_check_failed) else InstanceTransactionStatus.ReadyToCommit,
        ),
    ) }
    SettingsContent(
        transaction,
        { transaction = transaction.copy(draftInstance = it, status = InstanceTransactionStatus.Idle) },
        { transaction = transaction.copy(draftRelayOnly = it, status = InstanceTransactionStatus.Idle) },
        {}, {}, {}, {},
    )
}

@Composable
private fun AccountFixture() {
    var inbox by remember { mutableStateOf(true) }
    var custody by remember { mutableStateOf(KeyCustody.SELF) }
    var password by remember { mutableStateOf("") }
    AccountReadyContent(
        username = "fixture-owner", profileUrl = "https://example.test/u/fixture-owner", inbox = inbox, channel = "mail",
        situation = AccountKeySituation(7u, KeyCustody.SELF, emptyList(), false), custody = custody, password = password,
        replacementAcknowledged = false, onInbox = { inbox = it }, onNotification = {}, onCustody = { custody = it }, onPassword = { password = it },
        onAcknowledged = {}, onPrepare = {}, onExport = {}, onImport = {}, onSignOut = {},
    )
}

@Composable
private fun InboxFixture() = Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
    Text(stringResource(R.string.inbox))
    io.filebeam.android.ui.InboxRow(InboxItem("locked", "Encrypted delivery", "authenticated", 0, InboxItemState.Locked), false, {}, {})
    io.filebeam.android.ui.InboxRow(InboxItem("ready", "Encrypted delivery", "authenticated", 0, InboxItemState.Error("Delivery metadata could not be refreshed")), true, {}, {})
}

@Composable
private fun OptionsFixture() {
    val policy = SendInstancePolicy(
        SendDiscoveryKey("http://127.0.0.1:8027", null), true, setOf(Transport.HTTP, Transport.WEB_RTC), 24u, setOf(24u), "http", 65536u,
        mapOf("http" to SendDriverPolicy("http", null, null, null), "webrtc" to SendDriverPolicy("webrtc", null, null, null)),
    )
    TransferOptionsSheet(io.filebeam.android.ui.SendDraft(driver = "http"), policy, {}, {})
}

@Composable
private fun ReceiptFixture(kind: String) {
    val snapshot = when (kind) {
        "waiting" -> fixtureSnapshot(JobState.RUNNING, "waiting")
        "secret" -> fixtureSnapshot(JobState.RUNNING, "unlocking", peerWarning = "A secure secret prompt is required before this transfer can continue.")
        "error" -> fixtureSnapshot(JobState.FAILED, "failed", error = "The transfer could not be verified.")
        else -> fixtureSnapshot(JobState.COMPLETE, "complete")
    }
    val state = TransferUiState(
        busy = kind in setOf("waiting", "secret"), phase = snapshot.phase, snapshot = snapshot,
        receipt = if (kind == "verified") TransferReceipt("fixture", ReceiptState.VERIFIED_PRIVATE, "visual-only-output") else if (kind == "error") TransferReceipt("fixture", ReceiptState.FAILED, message = "Visual-only error") else null,
        current = if (kind in setOf("waiting", "secret")) CurrentTransfer("fixture", TransferKind.DOWNLOAD, null, TransferActions(pause = true)) else null,
    )
    TransferReceiptContent(state, {}, {})
}

private fun fixtureSnapshot(state: JobState, phase: String, error: String? = null, peerWarning: String? = null) = TransferSnapshot(
    state, "fixture", phase, "fixture-source.bin", 0u, 1u, 1024u, 512u, 512u, 512u,
    null, null, emptyList(), error, null, peerWarning, null, "download", "file", "http", state == JobState.RUNNING, false, false, true,
)
