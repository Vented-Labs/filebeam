package io.filebeam.android

import io.filebeam.android.platform.storage.DocumentStorage
import org.junit.Assert.assertEquals
import org.junit.Test

class DocumentStorageParityTest {
    @Test fun tree_paths_are_flattened_without_parent_escapes() {
        assertEquals("photos/2026/report.txt", DocumentStorage.safeRelativePath("photos/../2026/report.txt"))
        assertEquals("download", DocumentStorage.safeRelativePath("../"))
    }
}
