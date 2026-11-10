package io.filebeam.android.ui.feature.send

import android.app.Application
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.Immutable
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.filebeam.android.FilebeamApplication
import io.filebeam.android.platform.TransferKind
import io.filebeam.android.platform.TransferUiState
import io.filebeam.android.platform.UploadRequest
import io.filebeam.android.platform.ValidatedRecipient
import io.filebeam.android.platform.security.DraftLoadResult
import io.filebeam.android.platform.security.EncryptedDraftStore
import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.android.platform.services.NoteRequest
import io.filebeam.android.platform.services.NoteSummary
import io.filebeam.android.ui.shell.InstanceProfile
import io.filebeam.rust.AttachedNote
import io.filebeam.rust.DirectoryChoice
import io.filebeam.rust.PromptType
import io.filebeam.rust.Transport
import io.filebeam.rust.estimateUploadCiphertextBytes
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.security.SecureRandom

@Immutable
data class SendUiState(
    val mode: SendMode = SendMode.Files,
    val files: FileDraft = FileDraft(),
    val note: NoteDraft = NoteDraft(),
    val stage: SendStage = SendStage.Composing,
    /** A one-off line about the last add, e.g. skipped duplicates. */
    val queueMessage: String? = null,
    val recipientLookup: RecipientLookup = RecipientLookup.Idle,
    val createdNote: NoteSummary? = null,
    val noteBusy: Boolean = false,
    val error: String? = null,
)

sealed interface RecipientLookup {
    data object Idle : RecipientLookup
    data object Searching : RecipientLookup
    data class Failed(val message: String) : RecipientLookup
}

/**
 * Owns both composers (files and notes), their protected drafts, and the hand-off to the
 * transfer coordinator. Passwords live here in memory only and answer the native prompt.
 */
class SendViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as FilebeamApplication
    private val coordinator = app.transfers
    private val drafts = EncryptedDraftStore(application)
    private val draftWrites = Channel<JSONObject>(Channel.CONFLATED)
    private val mutable = MutableStateFlow(SendUiState())
    val state: StateFlow<SendUiState> = mutable.asStateFlow()
    val transfer: StateFlow<TransferUiState> = coordinator.state

    private var answeredPrompt: ULong? = null
    private var lookupRevision = 0
    private var edited = false

    init {
        viewModelScope.launch(Dispatchers.IO) {
            val restored = runCatching { drafts.loadResult() }.getOrNull()
            if (restored is DraftLoadResult.Restored && !edited) runCatching { restore(restored.value) }
            for (write in draftWrites) runCatching { drafts.save(write) }
        }
        // Answers prompts this screen already has the answer to, so the user is not asked twice.
        viewModelScope.launch {
            coordinator.state.collect { transfer ->
                val prompt = transfer.snapshot?.prompt ?: return@collect
                if (prompt.id == answeredPrompt || mutable.value.stage !is SendStage.Sharing) return@collect
                val draft = mutable.value
                when (prompt.kind) {
                    PromptType.PASSWORD -> {
                        val password = (if (draft.mode == SendMode.Files) draft.files.password else draft.note.password) ?: return@collect
                        answeredPrompt = prompt.id
                        coordinator.respond(prompt.id, password)
                    }
                    PromptType.DIRECTORY -> {
                        answeredPrompt = prompt.id
                        coordinator.respondDirectory(prompt.id, if (draft.files.archive) DirectoryChoice.ZIP else DirectoryChoice.INDIVIDUAL_FILES)
                    }
                    PromptType.SHARE_READY -> {
                        answeredPrompt = prompt.id
                        coordinator.respond(prompt.id, "continue")
                    }
                    else -> Unit
                }
            }
        }
    }

    fun setMode(mode: SendMode) = edit { it.copy(mode = mode) }
    fun clearError() = mutable.update { it.copy(error = null) }
    fun clearQueueMessage() = mutable.update { it.copy(queueMessage = null) }

    // region Files

    /**
     * Adds picked or shared documents. Exact duplicates are skipped and the selection is capped
     * at the instance's file-count limit, each with a short explanation.
     */
    fun addFiles(uris: List<Uri>, relativePaths: Map<String, String> = emptyMap(), maximumFiles: Long?, duplicateMessage: (Int) -> String, limitMessage: (Int, Long) -> String) {
        val candidates = uris.filter { it.scheme == "content" }.distinctBy(Uri::toString)
        if (candidates.isEmpty()) return
        viewModelScope.launch {
            val described = withContext(Dispatchers.IO) { candidates.map { describe(it, relativePaths[it.toString()]) } }
            var message: String? = null
            edit { state ->
                val existing = state.files.sources
                val fresh = described.filter { candidate ->
                    existing.none { it.identity == candidate.identity || (it.name == candidate.name && it.sizeBytes == candidate.sizeBytes && it.mimeType == candidate.mimeType && it.relativePath == candidate.relativePath) }
                }
                val duplicates = described.size - fresh.size
                val room = maximumFiles?.let { (it - existing.size).coerceAtLeast(0).toInt() } ?: fresh.size
                val accepted = fresh.take(room)
                message = when {
                    accepted.size < fresh.size && maximumFiles != null -> limitMessage(room, maximumFiles)
                    duplicates > 0 -> duplicateMessage(duplicates)
                    else -> null
                }
                state.copy(mode = SendMode.Files, files = state.files.copy(sources = existing + accepted), queueMessage = message)
            }
        }
    }

    fun addFolder(tree: Uri, maximumFiles: Long?, duplicateMessage: (Int) -> String, limitMessage: (Int, Long) -> String, failed: (String?) -> Unit) {
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { coordinator.storage.flattenDocumentTree(tree) } }
                .onSuccess { sources -> addFiles(sources.map { it.uri }, sources.associate { it.uri.toString() to it.relativePath }, maximumFiles, duplicateMessage, limitMessage) }
                .onFailure { failed(it.message) }
        }
    }

    fun removeSource(identity: String) = edit { it.copy(files = it.files.copy(sources = it.files.sources.filterNot { source -> source.identity == identity }), queueMessage = null) }
    fun setFileDriver(driver: String) = edit {
        // Inbox delivery is stored-only, so choosing live sending drops the recipient.
        it.copy(files = it.files.copy(driver = driver, recipient = if (driver == "webrtc") null else it.files.recipient))
    }
    fun setArchive(archive: Boolean) = edit { it.copy(files = it.files.copy(archive = archive)) }
    fun setFilePassword(password: String?) = edit { it.copy(files = it.files.copy(password = password?.takeIf(String::isNotEmpty))) }
    fun setFileRetention(hours: Long) = edit { it.copy(files = it.files.copy(retentionHours = hours)) }
    fun setFileIncludeKey(include: Boolean) = edit { it.copy(files = it.files.copy(includeKey = include)) }
    fun setAttachedNote(note: NoteDraft?) = edit { it.copy(files = it.files.copy(attachedNote = note)) }
    fun clearRecipient() {
        lookupRevision++
        edit { it.copy(files = it.files.copy(recipient = null), recipientLookup = RecipientLookup.Idle) }
    }

    /** Resolves a username to its inbox key. Sending to a friend forces stored HTTP without a password. */
    fun chooseRecipient(username: String) {
        val name = username.trim().removePrefix("@").lowercase()
        if (name.isEmpty()) return
        val revision = ++lookupRevision
        mutable.update { it.copy(recipientLookup = RecipientLookup.Searching) }
        coordinator.validateRecipient(name) { identity, error ->
            if (revision != lookupRevision) return@validateRecipient
            if (identity == null) {
                mutable.update { it.copy(recipientLookup = RecipientLookup.Failed(error ?: "")) }
            } else {
                edit { it.copy(files = it.files.copy(recipient = identity, driver = "http", password = null, includeKey = true), recipientLookup = RecipientLookup.Idle) }
            }
        }
    }

    fun setRecipient(identity: ValidatedRecipient) = edit {
        it.copy(files = it.files.copy(recipient = identity, driver = "http", password = null, includeKey = true), recipientLookup = RecipientLookup.Idle)
    }

    // endregion

    // region Notes

    fun updateNote(transform: (NoteDraft) -> NoteDraft) = edit { it.copy(note = transform(it.note)) }

    // endregion

    /** The first thing stopping a file send, or null when it can go. */
    fun fileBlocker(profile: InstanceProfile, signedIn: Boolean, state: SendUiState = mutable.value): SendBlocker? {
        val draft = state.files
        if (!profile.loaded) return if (profile.loading || profile.error == null) SendBlocker.LoadingPolicy else SendBlocker.PolicyUnavailable(profile.error)
        if (!profile.anonymousUploads && !signedIn) return SendBlocker.SignInRequired
        val driver = draft.driver ?: profile.defaultDriver
        if (driver !in profile.enabledDrivers) return SendBlocker.TransportDisabled
        if (draft.sources.isEmpty()) return SendBlocker.NothingSelected
        if (draft.sources.any { it.error != null }) return SendBlocker.SourceError
        val limits = profile.limits(driver)
        val count = if (draft.archive) 1L else draft.sources.size.toLong()
        limits.maximumFileCount?.let { if (count > it) return SendBlocker.TooManyFiles(it) }
        limits.maximumTransferBytes?.let { maximum ->
            if (!draft.archive && draft.sizeKnown && profile.chunkBytes > 0) {
                val sizes = draft.sources.map { it.sizeBytes!!.toULong() }
                val estimated = runCatching { estimateUploadCiphertextBytes(sizes, profile.chunkBytes.toULong()).toLong() }.getOrNull()
                if (estimated != null && estimated > maximum) {
                    val other = profile.webrtc.maximumTransferBytes
                    val fits = driver == "http" && profile.webrtcEnabled && draft.recipient == null && (other == null || estimated <= other)
                    return SendBlocker.TooLarge(maximum, fits)
                }
            }
        }
        draft.password?.let { if (it.length !in PASSWORD_MIN..PASSWORD_MAX) return SendBlocker.PasswordTooShort }
        draft.attachedNote?.let { note ->
            if (note.body.isEmpty() || note.bytes > ATTACHED_NOTE_MAX_BYTES || note.title.codePointCount(0, note.title.length) > NOTE_TITLE_MAX) return SendBlocker.AttachedNoteInvalid
        }
        return null
    }

    fun noteBlocker(profile: InstanceProfile, signedIn: Boolean, state: SendUiState = mutable.value): SendBlocker? {
        val draft = state.note
        if (!profile.loaded) return if (profile.loading || profile.error == null) SendBlocker.LoadingPolicy else SendBlocker.PolicyUnavailable(profile.error)
        if (!profile.anonymousUploads && !signedIn) return SendBlocker.SignInRequired
        val driver = draft.driver ?: profile.defaultDriver
        if (driver !in profile.enabledDrivers) return SendBlocker.TransportDisabled
        if (draft.body.isEmpty()) return SendBlocker.NothingSelected
        profile.limits(driver).maximumNoteBytes?.let { if (draft.bytes > it) return SendBlocker.NoteTooLarge(it) }
        draft.password?.let { if (it.length !in PASSWORD_MIN..PASSWORD_MAX) return SendBlocker.PasswordTooShort }
        return null
    }

    /** Starts the upload and switches the tab to the share view. */
    fun sendFiles(profile: InstanceProfile, turbo: Boolean) {
        val draft = mutable.value.files
        val driver = draft.driver ?: profile.defaultDriver
        val live = driver == "webrtc"
        val recipient = draft.recipient
        answeredPrompt = null
        coordinator.upload(
            UploadRequest(
                uris = draft.sources.map(SelectedSource::uri),
                transport = if (live) Transport.WEB_RTC else Transport.HTTP,
                archive = draft.archive,
                turbo = turbo && !live && recipient == null,
                passwordProtected = draft.password != null && recipient == null,
                retentionHours = (draft.retentionHours ?: profile.fileRetention.defaultHours).toULong(),
                recipients = recipient?.let { listOf(it.username) } ?: emptyList(),
                recipient = recipient,
                includeKeyInLink = draft.includeKey || recipient != null,
                driver = driver,
                names = draft.sources.associate { it.identity to it.name },
                attachedNote = draft.attachedNote?.takeIf { it.body.isNotEmpty() }?.let { AttachedNote(it.body, it.title.takeIf(String::isNotEmpty), it.language.key) },
            ),
        )
        val kind = when {
            recipient != null -> ShareKind.Inbox
            live -> ShareKind.LiveFiles
            else -> ShareKind.Files
        }
        mutable.update { it.copy(stage = SendStage.Sharing(kind, turbo && !live && recipient == null, System.currentTimeMillis()), error = null) }
    }

    fun sendNote(profile: InstanceProfile, instance: String) {
        val draft = mutable.value.note
        val driver = draft.driver ?: profile.defaultDriver
        val live = driver == "webrtc"
        answeredPrompt = null
        mutable.update { it.copy(noteBusy = true, error = null, createdNote = null, stage = SendStage.Sharing(if (live) ShareKind.LiveNote else ShareKind.Note, false, System.currentTimeMillis())) }
        viewModelScope.launch {
            runCatching {
                app.notes.create(
                    NoteRequest(
                        instance = AccountSessionRegistry.normalizeOrigin(instance),
                        text = draft.body,
                        title = draft.title,
                        language = draft.language.key,
                        password = draft.password,
                        retentionHours = (draft.retentionHours ?: profile.noteRetention.defaultHours).toULong(),
                        live = live,
                        burnAfterRead = draft.burnOnRead,
                        includeKeyInLink = draft.includeKey,
                    ),
                )
            }.onSuccess { created ->
                mutable.update { it.copy(noteBusy = false, createdNote = created) }
            }.onFailure { failure ->
                mutable.update { it.copy(noteBusy = false, stage = SendStage.Composing, error = failure.message) }
            }
        }
    }

    /** Stops the running upload. A cancelled upload returns to the composer with the draft intact. */
    fun cancelUpload() {
        coordinator.pause()
        mutable.update { it.copy(stage = SendStage.Composing) }
    }

    /** Leaves the share view. The sent draft is cleared; options such as retention are kept. */
    fun newTransfer() {
        edit { state ->
            when (state.stage.let { it as? SendStage.Sharing }?.kind) {
                ShareKind.Note, ShareKind.LiveNote -> state.copy(stage = SendStage.Composing, note = state.note.copy(title = "", body = "", password = null), createdNote = null)
                null -> state.copy(stage = SendStage.Composing)
                else -> state.copy(stage = SendStage.Composing, files = state.files.copy(sources = emptyList(), password = null, attachedNote = null, recipient = null), queueMessage = null)
            }
        }
    }

    /** Returns to the composer after a failed send, keeping everything that was selected. */
    fun backToComposer() = mutable.update { it.copy(stage = SendStage.Composing) }

    fun endLiveNote() {
        mutable.value.createdNote?.link?.let { runCatching { app.notes.endLive(it) } }
    }

    fun isUpload(transfer: TransferUiState) = transfer.current?.kind == TransferKind.UPLOAD || transfer.current?.kind == TransferKind.NOTE_LIVE

    private fun edit(transform: (SendUiState) -> SendUiState) {
        edited = true
        mutable.update(transform)
        draftWrites.trySend(encode(mutable.value))
    }

    private fun describe(uri: Uri, relativePath: String?): SelectedSource {
        var name = uri.lastPathSegment ?: "file"
        var size: Long? = null
        var error: String? = null
        val resolver = getApplication<Application>().contentResolver
        runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    cursor.getString(0)?.let { name = it }
                    if (!cursor.isNull(1)) size = cursor.getLong(1)
                }
            }
        }.onFailure { error = it.message ?: "unavailable" }
        return SelectedSource(uri, name, size, runCatching { resolver.getType(uri) }.getOrNull(), relativePath, error)
    }

    // Passwords are deliberately absent from the stored draft.
    private fun encode(state: SendUiState): JSONObject {
        fun note(note: NoteDraft) = JSONObject().put("title", note.title).put("body", note.body).put("language", note.language.key).put("wrap", note.wrap)
            .putOpt("retention", note.retentionHours).put("includeKey", note.includeKey).put("burn", note.burnOnRead).putOpt("driver", note.driver)
        val files = state.files
        return JSONObject().put("version", 2).put("mode", state.mode.name)
            .put("sources", JSONArray(files.sources.map { JSONObject().put("uri", it.uri.toString()).put("name", it.name).putOpt("size", it.sizeBytes).putOpt("mime", it.mimeType).putOpt("path", it.relativePath) }))
            .putOpt("driver", files.driver).put("archive", files.archive).putOpt("retention", files.retentionHours).put("includeKey", files.includeKey)
            .putOpt("attachedNote", files.attachedNote?.let(::note)).put("note", note(state.note))
    }

    private fun restore(value: JSONObject) {
        if (value.optInt("version") != 2) return
        fun note(json: JSONObject) = NoteDraft(
            title = json.optString("title"), body = json.optString("body"), language = NoteLanguage.fromKey(json.optString("language")), wrap = json.optBoolean("wrap", true),
            retentionHours = json.optLong("retention").takeIf { json.has("retention") }, includeKey = json.optBoolean("includeKey", true), burnOnRead = json.optBoolean("burn"),
            driver = json.optString("driver").takeIf(String::isNotBlank),
        )
        val sources = value.optJSONArray("sources")?.let { array ->
            (0 until array.length()).map { array.getJSONObject(it) }.map { SelectedSource(Uri.parse(it.getString("uri")), it.getString("name"), it.optLong("size").takeIf { _ -> it.has("size") }, it.optString("mime").takeIf(String::isNotBlank), it.optString("path").takeIf(String::isNotBlank)) }
        } ?: emptyList()
        mutable.update {
            it.copy(
                mode = if (value.optString("mode") == SendMode.Notes.name) SendMode.Notes else SendMode.Files,
                files = FileDraft(
                    sources = sources, driver = value.optString("driver").takeIf(String::isNotBlank), archive = value.optBoolean("archive"),
                    retentionHours = value.optLong("retention").takeIf { value.has("retention") }, includeKey = value.optBoolean("includeKey", true),
                    attachedNote = value.optJSONObject("attachedNote")?.let(::note),
                ),
                note = value.optJSONObject("note")?.let(::note) ?: NoteDraft(),
            )
        }
    }

    override fun onCleared() {
        draftWrites.close()
    }

    companion object {
        private const val PASSWORD_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"

        /** 24 characters from a 64-symbol alphabet: 144 bits, the same recipe as the web. */
        fun generatePassword(): String {
            val random = SecureRandom()
            return buildString(24) { repeat(24) { append(PASSWORD_ALPHABET[random.nextInt(PASSWORD_ALPHABET.length)]) } }
        }
    }
}
