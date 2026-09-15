package io.filebeam.android

import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.filebeam.android.platform.storage.DocumentStorage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

@RunWith(AndroidJUnit4::class)
class DocumentStorageExportRecoveryTest {
    private val uri = Uri.parse("content://io.filebeam.android.testdocuments/document/writable")

    @Before fun resetProviderState() {
        TestDocumentsProvider.revoked = false
        TestDocumentsProvider.nonSeekable = false
    }

    @Test fun nonseekableProviderFallsBackToSnapshotAndInterruptedExportResumesExactly() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val storage = DocumentStorage(context)
        val bytes = ByteArray(160 * 1024) { (it % 251).toByte() }
        TestDocumentsProvider.file = File(context.cacheDir, "provider-${System.nanoTime()}").apply { writeBytes(bytes) }
        TestDocumentsProvider.nonSeekable = true
        assertThrows(Exception::class.java) { storage.providerInput(uri, "pipe.bin") }
        val paths = storage.importFiles("snapshot-${System.nanoTime()}", listOf(uri), false, {}, {})
        assertArrayEquals(bytes, File(paths.single()).readBytes())
        TestDocumentsProvider.nonSeekable = false
        val source = File(storage.root, "export-${System.nanoTime()}").apply { parentFile?.mkdirs(); writeBytes(bytes) }
        var checks = 0
        assertThrows(CancellationException::class.java) { runBlocking { storage.export(source.absolutePath, uri) { if (++checks > 1) throw CancellationException() } } }
        storage.export(source.absolutePath, uri) {}
        assertArrayEquals(bytes, TestDocumentsProvider.file.readBytes())
    }

    @Test fun changedDestinationOrSourceNeverAppendsJournaledBytes() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val storage = DocumentStorage(context)
        val original = ByteArray(160 * 1024) { 7 }
        val source = File(storage.root, "changed-${System.nanoTime()}").apply { parentFile?.mkdirs(); writeBytes(original) }
        TestDocumentsProvider.file = File(context.cacheDir, "destination-${System.nanoTime()}").apply { writeBytes(ByteArray(0)) }
        TestDocumentsProvider.nonSeekable = false
        var checks = 0
        assertThrows(CancellationException::class.java) { runBlocking { storage.export(source.absolutePath, uri) { if (++checks > 1) throw CancellationException() } } }
        TestDocumentsProvider.file.writeBytes(ByteArray(TestDocumentsProvider.file.length().toInt()) { 3 })
        val changed = ByteArray(160 * 1024) { 9 }
        source.writeBytes(changed)
        storage.export(source.absolutePath, uri) {}
        assertArrayEquals(changed, TestDocumentsProvider.file.readBytes())
        TestDocumentsProvider.revoked = true
        assertThrows(Exception::class.java) { runBlocking { storage.export(source.absolutePath, uri) {} } }
        TestDocumentsProvider.revoked = false
    }
}
