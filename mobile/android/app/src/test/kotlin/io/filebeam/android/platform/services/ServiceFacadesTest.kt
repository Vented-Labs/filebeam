package io.filebeam.android.platform.services

import org.junit.Assert.assertTrue
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.json.JSONObject
import io.filebeam.android.platform.resumeRequest
import io.filebeam.android.platform.canControlActive
import io.filebeam.rust.NativeRuntime
import io.filebeam.rust.NoHandle

class ServiceFacadesTest {
    @Test fun inboxStartsLoadingUntilTheNativeNetworkCallIsRequested() {
        val service = NativeInboxService(AccountSessionRegistry(false, NativeRuntime(NoHandle)), object : AccountService {
            override val state = kotlinx.coroutines.flow.MutableStateFlow<ServiceState<AccountSummary>>(ServiceState.Loading)
            override suspend fun signIn(instance: String, username: String, password: String) = Unit
            override suspend fun signUp(instance: String, username: String, name: String?, email: String, password: String) = Unit
            override suspend fun resume(instance: String) = Unit
            override suspend fun signOut() = Unit
            override suspend fun generateKey() = ""
            override suspend fun exportKey() = ""
            override suspend fun importKey(value: String) = Unit
            override suspend fun privateKeyForInbox(bundleId: ULong, userId: ULong, custodyMode: String, envelope: String?, publicKey: String, password: String?) = ByteArray(32)
            override suspend fun rememberInboxKey(bundleId: ULong, privateKey: ByteArray) = Unit
            override suspend fun inboxDownloadCredentials(instance: String, transferId: String) = InboxDownloadCredentials(ByteArray(32), "session=opaque")
        })
        assertTrue(service.state.value is ServiceState.Loading)
    }

    @Test fun sessionOriginsAreNormalizedBeforeTheyAreShared() {
        assertTrue(AccountSessionRegistry.normalizeOrigin("HTTPS://Filebeam.test:443/") == "https://filebeam.test")
        assertEquals("https://filebeam.test:8443", AccountSessionRegistry.normalizeOrigin("https://filebeam.test:8443/"))
        assertThrows(IllegalArgumentException::class.java) {
            AccountSessionRegistry.normalizeOrigin("https://user@filebeam.test")
        }
    }

    @Test fun inboxResumePreservesItsOriginAndHistoricalTransferWithoutCredentials() {
        val request = resumeRequest("checkpoint", JSONObject()
            .put("kind", "inbox-download").put("id", "job")
            .put("instance", "https://inbox.example").put("transfer", "delivery"),
            "https://other.example", false)
        assertEquals("inbox-download", request.getString("kind"))
        assertEquals("https://inbox.example", request.getString("instance"))
        assertEquals("delivery", request.getString("transfer"))
        assertTrue(!request.has("cookie") && !request.has("password") && !request.has("workingKey"))
    }

    @Test fun activeEndControlsOnlyItsWorkerAndRejectsASecondIntent() {
        assertTrue(canControlActive("checkpoint", "checkpoint", null))
        assertTrue(!canControlActive("other", "checkpoint", null))
        assertTrue(!canControlActive("checkpoint", "checkpoint", "live-end" to "checkpoint"))
    }

    @Test fun generatedKeyIsExportedAndPersistedBeforeItIsZeroed() {
        val generated = ByteArray(32) { 9 }
        var exported: ByteArray? = null
        var persisted: ByteArray? = null
        assertEquals("fbsk1.key", commitGeneratedKey(generated, { 7uL }, { key ->
            exported = key.copyOf()
            "fbsk1.key"
        }) { _, key -> persisted = key.copyOf() })
        assertArrayEquals(ByteArray(32) { 9 }, exported)
        assertArrayEquals(ByteArray(32) { 9 }, persisted)
        assertArrayEquals(ByteArray(32), generated)
    }
}
