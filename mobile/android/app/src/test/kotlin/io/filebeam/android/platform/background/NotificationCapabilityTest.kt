package io.filebeam.android.platform.background

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NotificationCapabilityTest {
    @Test fun deniedAndroid13NotificationPermissionSuppressesLocalCompletionAlert() {
        assertFalse(TransferScheduler.notificationsAllowed(33, false))
        assertTrue(TransferScheduler.notificationsAllowed(33, true))
        assertTrue(TransferScheduler.notificationsAllowed(32, false))
    }
}
