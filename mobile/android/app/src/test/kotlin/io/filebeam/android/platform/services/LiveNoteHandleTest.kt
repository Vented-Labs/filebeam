package io.filebeam.android.platform.services

import org.junit.Assert.assertEquals
import org.junit.Test

class LiveNoteHandleTest {
    @Test fun keylessPresentationKeepsTheSameOpaqueLiveNoteIdentity() {
        assertEquals("note-123", noteTransferId("https://example.test/note/note-123"))
        assertEquals("note-123", noteTransferId("https://example.test/note/note-123#secret-not-used-as-a-handle"))
    }
}
