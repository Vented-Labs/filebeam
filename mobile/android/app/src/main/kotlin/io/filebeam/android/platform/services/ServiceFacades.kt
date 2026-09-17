package io.filebeam.android.platform.services

import io.filebeam.rust.NoteRequest as NativeNoteRequest
import io.filebeam.rust.splitShareLink
import io.filebeam.rust.NoteTransport
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay

sealed interface ServiceState<out T> {
    data object Loading : ServiceState<Nothing>
    data class Ready<T>(val value: T) : ServiceState<T>
    data class Unavailable(val reason: String) : ServiceState<Nothing>
    data class Failed(val message: String) : ServiceState<Nothing>
}

data class NoteSummary(val id: String, val title: String, val expiresAtMillis: Long?, val link: String? = null, val separateKey: String? = null)
data class NoteRequest(
    val instance: String,
    val text: String,
    val title: String = "",
    val language: String = "plain",
    val password: String? = null,
    val retentionHours: ULong? = null,
    val live: Boolean = false,
    val burnAfterRead: Boolean,
    val includeKeyInLink: Boolean = true,
)
data class NoteContent(val id: String, val text: String, val title: String?, val language: String, val consumed: Boolean)
data class TurboAvailability(val available: Boolean, val detail: String? = null)
interface NotesService {
    val state: StateFlow<ServiceState<List<NoteSummary>>>
    suspend fun create(request: NoteRequest): NoteSummary
    suspend fun claim(link: String, password: String? = null): NoteContent
    fun endLive(link: String)
}

interface TurboService {
    val state: StateFlow<ServiceState<TurboAvailability>>
    suspend fun refresh(instance: String, transferId: String): TurboAvailability
}

class NativeNotesService(
    private val sessions: AccountSessionRegistry,
    private val startLive: (String, NativeNoteRequest) -> io.filebeam.rust.TransferJob,
    private val createHttp: suspend (suspend () -> io.filebeam.rust.CreatedNote) -> io.filebeam.rust.CreatedNote,
) : NotesService {
    private val mutable = MutableStateFlow<ServiceState<List<NoteSummary>>>(ServiceState.Loading)
    private val liveJobs = mutableMapOf<String, io.filebeam.rust.TransferJob>()
    override val state = mutable.asStateFlow()
    override suspend fun create(request: NoteRequest): NoteSummary = withContext(Dispatchers.IO) {
        val native = NativeNoteRequest(
            text = request.text,
            title = request.title.ifBlank { null },
            language = request.language,
            password = request.password?.takeIf { it.isNotEmpty() },
            burnOnRead = request.burnAfterRead,
            retentionHours = request.retentionHours,
            transport = if (request.live) NoteTransport.WEB_RTC else NoteTransport.HTTP,
        )
        val created = if (request.live) {
            val job = startLive(request.instance, native)
            var created: io.filebeam.rust.CreatedNote? = null
            while (created == null) {
                val snapshot = job.snapshot()
                snapshot.shareUrl?.let { ready ->
                    liveJobs[ready] = job
                    created = io.filebeam.rust.CreatedNote(transferId(ready), ready, "")
                }
                snapshot.error?.let { error(it) }
                delay(100)
            }
            requireNotNull(created)
        } else createHttp { sessions.service(request.instance).createNote(native) }
        val presentation = if (request.includeKeyInLink) null else splitShareLink(request.instance, created.link)
        NoteSummary(created.id, request.title.ifBlank { "Encrypted note" }, null, presentation?.link ?: created.link, presentation?.separateKey).also {
            mutable.value = ServiceState.Ready(listOf(it))
        }
    }
    override suspend fun claim(link: String, password: String?): NoteContent = withContext(Dispatchers.IO) {
        // Passwords deliberately stay caller-owned memory. A process restart therefore prompts again.
        val opened = sessions.service(originForLink(link)).openNote(link, password?.takeIf { it.isNotEmpty() })
        mutable.value = ServiceState.Ready(listOf(NoteSummary(opened.id, opened.title ?: "Encrypted note", null)))
        NoteContent(opened.id, opened.text, opened.title, opened.language, opened.consumed)
    }
    override fun endLive(link: String) { liveJobs.remove(link)?.endLive() }
}

class NativeTurboService(private val sessions: AccountSessionRegistry) : TurboService {
    private val mutable = MutableStateFlow<ServiceState<TurboAvailability>>(ServiceState.Loading)
    override val state = mutable.asStateFlow()
    override suspend fun refresh(instance: String, transferId: String): TurboAvailability = withContext(Dispatchers.IO) {
        val value = sessions.service(instance).turboAvailability(transferId)
        TurboAvailability(value.status != "unavailable", value.status).also { mutable.value = ServiceState.Ready(it) }
    }
}

private fun transferId(link: String): String = link.substringBefore('#').substringAfterLast('/').also {
    require(it.isNotBlank()) { "Enter a note transfer link" }
}
private fun originForLink(link: String): String = java.net.URI(link.substringBefore('#')).let { uri ->
    require(uri.scheme in setOf("https", "http") && !uri.authority.isNullOrBlank()) { "Enter a note transfer link" }
    "${uri.scheme}://${uri.authority}"
}
