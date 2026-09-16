package io.filebeam.android.ui

import io.filebeam.rust.Transport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StateModelsTest {
    @Test fun retentionRejectsZeroAndOverflowInsteadOfDefaulting() {
        assertFalse(retentionHours("0").isSuccess)
        assertFalse(retentionHours("18446744073709551616").isSuccess)
        assertEquals(24uL, retentionHours("24").getOrThrow())
        assertEquals(null, retentionHours("").getOrThrow())
    }

    @Test fun recipientConflictIsRetainedAndBlocksIncompatibleModes() {
        val draft = SendDraft(transport = Transport.WEB_RTC, recipient = RecipientDraft("alex", RecipientStatus.UNVALIDATED))
        assertTrue(draft.recipientConflict()!!.contains("HTTP"))
        assertEquals("alex", draft.recipient.username)
    }

    @Test fun nestedNavigationReturnsToActualPreviousDestination() {
        val state = NavigationState().push(DestinationRoute.Receive).push(DestinationRoute.Settings)
        assertEquals(DestinationRoute.Receive, state.back().current)
        assertEquals(DestinationRoute.Send, state.back().back().current)
    }

    @Test fun legacyRoutesAreSafeAndOnlyKnownRoutesMap() {
        assertEquals(DestinationRoute.Send, DestinationRoute.fromLegacy("notes/editor"))
        assertEquals(DestinationRoute.Send, DestinationRoute.fromLegacy("untrusted"))
    }
}
