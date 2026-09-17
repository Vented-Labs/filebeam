package io.filebeam.android.ui

import io.filebeam.rust.Transport
import io.filebeam.android.ui.send.formatFileSize
import io.filebeam.android.ui.send.transferOptionsSummary
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

    @Test fun selectingPrimaryDestinationClearsNestedBackStack() {
        val state = NavigationState().push(DestinationRoute.Receive).push(DestinationRoute.Settings)
            .selectPrimary(DestinationRoute.Inbox)
        assertEquals(DestinationRoute.Inbox, state.current)
        assertTrue(state.backStack.isEmpty())
    }

    @Test fun delayedRestoreCannotOverwriteTheFirstQueuedEdit() {
        val revisionBeforeDelayedRead = 4L
        val firstEditQueuedWhileReadBlocked = 5L
        assertFalse(shouldRestoreDraft(revisionBeforeDelayedRead, firstEditQueuedWhileReadBlocked))
        assertTrue(shouldRestoreDraft(revisionBeforeDelayedRead, revisionBeforeDelayedRead))
    }

    @Test fun fileAndNoteKeyPresentationPreferencesStaySeparate() {
        val files = SendDraft(includeKeyInLink = false)
        val notes = NoteDraft(includeKeyInLink = true)
        assertFalse(files.includeKeyInLink)
        assertTrue(notes.includeKeyInLink)
    }

    @Test fun knownIndividualSizesUseNativeAeadEstimateWhileZipStaysUnknown() {
        val policy = policy(maximumBytes = 100u)
        assertEquals(SendLimitStatus.WithinKnownLimits, fileLimitStatus(policy, null, false, listOf(10)))
        assertEquals(SendLimitStatus.UnknownSize, fileLimitStatus(policy, null, true, listOf(10)))
    }

    @Test fun unavailableSelectedDriverIsNeverReplacedWithTheDefault() {
        assertTrue(fileLimitStatus(policy(), "webrtc", false, listOf(1)) is SendLimitStatus.Exceeds)
    }

    @Test fun policyRejectsKnownDriverCountAndNoteByteConflicts() {
        val policy = policy(maximumFiles = 1u, maximumNoteBytes = 2u)
        assertTrue(fileLimitStatus(policy, null, false, listOf(null, null)) is SendLimitStatus.Exceeds)
        assertTrue(NoteDraft(body = "too long").limitStatus(policy) is SendLimitStatus.Exceeds)
    }

    @Test fun instanceInputStaysUncommittedUntilACheckedTransactionCommits() {
        val committed = io.filebeam.android.platform.AppSettings("https://one.example")
        val transaction = InstanceSettingsTransaction(committed).withInput("https://two.example", true)
        assertEquals("https://one.example", transaction.committed.instance)
        assertTrue(transaction.dirty)
        assertEquals(InstanceTransactionStatus.Idle, transaction.status)
    }

    @Test fun legacyRoutesAreSafeAndOnlyKnownRoutesMap() {
        assertEquals(DestinationRoute.Send, DestinationRoute.fromLegacy("notes/editor"))
        assertEquals(DestinationRoute.Send, DestinationRoute.fromLegacy("untrusted"))
    }

    @Test fun sendOptionSummaryKeepsEachCommittedChoice() {
        val summary = transferOptionsSummary(SendDraft(archive = true, passwordProtected = true, retentionHours = "48"))
        assertTrue(summary.contains("48 hours"))
        assertTrue(summary.contains("Password on"))
        assertTrue(summary.contains("ZIP on"))
    }

    @Test fun fileMetadataFormatsKnownAndUnknownSizesWithoutInventingZero() {
        assertEquals("1.0 KB", formatFileSize(1024))
        assertEquals("0 B", formatFileSize(0))
    }

    @Test fun compactNavigationHidesLabelsBeforeAccessibilityTextWouldWrap() {
        assertTrue(shouldShowCompactNavigationLabels(1f))
        assertFalse(shouldShowCompactNavigationLabels(2f))
    }
}

private fun policy(maximumBytes: ULong? = null, maximumFiles: ULong? = null, maximumNoteBytes: ULong? = null) = SendInstancePolicy(
    SendDiscoveryKey("https://example.test", null), true, setOf(Transport.HTTP), 24u, setOf(24u), "http", 10u,
    mapOf("http" to SendDriverPolicy("http", maximumBytes, maximumFiles, maximumNoteBytes)),
)
