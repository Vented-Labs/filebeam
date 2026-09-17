package io.filebeam.android.platform

import io.filebeam.rust.JobState
import io.filebeam.rust.Transport
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TransferPresentationTest {
    @Test fun currentUploadCarriesActualKindTransportAndOnlySafeActiveAction() {
        val current = currentPresentation(JSONObject().put("id", "job-1").put("kind", "upload").put("live", true), JobState.RUNNING)
        assertEquals(TransferKind.UPLOAD, current.kind)
        assertEquals(Transport.WEB_RTC, current.transport)
        assertTrue(current.actions.pause)
        assertFalse(current.actions.revokeRemote)
    }

    @Test fun notificationRoutesNeverNeedLinkData() {
        assertEquals("transfer:job-1", io.filebeam.android.platform.background.TransferScheduler.notificationRoute(
            TransferUiState(current = CurrentTransfer("job-1", TransferKind.DOWNLOAD, null, TransferActions()),
                snapshot = null)))
    }
}
