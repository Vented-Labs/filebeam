package io.filebeam.android.platform.services

import org.junit.Assert.assertTrue
import org.junit.Test

class ServiceFacadesTest {
    @Test fun inboxStartsLoadingUntilTheNativeNetworkCallIsRequested() {
        val service = NativeInboxService(false)
        assertTrue(service.state.value is ServiceState.Loading)
    }
}
