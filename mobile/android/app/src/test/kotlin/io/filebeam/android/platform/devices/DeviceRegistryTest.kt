package io.filebeam.android.platform.devices

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertThrows
import org.junit.Test

class DeviceRegistryTest {
    @Test fun unavailableAdapterDoesNotFabricateDirectory() {
        assertThrows(DeviceRegistryUnavailable::class.java) {
            runBlocking { UnavailableDeviceRegistry.directory("https://example.test") }
        }
    }
}
