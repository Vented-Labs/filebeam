package io.filebeam.android.ui.feature.receive

import android.app.Application
import android.net.Uri
import androidx.compose.runtime.Immutable
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.filebeam.android.FilebeamApplication
import io.filebeam.android.platform.PreparedDownload
import io.filebeam.android.platform.TransferKind
import io.filebeam.android.platform.TransferUiState
import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.rust.DownloadItem
import io.filebeam.rust.JobState
import io.filebeam.rust.LinkInspection
import io.filebeam.rust.ManagedNoteReceive
import io.filebeam.rust.NoteInspection
import io.filebeam.rust.NoteReceiveRequest
import io.filebeam.rust.OpenedNote
import io.filebeam.rust.PromptType
import io.filebeam.rust.TransferJob
import io.filebeam.rust.presentShareLink
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/** Where an incoming link is on its way from pasted text to saved files or an open note. */
sealed interface ReceiveStage {
    data object Idle : ReceiveStage
    data object Inspecting : ReceiveStage

    /** The link carries no key; the sender shared it separately. */
    data class NeedKey(val error: Boolean = false) : ReceiveStage
    data class NeedPassword(val error: Boolean = false) : ReceiveStage

    /** A live transfer wants to connect peers directly, which reveals network addresses. */
    data class NeedConsent(val peer: String?) : ReceiveStage
    data class Unlocking(val phase: String) : ReceiveStage
    data class NoteLocked(val burnOnRead: Boolean) : ReceiveStage
    data class Files(val items: List<DownloadItem>, val downloaded: Set<String> = emptySet(), val selectable: Boolean = true) : ReceiveStage
    data class Downloading(val items: List<DownloadItem>, val selection: Set<String>?) : ReceiveStage
    data class Saved(val items: List<DownloadItem>, val files: List<File>) : ReceiveStage
    data class Note(val note: OpenedNote, val burnPending: Boolean) : ReceiveStage
    data class Unavailable(val message: String?) : ReceiveStage
}

@Immutable
data class ReceiveUiState(
    val input: String = "",
    val stage: ReceiveStage = ReceiveStage.Idle,
    val inspection: LinkInspection? = null,
    val live: Boolean = false,
    val attachedNote: io.filebeam.rust.AttachedNote? = null,
    val reportSent: Boolean = false,
    val reportBusy: Boolean = false,
    val reportError: String? = null,
)

private val keyPattern = Regex("^(?:v1\\.)?[A-Za-z0-9_-]{43}$")

/**
 * Drives receiving. Files are unlocked first (key, password, consent) so their names and sizes
 * can be shown and chosen before any payload is fetched; notes are decrypted only on request.
 * Keys and passwords stay in memory for the life of one attempt.
 */
class ReceiveViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as FilebeamApplication
    private val coordinator = app.transfers
    private val mutable = MutableStateFlow(ReceiveUiState())
    val state: StateFlow<ReceiveUiState> = mutable.asStateFlow()
    val transfer: StateFlow<TransferUiState> = coordinator.state

    private var link: String = ""
    private var password: String? = null
    private var consent: Boolean = false
    private var prepared: PreparedDownload? = null
    private var checkpoint: String? = null
    private var noteJob: ManagedNoteReceive? = null
    private var work: Job? = null
    private var answered: ULong? = null

    init {
        // A direct download started here reports through the shared coordinator.
        viewModelScope.launch {
            coordinator.state.collect { transfer ->
                val stage = mutable.value.stage as? ReceiveStage.Downloading ?: return@collect
                val snapshot = transfer.snapshot ?: return@collect
                val prompt = snapshot.prompt
                if (prompt != null && prompt.id != answered) answerCoordinatorPrompt(prompt.id, prompt.kind, prompt.peer)
                transfer.attachedNote?.let { note -> mutable.update { it.copy(attachedNote = note) } }
                if (transfer.busy) return@collect
                when (snapshot.state) {
                    JobState.COMPLETE -> {
                        val files = prepared?.let { coordinator.outputs(it.id) } ?: snapshot.results.map(::File)
                        mutable.update { it.copy(stage = ReceiveStage.Saved(stage.items, files)) }
                    }
                    JobState.FAILED -> mutable.update { it.copy(stage = ReceiveStage.Unavailable(snapshot.error)) }
                    else -> Unit
                }
            }
        }
    }

    fun setInput(value: String) {
        if (mutable.value.stage !is ReceiveStage.Idle && mutable.value.stage !is ReceiveStage.Unavailable) return
        mutable.update { it.copy(input = value, stage = ReceiveStage.Idle) }
    }

    /** Looks the link up on its instance and picks the files or the note path. */
    fun open(value: String = mutable.value.input) {
        val trimmed = value.trim()
        if (trimmed.isEmpty()) return
        reset(keepInput = false)
        link = trimmed
        mutable.update { ReceiveUiState(input = trimmed, stage = ReceiveStage.Inspecting) }
        coordinator.inspectLink(trimmed) { result ->
            result.onSuccess { inspection ->
                val live = inspection.driver == "webrtc"
                mutable.update { it.copy(inspection = inspection, live = live) }
                if (inspection.status in setOf("expired", "deleted", "removed", "ended", "burned", "abandoned", "cleanup")) {
                    mutable.update { it.copy(stage = ReceiveStage.Unavailable(null)) }
                    return@onSuccess
                }
                when {
                    !hasKey(link) -> mutable.update { it.copy(stage = ReceiveStage.NeedKey()) }
                    inspection.passwordRequired && password == null -> mutable.update { it.copy(stage = ReceiveStage.NeedPassword()) }
                    else -> proceed()
                }
            }.onFailure { failure ->
                mutable.update { it.copy(stage = ReceiveStage.Unavailable(failure.message)) }
            }
        }
    }

    fun submitKey(key: String) {
        val inspection = mutable.value.inspection ?: return
        val value = key.trim()
        if (!keyPattern.matches(value)) {
            mutable.update { it.copy(stage = ReceiveStage.NeedKey(error = true)) }
            return
        }
        val combined = runCatching { presentShareLink(inspection.instance, link.substringBefore('#'), if (value.startsWith("v1.")) value else "v1.$value", true).link }.getOrNull()
        if (combined == null) {
            mutable.update { it.copy(stage = ReceiveStage.NeedKey(error = true)) }
            return
        }
        link = combined
        if (inspection.passwordRequired && password == null) mutable.update { it.copy(stage = ReceiveStage.NeedPassword()) } else proceed()
    }

    /** Returns to the key step, forgetting the key that was entered. */
    fun changeKey() {
        cancelWork()
        link = link.substringBefore('#')
        password = null
        mutable.update { it.copy(stage = ReceiveStage.NeedKey()) }
    }

    fun submitPassword(value: String) {
        if (value.isEmpty()) return
        password = value
        proceed()
    }

    fun submitConsent(allowed: Boolean) {
        if (!allowed) {
            cancelWork()
            mutable.update { it.copy(stage = ReceiveStage.Idle) }
            return
        }
        consent = true
        viewModelScope.launch { app.appearance.acceptPeerExposure(System.currentTimeMillis()) }
        proceed()
    }

    private fun proceed() {
        val inspection = mutable.value.inspection ?: return
        if (inspection.kind.startsWith("note")) inspectNote() else unlockFiles()
    }

    // region Files

    private fun unlockFiles() {
        cancelWork()
        prepared?.job?.let { stale -> viewModelScope.launch(Dispatchers.IO) { runCatching { stale.destroy() } } }
        prepared = null
        mutable.update { it.copy(stage = ReceiveStage.Unlocking("unlocking")) }
        work = viewModelScope.launch {
            val preparation = runCatching { coordinator.prepareDownload(link) }.getOrElse { failure ->
                mutable.update { it.copy(stage = ReceiveStage.Unavailable(failure.message)) }
                return@launch
            }
            prepared = preparation
            val outcome = drive(preparation.job)
            if (outcome != JobState.COMPLETE) return@launch
            val id = withContext(Dispatchers.IO) { preparation.job.snapshot().results.firstOrNull() }
            withContext(Dispatchers.IO) { preparation.job.destroy() }
            if (id == null) {
                mutable.update { it.copy(stage = ReceiveStage.Unavailable(null)) }
                return@launch
            }
            checkpoint = id
            runCatching { coordinator.downloadItems(id) }
                .onSuccess { items -> mutable.update { it.copy(stage = ReceiveStage.Files(items)) } }
                .onFailure { failure -> mutable.update { it.copy(stage = ReceiveStage.Unavailable(failure.message)) } }
        }
    }

    /** Polls a native job, answering its prompts from what the user already entered. Returns its final state. */
    private suspend fun drive(job: TransferJob): JobState {
        var lastPrompt: ULong? = null
        while (true) {
            val snapshot = withContext(Dispatchers.IO) { job.snapshot() }
            val prompt = snapshot.prompt
            if (prompt != null && prompt.id != lastPrompt) {
                when (prompt.kind) {
                    PromptType.PASSWORD -> {
                        val known = password
                        if (known == null || snapshot.secretRetry != null) {
                            password = null
                            mutable.update { it.copy(stage = ReceiveStage.NeedPassword(error = snapshot.secretRetry != null)) }
                            withContext(Dispatchers.IO) { job.pause() }
                            return JobState.PAUSED
                        }
                        lastPrompt = prompt.id
                        withContext(Dispatchers.IO) { job.respond(prompt.id, known) }
                    }
                    PromptType.SHARE_KEY -> {
                        mutable.update { it.copy(stage = ReceiveStage.NeedKey(error = snapshot.secretRetry != null)) }
                        withContext(Dispatchers.IO) { job.pause() }
                        return JobState.PAUSED
                    }
                    PromptType.PEER_CONSENT -> {
                        if (!consent && !recentlyConsented()) {
                            mutable.update { it.copy(stage = ReceiveStage.NeedConsent(prompt.peer)) }
                            withContext(Dispatchers.IO) { job.pause() }
                            return JobState.PAUSED
                        }
                        lastPrompt = prompt.id
                        withContext(Dispatchers.IO) { job.respondConsent(prompt.id, true) }
                    }
                    else -> {
                        lastPrompt = prompt.id
                        withContext(Dispatchers.IO) { runCatching { job.respond(prompt.id, "continue") } }
                    }
                }
            }
            when (snapshot.state) {
                JobState.RUNNING, JobState.PAUSING -> mutable.update { if (it.stage is ReceiveStage.Unlocking) it.copy(stage = ReceiveStage.Unlocking(snapshot.phase)) else it }
                JobState.COMPLETE -> return JobState.COMPLETE
                else -> {
                    mutable.update { it.copy(stage = ReceiveStage.Unavailable(snapshot.error)) }
                    return snapshot.state
                }
            }
            delay(120)
        }
    }

    /** Downloads [selection], or every file when it is null. */
    fun download(selection: Set<String>?) {
        val stage = mutable.value.stage as? ReceiveStage.Files ?: return
        val preparation = prepared ?: return
        val id = checkpoint ?: return
        answered = null
        mutable.update { it.copy(stage = ReceiveStage.Downloading(stage.items, selection)) }
        viewModelScope.launch {
            runCatching { coordinator.startPrepared(preparation, id, selection?.toList()) }
                .onFailure { failure -> mutable.update { it.copy(stage = ReceiveStage.Unavailable(failure.message)) } }
        }
    }

    fun cancelDownload() {
        coordinator.pause()
        val stage = mutable.value.stage as? ReceiveStage.Downloading ?: return
        mutable.update { it.copy(stage = ReceiveStage.Files(stage.items)) }
    }

    private fun answerCoordinatorPrompt(id: ULong, kind: PromptType, peer: String?) {
        when (kind) {
            PromptType.PASSWORD -> password?.let {
                answered = id
                coordinator.respond(id, it)
            }
            PromptType.PEER_CONSENT -> if (consent) {
                answered = id
                coordinator.respondConsent(id, true)
            } else {
                mutable.update { it.copy(stage = ReceiveStage.NeedConsent(peer)) }
            }
            PromptType.SHARE_KEY -> Unit
            else -> {
                answered = id
                coordinator.respond(id, "continue")
            }
        }
    }

    /** Writes one verified file to a location the user picked. */
    fun save(file: File, destination: Uri) = coordinator.export(file.absolutePath, destination)

    fun isDownload(transfer: TransferUiState) = transfer.current?.kind == TransferKind.DOWNLOAD || transfer.current?.kind == TransferKind.CONTROL

    // endregion

    // region Notes

    private fun inspectNote() {
        cancelWork()
        mutable.update { it.copy(stage = ReceiveStage.Unlocking("unlocking")) }
        work = viewModelScope.launch {
            val inspection = mutable.value.inspection ?: return@launch
            runCatching { withContext(Dispatchers.IO) { app.accountSessions.service(inspection.instance).inspectNote(link) } }
                .onSuccess { note: NoteInspection -> mutable.update { it.copy(stage = ReceiveStage.NoteLocked(note.burnOnRead)) } }
                .onFailure { failure -> mutable.update { it.copy(stage = ReceiveStage.Unavailable(failure.message)) } }
        }
    }

    /** Decrypts the note. For burn-on-read notes this is the read that removes it. */
    fun decryptNote() {
        val inspection = mutable.value.inspection ?: return
        val locked = mutable.value.stage as? ReceiveStage.NoteLocked
        cancelWork()
        mutable.update { it.copy(stage = ReceiveStage.Unlocking("unlocking")) }
        work = viewModelScope.launch {
            val job = runCatching {
                withContext(Dispatchers.IO) {
                    app.accountSessions.service(inspection.instance).startReceiveNote(NoteReceiveRequest(link, password, locked?.burnOnRead ?: true))
                }
            }.getOrElse { failure ->
                mutable.update { it.copy(stage = ReceiveStage.Unavailable(failure.message)) }
                return@launch
            }
            noteJob = job
            var lastPrompt: ULong? = null
            while (true) {
                val snapshot = withContext(Dispatchers.IO) { job.snapshot() }
                val prompt = snapshot.prompt
                if (prompt != null && prompt.id != lastPrompt) {
                    when (prompt.kind) {
                        PromptType.PASSWORD -> {
                            val known = password
                            if (known == null || snapshot.secretRetry != null) {
                                password = null
                                withContext(Dispatchers.IO) { job.cancel() }
                                mutable.update { it.copy(stage = ReceiveStage.NeedPassword(error = snapshot.secretRetry != null)) }
                                return@launch
                            }
                            lastPrompt = prompt.id
                            withContext(Dispatchers.IO) { job.respond(prompt.id, known) }
                        }
                        PromptType.PEER_CONSENT -> {
                            if (!consent && !recentlyConsented()) {
                                withContext(Dispatchers.IO) { job.cancel() }
                                mutable.update { it.copy(stage = ReceiveStage.NeedConsent(prompt.peer)) }
                                return@launch
                            }
                            lastPrompt = prompt.id
                            withContext(Dispatchers.IO) { job.respondConsent(prompt.id, true) }
                        }
                        PromptType.SHARE_KEY -> {
                            withContext(Dispatchers.IO) { job.cancel() }
                            mutable.update { it.copy(stage = ReceiveStage.NeedKey(error = true)) }
                            return@launch
                        }
                        else -> {
                            lastPrompt = prompt.id
                            withContext(Dispatchers.IO) { runCatching { job.respond(prompt.id, "continue") } }
                        }
                    }
                }
                when (snapshot.state) {
                    JobState.RUNNING, JobState.PAUSING -> mutable.update { if (it.stage is ReceiveStage.Unlocking) it.copy(stage = ReceiveStage.Unlocking(snapshot.phase)) else it }
                    JobState.COMPLETE -> {
                        val opened = withContext(Dispatchers.IO) { job.takeNote() }
                        val pending = withContext(Dispatchers.IO) { job.canRetryBurn() }
                        mutable.update { it.copy(stage = if (opened != null) ReceiveStage.Note(opened, pending) else ReceiveStage.Unavailable(null)) }
                        return@launch
                    }
                    else -> {
                        mutable.update { it.copy(stage = ReceiveStage.Unavailable(snapshot.error)) }
                        return@launch
                    }
                }
                delay(100)
            }
        }
    }

    /** Asks the server again to remove a burn-on-read note whose removal did not go through. */
    fun retryBurn() {
        val job = noteJob ?: return
        val stage = mutable.value.stage as? ReceiveStage.Note ?: return
        viewModelScope.launch {
            val removed = runCatching { withContext(Dispatchers.IO) { job.retryBurn() } }.getOrDefault(false)
            mutable.update { it.copy(stage = stage.copy(burnPending = !removed)) }
        }
    }

    // endregion

    fun report(category: String, description: String, email: String?) {
        val inspection = mutable.value.inspection ?: return
        mutable.update { it.copy(reportBusy = true, reportError = null) }
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    app.accountSessions.service(AccountSessionRegistry.normalizeOrigin(inspection.instance)).accountReport(inspection.id, category, description, email?.takeIf(String::isNotBlank))
                }
            }.onSuccess { mutable.update { it.copy(reportBusy = false, reportSent = true) } }
                .onFailure { failure -> mutable.update { it.copy(reportBusy = false, reportError = failure.message ?: "") } }
        }
    }

    fun clearReport() = mutable.update { it.copy(reportSent = false, reportError = null) }

    /** Drops everything about the current link, including entered secrets and the opened note. */
    fun reset(keepInput: Boolean = false) {
        cancelWork()
        noteJob?.let { job -> viewModelScope.launch(Dispatchers.IO) { runCatching { job.destroy() } } }
        noteJob = null
        prepared = null
        checkpoint = null
        password = null
        consent = false
        answered = null
        link = ""
        mutable.update { ReceiveUiState(input = if (keepInput) it.input else "") }
    }

    private fun cancelWork() {
        work?.cancel()
        work = null
    }

    private fun hasKey(value: String) = value.substringAfter('#', "").contains("k=")

    /** Consent to peer address exposure is remembered for a year, as on the web. */
    private suspend fun recentlyConsented(): Boolean {
        val at = app.appearance.peerExposureAcceptedAt.first() ?: return false
        return System.currentTimeMillis() - at < 365L * 24 * 3_600_000
    }

    override fun onCleared() {
        noteJob?.let { runCatching { it.destroy() } }
    }
}
