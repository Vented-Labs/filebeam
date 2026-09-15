package io.filebeam.android

import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.filebeam.android.platform.storage.DocumentStorage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

@RunWith(AndroidJUnit4::class)
class DocumentProviderRecoveryTest {
    private val uri = Uri.parse("content://io.filebeam.android.testdocuments/document/seekable")

    @Test fun providerInputReopensAndRejectsMutationOrRevokedGrant() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        TestDocumentsProvider.file = File(context.cacheDir, "provider-${System.nanoTime()}").apply { writeBytes(ByteArray(32) { it.toByte() }) }
        TestDocumentsProvider.revoked = false
        val storage = DocumentStorage(context)
        val input = storage.providerInput(uri, "tree/file.bin")
        assertEquals(32, input.length)
        val fd = storage.openProviderDescriptor(input.identity)
        android.os.ParcelFileDescriptor.adoptFd(fd).close()
        TestDocumentsProvider.file.appendBytes(byteArrayOf(1))
        assertNotEquals(input.mutationToken, storage.providerMutationToken(input.identity))
        TestDocumentsProvider.revoked = true
        assertThrows(Exception::class.java) { storage.openProviderDescriptor(input.identity) }
    }
}
