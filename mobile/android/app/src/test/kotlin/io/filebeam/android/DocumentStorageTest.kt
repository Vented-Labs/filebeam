package io.filebeam.android

import io.filebeam.android.platform.storage.DocumentStorage
import org.junit.Assert.assertEquals
import org.junit.Test

class DocumentStorageTest {
    @Test fun providerNamesCannotEscapeTheirPrivateDirectory() {
        assertEquals(".._.._report.txt", DocumentStorage.safeName("../../report.txt"))
        assertEquals("download", DocumentStorage.safeName(".."))
        assertEquals("report_2026.txt", DocumentStorage.safeName("report\u00002026.txt"))
    }
}
