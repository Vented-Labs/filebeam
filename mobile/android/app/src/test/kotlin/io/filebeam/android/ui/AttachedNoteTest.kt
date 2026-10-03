package io.filebeam.android.ui

import org.junit.Assert.assertNull
import org.junit.Assert.assertNotNull
import org.junit.Test

class AttachedNoteTest {
    @Test fun attachmentBoundsUseUtf8AndCodePoints() {
        val valid = NoteDraft(body = "🦀".repeat(16384), title = "🦀".repeat(160), language = "markdown")
        assertNull(valid.attachmentError())
        assertNotNull(valid.copy(body = valid.body + "x").attachmentError())
        assertNotNull(valid.copy(title = valid.title + "x").attachmentError())
        assertNotNull(valid.copy(body = "").attachmentError())
        assertNotNull(valid.copy(language = "unknown").attachmentError())
        assertNull(valid.copy(body = " \n\t").attachmentError())
    }
}
