package io.filebeam.android.platform.services

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import io.filebeam.android.platform.TransferCoordinator

sealed interface InboxItemState {
    data object Locked : InboxItemState
    data object NotSetup : InboxItemState
    data object Expired : InboxItemState
    data object UserDisabled : InboxItemState
    data object InstanceDisabled : InboxItemState
    data class Error(val message: String) : InboxItemState
}

/** Inbox metadata stays opaque until the user explicitly opens a delivery. */
data class InboxItem(val id: String, val title: String, val link: String, val receivedAtMillis: Long, val state: InboxItemState = InboxItemState.Locked) {
    val error: String? get() = (state as? InboxItemState.Error)?.message
}

interface InboxService {
    val state: StateFlow<ServiceState<List<InboxItem>>>
    suspend fun refresh(instance: String, password: String? = null): List<InboxItem>
    suspend fun download(instance: String, transferId: String, password: String? = null)
    suspend fun acknowledgeNotificationsRead(instance: String)
}

class NativeInboxService(private val sessions: AccountSessionRegistry, private val accounts: AccountService, private val transfers: TransferCoordinator? = null) : InboxService {
    private val mutable = MutableStateFlow<ServiceState<List<InboxItem>>>(ServiceState.Loading)
    override val state: StateFlow<ServiceState<List<InboxItem>>> = mutable.asStateFlow()

    override suspend fun refresh(instance: String, password: String?): List<InboxItem> = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        check(sessions.cookie(origin) != null) { "Sign in to view your inbox" }
        sessions.service(origin).accountInbox().map { item ->
            InboxItem(item.id, "${item.itemCount} encrypted files", "authenticated", 0)
        }.also { mutable.value = ServiceState.Ready(it) }
    }

    override suspend fun download(instance: String, transferId: String, password: String?) = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        val service = sessions.service(origin)
        val metadata = service.accountInboxMetadata(transferId)
        val bundle = metadata.recipientKey.bundle
        val privateKey = accounts.privateKeyForInbox(bundle.id, bundle.userId, bundle.custodyMode, bundle.encryptedPrivateKey, bundle.publicKey, password)
        try {
            // Native opens the HPKE envelope and passes only the working key to the job.
            val workingKey = service.accountOpenInboxKey(transferId, privateKey)
            try {
                requireNotNull(transfers) { "Inbox downloads are unavailable" }.inboxDownload(origin, transferId, workingKey, service.accountCookieContext())
            } finally {
                workingKey.fill(0)
            }
        } finally {
            privateKey.fill(0)
        }
    }

    override suspend fun acknowledgeNotificationsRead(instance: String) = withContext(Dispatchers.IO) {
        val origin = AccountSessionRegistry.normalizeOrigin(instance)
        check(sessions.cookie(origin) != null) { "Sign in to acknowledge inbox notifications" }
        sessions.service(origin).accountMarkInboxNotificationsRead()
    }
}
