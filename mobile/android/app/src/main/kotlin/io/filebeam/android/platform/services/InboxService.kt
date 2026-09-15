package io.filebeam.android.platform.services

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext

data class InboxItem(val id: String, val title: String, val link: String, val receivedAtMillis: Long, val error: String? = null)

interface InboxService {
    val state: StateFlow<ServiceState<List<InboxItem>>>
    suspend fun refresh(instance: String, password: String? = null): List<InboxItem>
}

class NativeInboxService(private val sessions: AccountSessionRegistry, private val accounts: AccountService) : InboxService {
    private val mutable = MutableStateFlow<ServiceState<List<InboxItem>>>(ServiceState.Loading)
    override val state: StateFlow<ServiceState<List<InboxItem>>> = mutable.asStateFlow()

    override suspend fun refresh(instance: String, password: String?): List<InboxItem> = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        check(sessions.cookie(origin) != null) { "Sign in to view your inbox" }
        sessions.service(origin).accountInbox().map { item ->
            runCatching {
                val metadata = sessions.service(origin).accountInboxMetadata(item.id)
                val bundle = metadata.recipientKey.bundle
                val privateKey = accounts.privateKeyForInbox(bundle.id, bundle.userId, bundle.custodyMode, bundle.encryptedPrivateKey, bundle.publicKey, password)
                try {
                    val opened = sessions.service(origin).accountOpenInbox(item.id, privateKey)
                    accounts.rememberInboxKey(opened.keyBundleId, privateKey)
                    InboxItem(item.id, opened.filenames.joinToString().ifBlank { "${item.itemCount} encrypted files" }, opened.downloadLink, 0)
                } finally {
                    privateKey.fill(0)
                }
            }.getOrElse { error ->
                InboxItem(item.id, "${item.itemCount} encrypted files", "", 0, error.message ?: "Could not open inbox delivery")
            }
        }.also { mutable.value = ServiceState.Ready(it) }
    }
}
