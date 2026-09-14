package io.filebeam.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.filebeam.rust.ClientConfig
import io.filebeam.rust.TransferClient
import io.filebeam.rust.cryptoSelfTest
import io.filebeam.rust.webrtcSelfTest
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class NativeCoreTest {
    @Test fun installedAbiHasEntropyAndAuthenticatedEncryption() { assertTrue(cryptoSelfTest()) }
    @Test fun installedAbiTransfersAnEncryptedWebRtcRecord() { assertTrue(webrtcSelfTest()) }

    @Test fun generatedBindingsOpenAnIsolatedNativeJobCatalog() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val home = File(context.noBackupFilesDir, "test-${UUID.randomUUID()}")
        val client = TransferClient(ClientConfig(home.absolutePath, 128u, 1u, false, false))
        try { assertTrue(client.savedTransfers().isEmpty()) } finally { client.destroy(); home.deleteRecursively() }
    }
}
