package io.filebeam.android.platform.services

import io.filebeam.rust.NativeServices
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers

sealed interface ServiceState<out T> {
    data object Loading : ServiceState<Nothing>
    data class Ready<T>(val value: T) : ServiceState<T>
    data class Unavailable(val reason: String) : ServiceState<Nothing>
    data class Failed(val message: String) : ServiceState<Nothing>
}

data class AccountSummary(val username: String, val instance: String)
data class NoteSummary(val id: String, val title: String, val expiresAtMillis: Long?)
data class NoteRequest(val instance: String, val text: String, val burnAfterRead: Boolean)
data class NoteContent(val id: String, val text: String, val consumed: Boolean)
data class TurboAvailability(val available: Boolean, val detail: String? = null)
data class TurboDownloadRequest(val instance: String, val descriptor: String)
data class TurboDownloadSession(val id: String, val expiresAtMillis: Long?)
data class InboxItem(val id: String, val title: String, val link: String, val receivedAtMillis: Long)

interface AccountService {
    val state: StateFlow<ServiceState<AccountSummary>>
    suspend fun signIn(instance: String, username: String, password: String)
    suspend fun signOut()
    suspend fun exportKey(): String
    suspend fun importKey(value: String)
}

interface NotesService {
    val state: StateFlow<ServiceState<List<NoteSummary>>>
    suspend fun create(request: NoteRequest): NoteSummary
    suspend fun claim(link: String): NoteContent
}

interface TurboService {
    val state: StateFlow<ServiceState<TurboAvailability>>
    suspend fun refresh(instance: String, transferId: String): TurboAvailability
    suspend fun open(request: TurboDownloadRequest): TurboDownloadSession
}

interface InboxService {
    val state: StateFlow<ServiceState<List<InboxItem>>>
    suspend fun refresh(instance: String): List<InboxItem>
}

private class NativeServicePool(private val allowHttp: Boolean) {
    private var instance: String? = null
    private var services: NativeServices? = null

    @Synchronized fun get(origin: String): NativeServices {
        if (instance != origin) {
            services?.destroy()
            services = NativeServices(origin, allowHttp)
            instance = origin
        }
        return requireNotNull(services)
    }
}

class NativeAccountService(private val allowHttp: Boolean) : AccountService {
    private val pool = NativeServicePool(allowHttp)
    private val mutable = MutableStateFlow<ServiceState<AccountSummary>>(ServiceState.Loading)
    override val state = mutable.asStateFlow()
    override suspend fun signIn(instance: String, username: String, password: String) = withContext(Dispatchers.IO) {
        val session = pool.get(instance).accountLogin(username, password, true)
        mutable.value = ServiceState.Ready(AccountSummary(session.username ?: session.name, instance))
    }
    override suspend fun signOut() = withContext(Dispatchers.IO) {
        pool.get(requireInstance()).accountLogout()
        mutable.value = ServiceState.Loading
    }
    // FFI has no fbsk1 import/export binding yet; do not serialize or invent private key bytes here.
    override suspend fun exportKey(): String = unavailable()
    override suspend fun importKey(value: String): Unit = unavailable()
    private fun requireInstance() = (state.value as? ServiceState.Ready)?.value?.instance
        ?: throw IllegalStateException("Sign in before signing out")
}

class NativeNotesService(private val allowHttp: Boolean) : NotesService {
    private val pool = NativeServicePool(allowHttp)
    private val mutable = MutableStateFlow<ServiceState<List<NoteSummary>>>(ServiceState.Loading)
    override val state = mutable.asStateFlow()
    override suspend fun create(request: NoteRequest): NoteSummary = unavailable()
    override suspend fun claim(link: String): NoteContent = withContext(Dispatchers.IO) {
        val metadata = pool.get(originForLink(link)).readNote(transferId(link))
        mutable.value = ServiceState.Ready(listOf(NoteSummary(metadata.id, metadata.status, null)))
        // Note decryption is intentionally not represented by this metadata-only binding.
        NoteContent(metadata.id, metadata.encryptedManifest ?: "", false)
    }
}

class NativeTurboService(private val allowHttp: Boolean) : TurboService {
    private val pool = NativeServicePool(allowHttp)
    private val mutable = MutableStateFlow<ServiceState<TurboAvailability>>(ServiceState.Loading)
    override val state = mutable.asStateFlow()
    override suspend fun refresh(instance: String, transferId: String): TurboAvailability = withContext(Dispatchers.IO) {
        val value = pool.get(instance).turboAvailability(transferId)
        TurboAvailability(value.status != "unavailable", value.status).also { mutable.value = ServiceState.Ready(it) }
    }
    override suspend fun open(request: TurboDownloadRequest): TurboDownloadSession = withContext(Dispatchers.IO) {
        val session = pool.get(request.instance).turboCreateDownloadSession(request.descriptor, emptyList())
        TurboDownloadSession(session.id, null)
    }
}

class NativeInboxService(private val allowHttp: Boolean) : InboxService {
    private val pool = NativeServicePool(allowHttp)
    private val mutable = MutableStateFlow<ServiceState<List<InboxItem>>>(ServiceState.Loading)
    override val state = mutable.asStateFlow()
    override suspend fun refresh(instance: String): List<InboxItem> = withContext(Dispatchers.IO) {
        pool.get(instance).accountInbox().map { item ->
            InboxItem(item.id, "${item.itemCount} encrypted files", item.id, 0)
        }.also { mutable.value = ServiceState.Ready(it) }
    }
}

private fun transferId(link: String): String = link.substringBefore('#').substringAfterLast('/').also {
    require(it.isNotBlank()) { "Enter a note transfer link" }
}
private fun originForLink(link: String): String = java.net.URI(link.substringBefore('#')).let { uri ->
    require(uri.scheme in setOf("https", "http") && !uri.authority.isNullOrBlank()) { "Enter a note transfer link" }
    "${uri.scheme}://${uri.authority}"
}
private fun unavailable(): Nothing = throw UnsupportedOperationException("The installed native binding does not expose this operation")
