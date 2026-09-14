package io.filebeam.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.filebeam.android.platform.security.CheckpointSecretStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class CheckpointSecretStoreTest {
    private val scope = "checkpoint-catalog-v1"

    @Test fun tamperedWrappedCatalogKeyFailsClosed() = withStore { store, _, record ->
        val key = store.loadOrCreate(scope)
        assertEquals(32, key.size)
        val bytes = record.readBytes()
        bytes[bytes.lastIndex] = (bytes.last().toInt() xor 1).toByte()
        record.writeBytes(bytes)
        assertThrows(Exception::class.java) { store.loadOrCreate(scope) }
    }

    @Test fun missingKeystoreKeyRejectsRestoredWrappedCatalogKey() = withStore { store, root, _ ->
        store.loadOrCreate(scope)
        KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry(alias(root)) }
        assertThrows(IllegalStateException::class.java) { store.loadOrCreate(scope) }
    }

    private fun withStore(block: (CheckpointSecretStore, File, File) -> Unit) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.noBackupFilesDir, "checkpoint-keystore-${UUID.randomUUID()}")
        val id = id(root)
        val record = File(context.noBackupFilesDir, "checkpoint-key-$id")
        try { block(CheckpointSecretStore(context, root), root, record) }
        finally {
            record.delete()
            KeyStore.getInstance("AndroidKeyStore").apply { load(null); deleteEntry("filebeam.checkpoint.$id.v1") }
        }
    }

    private fun id(root: File) = MessageDigest.getInstance("SHA-256")
        .digest(root.canonicalPath.toByteArray(Charsets.UTF_8)).take(12)
        .joinToString("") { "%02x".format(it) }

    private fun alias(root: File) = "filebeam.checkpoint.${id(root)}.v1"
}
