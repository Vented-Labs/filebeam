package io.filebeam.android.ui

import android.app.Application
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.filebeam.android.FilebeamApplication
import io.filebeam.android.R
import io.filebeam.android.platform.AppSettings
import io.filebeam.android.platform.UploadRequest
import io.filebeam.android.platform.security.EncryptedDraftStore
import io.filebeam.rust.Transport
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/** Legacy enum remains until the scaffold consumes DestinationRoute. */
enum class Destination(val label: Int) {
    Send(R.string.send), Receive(R.string.receive), Transfers(R.string.transfers),
    Notes(R.string.notes), Inbox(R.string.inbox), Turbo(R.string.turbo),
    Account(R.string.account), Settings(R.string.settings);
    companion object { val primary = listOf(Send, Receive, Transfers, Inbox) }
}

class FilebeamViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as FilebeamApplication
    private val drafts = EncryptedDraftStore(application)
    val coordinator = app.transfers
    val accounts = app.accounts
    val notes = app.notes
    val turbo = app.turbo
    val inbox = app.inbox
    val transfers = coordinator.state
    val settings = app.settings.values.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AppSettings())

    var navigation by mutableStateOf(NavigationState())
        private set
    var destination by mutableStateOf(Destination.Send)
    var sendDraft by mutableStateOf(SendDraft())
        private set
    var noteDraft by mutableStateOf(NoteDraft())
        private set
    var link by mutableStateOf("")
    var receiveIngress by mutableStateOf<ReceiveIngress?>(null)
        private set
    var exportSource: String? = null

    // Transitional screen properties. They are backed by the durable draft rather than SavedStateHandle/Bundle.
    val selectedFiles: List<Uri> get() = sendDraft.sources.map(SelectedSource::uri)
    var transport: Transport
        get() = sendDraft.transport
        set(value) { updateSend { it.copy(transport = value, recipient = it.recipient.copy(status = recipientStatus(it.recipient.username, value, it.turbo, it.passwordProtected))) } }
    var archive: Boolean
        get() = sendDraft.archive
        set(value) { updateSend { it.copy(archive = value) } }
    var turboTransfer: Boolean
        get() = sendDraft.turbo
        set(value) { updateSend { it.copy(turbo = value, recipient = it.recipient.copy(status = recipientStatus(it.recipient.username, it.transport, value, it.passwordProtected))) } }
    var passwordProtected: Boolean
        get() = sendDraft.passwordProtected
        set(value) { updateSend { it.copy(passwordProtected = value, recipient = it.recipient.copy(status = recipientStatus(it.recipient.username, it.transport, it.turbo, value))) } }
    var retentionHours: String
        get() = sendDraft.retentionHours
        set(value) { updateSend { it.copy(retentionHours = value) } }
    var recipientUsername: String
        get() = sendDraft.recipient.username
        set(value) { updateSend { it.copy(recipient = RecipientDraft(value.trim(), recipientStatus(value.trim(), it.transport, it.turbo, it.passwordProtected))) } }

    init {
        viewModelScope.launch {
            val saved = withContext(Dispatchers.IO) { runCatching { drafts.load() }.getOrNull() } ?: return@launch
            restore(saved)
        }
    }

    fun appendFiles(uris: List<Uri>, paths: Map<String, String> = emptyMap()) {
        val candidates = uris.filter { it.scheme == "content" }.distinctBy(Uri::toString)
        viewModelScope.launch {
            val additions = withContext(Dispatchers.IO) { candidates.map { uri -> sourceMetadata(uri, uri.toString(), paths[uri.toString()]) } }
            // Recheck after I/O: a second picker result may have arrived while metadata was read.
            updateSend { draft -> draft.copy(sources = draft.sources + additions.filter { candidate -> draft.sources.none { it.identity == candidate.identity } }) }
        }
        destination = Destination.Send
        navigation = navigation.push(DestinationRoute.Send)
    }

    fun selectFiles(uris: List<Uri>) = appendFiles(uris)
    fun removeFile(uri: Uri) = updateSend { draft -> draft.copy(sources = draft.sources.filterNot { it.identity == uri.toString() }) }
    fun markSourceError(uri: Uri, error: String) = updateSend { draft -> draft.copy(sources = draft.sources.map { if (it.identity == uri.toString()) it.copy(error = error) else it }) }
    fun receiveLink(value: String) { link = value; receiveIngress = parseReceiveIngress(value); destination = Destination.Receive; navigation = navigation.push(DestinationRoute.Receive) }
    fun navigate(value: Destination) { destination = value; navigation = navigation.push(value.route()) }
    fun back(): Boolean {
        val next = navigation.back()
        if (next == navigation) return false
        navigation = next
        destination = next.current.destination()
        return true
    }

    fun validateRecipient() {
        val name = sendDraft.recipient.username
        val conflict = sendDraft.recipientConflict()
        if (name.isBlank()) return
        if (conflict != null) { updateSend { it.copy(recipient = it.recipient.copy(status = RecipientStatus.CONFLICT, error = conflict)) }; return }
        updateSend { it.copy(recipient = it.recipient.copy(status = RecipientStatus.VALIDATING, error = null)) }
        coordinator.validateRecipient(name) { error ->
            updateSend { it.copy(recipient = it.recipient.copy(status = if (error == null) RecipientStatus.VALIDATED else RecipientStatus.INVALID, error = error)) }
        }
    }

    fun updateNote(transform: (NoteDraft) -> NoteDraft) { noteDraft = transform(noteDraft); persistDrafts() }

    fun upload() {
        retentionHours(retentionHours).onFailure { coordinator.message(it.message); return }
        val recipient = sendDraft.recipient
        sendDraft.recipientConflict()?.let { coordinator.message(it); return }
        if (recipient.username.isNotBlank() && recipient.status != RecipientStatus.VALIDATED) {
            coordinator.message("Validate the recipient before sending")
            return
        }
        coordinator.upload(UploadRequest(sendDraft.sources.map(SelectedSource::uri), transport, archive, turboTransfer, passwordProtected,
            retentionHours(retentionHours).getOrNull(), recipients = recipient.username.takeIf(String::isNotBlank)?.let(::listOf) ?: emptyList()))
        destination = Destination.Transfers
        navigation = navigation.push(DestinationRoute.Transfers)
    }

    fun download() {
        val ingress = parseReceiveIngress(link)
        if (ingress == null || ingress.kind == ReceiveKind.UNKNOWN) { coordinator.message("Enter a supported transfer link or ID"); return }
        receiveIngress = ingress
        coordinator.download(ingress.raw)
        destination = Destination.Transfers
        navigation = navigation.push(DestinationRoute.Transfers)
    }
    fun saveSettings(value: AppSettings) { viewModelScope.launch { app.settings.save(value) } }

    private fun updateSend(transform: (SendDraft) -> SendDraft) { sendDraft = transform(sendDraft); persistDrafts() }
    private fun persistDrafts() {
        val send = sendDraft
        val note = noteDraft
        viewModelScope.launch(Dispatchers.IO) { drafts.save(send.toJson().put("note", note.toJson())) }
    }
    private fun restore(value: JSONObject) {
        sendDraft = SendDraft(
            sources = value.optJSONArray("sources")?.let { array -> (0 until array.length()).map { index -> array.getJSONObject(index) }.map { source ->
                SelectedSource(Uri.parse(source.getString("uri")), source.getString("identity"), source.getString("name"), source.optLong("size").takeIf { source.has("size") }, source.optString("path").takeIf(String::isNotBlank), source.optString("error").takeIf(String::isNotBlank))
            } } ?: emptyList(),
            transport = if (value.optString("transport") == Transport.WEB_RTC.name) Transport.WEB_RTC else Transport.HTTP,
            archive = value.optBoolean("archive"), turbo = value.optBoolean("turbo"), passwordProtected = value.optBoolean("password"),
            retentionHours = value.optString("retention"), recipient = RecipientDraft(value.optString("recipient"), RecipientStatus.UNVALIDATED),
        )
        value.optJSONObject("note")?.let { noteDraft = NoteDraft(it.optString("title"), it.optString("body"), it.optString("password"), it.optString("retention"), it.optString("language", "plain"), it.optBoolean("burn"), it.optBoolean("live")) }
    }
    private fun sourceMetadata(uri: Uri, identity: String, path: String?): SelectedSource {
        var name = uri.lastPathSegment ?: "file"
        var size: Long? = null
        var error: String? = null
        runCatching { getApplication<Application>().contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor -> if (cursor.moveToFirst()) { cursor.getString(0)?.let { name = it }; if (!cursor.isNull(1)) size = cursor.getLong(1) } } }.onFailure { error = it.message ?: "Source metadata is unavailable" }
        return SelectedSource(uri, identity, name, size, path, error)
    }
    private fun SendDraft.toJson() = JSONObject().put("sources", JSONArray(sources.map { source -> JSONObject().put("uri", source.uri).put("identity", source.identity).put("name", source.displayName).putOpt("size", source.sizeBytes).putOpt("path", source.relativePath).putOpt("error", source.error) }))
        .put("transport", transport.name).put("archive", archive).put("turbo", turbo).put("password", passwordProtected).put("retention", retentionHours).put("recipient", recipient.username)
    private fun NoteDraft.toJson() = JSONObject().put("title", title).put("body", body).put("password", password).put("retention", retentionHours).put("language", language).put("burn", burnAfterRead).put("live", live)
}

private fun recipientStatus(name: String, transport: Transport, turbo: Boolean, password: Boolean) = when {
    name.isBlank() -> RecipientStatus.NONE
    transport != Transport.HTTP || turbo || password -> RecipientStatus.CONFLICT
    else -> RecipientStatus.UNVALIDATED
}
private fun Destination.route() = when (this) { Destination.Send, Destination.Notes, Destination.Turbo -> DestinationRoute.Send; Destination.Receive -> DestinationRoute.Receive; Destination.Transfers -> DestinationRoute.Transfers; Destination.Inbox -> DestinationRoute.Inbox; Destination.Settings -> DestinationRoute.Settings; Destination.Account -> DestinationRoute.Account }
private fun DestinationRoute.destination() = when (this) { DestinationRoute.Send -> Destination.Send; DestinationRoute.Receive -> Destination.Receive; DestinationRoute.Transfers, DestinationRoute.TransferDetail -> Destination.Transfers; DestinationRoute.Inbox, DestinationRoute.NoteViewer -> Destination.Inbox; DestinationRoute.Settings -> Destination.Settings; DestinationRoute.Account -> Destination.Account }
