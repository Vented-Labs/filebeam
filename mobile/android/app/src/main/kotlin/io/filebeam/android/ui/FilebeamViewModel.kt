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
import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.platform.security.DraftLoadResult
import io.filebeam.android.platform.security.EncryptedDraftStore
import io.filebeam.rust.Transport
import io.filebeam.rust.LinkInspection
import io.filebeam.rust.presentShareLink
import io.filebeam.rust.splitShareLink
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.channels.Channel
import org.json.JSONArray
import org.json.JSONObject

/** Legacy enum remains until the scaffold consumes DestinationRoute. */
enum class Destination(val label: Int) {
    Send(R.string.send), Receive(R.string.receive), Transfers(R.string.transfers),
    Notes(R.string.notes), Inbox(R.string.inbox), Turbo(R.string.turbo),
    Account(R.string.account), Settings(R.string.settings), StorageUsage(R.string.storage), ReviewTransfers(R.string.review_transfers);
    companion object { val primary = listOf(Send, Receive, Transfers, Inbox) }
}

class FilebeamViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as FilebeamApplication
    private val drafts = EncryptedDraftStore(application)
    private val draftWrites = Channel<Pair<SendDraft, NoteDraft>>(Channel.CONFLATED)
    private var draftRevision = 0L
    private var recipientRevision = 0L
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
    var sendContent by mutableStateOf(SendContent.FILES)
        private set
    /** Discovery is keyed by normalized origin and signed-in account, never reused across instances. */
    var sendDiscovery by mutableStateOf<Map<SendDiscoveryKey, SendDiscoveryState>>(emptyMap())
        private set
    var instanceTransaction by mutableStateOf(InstanceSettingsTransaction())
        private set
    /** An unavailable keystore draft is surfaced to the UI; it is never represented as a new empty draft. */
    var draftRestoreError by mutableStateOf<String?>(null)
        private set
    var draftRestoreComplete by mutableStateOf(false)
        private set
    var link by mutableStateOf("")
    var receiveIngress by mutableStateOf<ReceiveIngress?>(null)
        private set
    var exportSource: String? = null

    // Transitional screen properties. They are backed by the durable draft rather than SavedStateHandle/Bundle.
    val selectedFiles: List<Uri> get() = sendDraft.sources.map(SelectedSource::uri)
    var transport: Transport
        get() = sendDraft.transport
        set(value) { updateSend { it.copy(transport = value, recipient = it.recipient.unvalidated(value, it.turbo, it.passwordProtected)) } }
    var archive: Boolean
        get() = sendDraft.archive
        set(value) { updateSend { it.copy(archive = value) } }
    var turboTransfer: Boolean
        get() = sendDraft.turbo
        set(value) { updateSend { it.copy(turbo = value, recipient = it.recipient.unvalidated(it.transport, value, it.passwordProtected)) } }
    var passwordProtected: Boolean
        get() = sendDraft.passwordProtected
        set(value) { updateSend { it.copy(passwordProtected = value, recipient = it.recipient.unvalidated(it.transport, it.turbo, value)) } }
    var retentionHours: String
        get() = sendDraft.retentionHours
        set(value) { updateSend { it.copy(retentionHours = value) } }
    var recipientUsername: String
        get() = sendDraft.recipient.username
        set(value) { recipientRevision++; updateSend { it.copy(recipient = RecipientDraft(value.trim(), recipientStatus(value.trim(), it.transport, it.turbo, it.passwordProtected))) } }

    val activeSendDiscovery: SendDiscoveryState?
        get() = sendDiscovery[sendDiscoveryKey()]

    init {
        // Capture before any coroutine can be delayed behind a first user edit.
        val initialRevision = draftRevision
        viewModelScope.launch(Dispatchers.IO) {
            val restored = runCatching { drafts.loadResult() }
            withContext(Dispatchers.Main.immediate) {
                restored.fold(
                    onSuccess = {
                        when (it) {
                            is DraftLoadResult.Restored -> if (shouldRestoreDraft(initialRevision, draftRevision)) {
                                runCatching { restore(it.value) }.onFailure {
                                    draftRestoreError = "A protected draft is invalid and could not be restored."
                                }
                            }
                            is DraftLoadResult.Unavailable -> draftRestoreError = "A protected draft could not be restored on this device."
                            DraftLoadResult.Missing -> Unit
                        }
                    },
                    onFailure = { draftRestoreError = "A protected draft could not be restored on this device." },
                )
                draftRestoreComplete = true
            }
            // Writes submitted while decrypting were buffered by the conflated channel. Reading and
            // writing are therefore serialized through this one coroutine.
            for ((send, note) in draftWrites) {
                runCatching { drafts.save(send.toJson().put("note", note.toJson())) }.onFailure {
                    withContext(Dispatchers.Main.immediate) {
                        draftRestoreError = "A protected draft could not be saved on this device."
                    }
                }
            }
        }
        viewModelScope.launch { settings.collect {
            if (!instanceTransaction.dirty) instanceTransaction = InstanceSettingsTransaction(committed = it)
            refreshSendDiscovery()
        } }
        viewModelScope.launch { accounts.state.collect { refreshSendDiscovery() } }
    }

    fun appendFiles(uris: List<Uri>, paths: Map<String, String> = emptyMap()) {
        draftRevision++
        val candidates = uris.filter { it.scheme == "content" }.distinctBy(Uri::toString)
        viewModelScope.launch {
            val additions = withContext(Dispatchers.IO) { candidates.map { uri -> sourceMetadata(uri, uri.toString(), paths[uri.toString()]) } }
            // Recheck after I/O: a second picker result may have arrived while metadata was read.
            updateSend { draft -> draft.copy(sources = draft.sources + additions.filter { candidate -> draft.sources.none { it.identity == candidate.identity } }) }
        }
        showSend(SendContent.FILES)
    }

    fun selectFiles(uris: List<Uri>) = appendFiles(uris)
    fun removeFile(uri: Uri) = updateSend { draft -> draft.copy(sources = draft.sources.filterNot { it.identity == uri.toString() }) }
    fun markSourceError(uri: Uri, error: String) = updateSend { draft -> draft.copy(sources = draft.sources.map { if (it.identity == uri.toString()) it.copy(error = error) else it }) }
    fun receiveLink(value: String) { link = value; receiveIngress = parseReceiveIngress(value); navigate(Destination.Receive) }
    /** Receive input is deliberate: clipboard access is initiated only by the Paste button. */
    fun updateReceiveLink(value: String) { link = value; receiveIngress = parseReceiveIngress(value) }
    fun inspectReceiveLink(complete: (Result<io.filebeam.rust.LinkInspection>) -> Unit) = coordinator.inspectLink(link, complete)
    fun noteLinkHasKey(inspection: LinkInspection): Boolean = runCatching { splitShareLink(inspection.instance, link) }.isSuccess
    fun combineReceivedNoteKey(inspection: LinkInspection, key: String): Result<String> = runCatching {
        presentShareLink(inspection.instance, link.substringBefore('#'), key, true).link
    }
    fun openReceivedNote(noteLink: String, password: String? = null, complete: (Result<io.filebeam.android.platform.services.NoteContent>) -> Unit) = viewModelScope.launch {
        complete(runCatching { notes.claim(noteLink, password) })
    }
    fun startReceivedDownload() = download()
    fun retrySavedExport(id: String, complete: (Result<String>) -> Unit) = coordinator.retrySafExport(id, complete)
    fun pauseTransfer() = coordinator.pause()
    fun resumePendingTransfer() = coordinator.resumePending()
    fun savedTransferDetails(id: String, complete: (Result<io.filebeam.android.platform.SavedTransferPresentation>) -> Unit) = coordinator.savedDetails(id, complete)
    fun resumeSavedTransfer(id: String) = coordinator.resume(id)
    fun endSavedLiveTransfer(id: String) = coordinator.endLive(id)
    fun revokeSavedTransfer(id: String) = coordinator.revoke(id)
    fun discardSavedTransfer(id: String) = coordinator.discard(id)
    fun showSend(content: SendContent = SendContent.FILES) {
        sendContent = content
        destination = if (content == SendContent.NOTES) Destination.Notes else Destination.Send
        navigation = navigation.selectPrimary(DestinationRoute.Send)
    }
    fun navigate(value: Destination) {
        when (value) {
            Destination.Notes -> { showSend(SendContent.NOTES); return }
            Destination.Turbo -> { sendContent = SendContent.FILES; turboTransfer = true; showSend(); return }
            else -> Unit
        }
        destination = value
        navigation = if (value.route() in DestinationRoute.primary) navigation.selectPrimary(value.route()) else navigation.push(value.route())
    }
    /** Handles pre-scaffold route intents without keeping Notes or Turbo as dead destinations. */
    fun navigateLegacy(route: String?) = when (route?.substringBefore('/')?.lowercase()) {
        "notes" -> showSend(SendContent.NOTES)
        "turbo" -> navigate(Destination.Turbo)
        "account" -> navigate(Destination.Account)
        "settings" -> navigate(Destination.Settings)
        "receive" -> navigate(Destination.Receive)
        "transfers" -> navigate(Destination.Transfers)
        "inbox" -> navigate(Destination.Inbox)
        else -> showSend()
    }
    fun back(): Boolean {
        val next = navigation.back()
        if (next == navigation) return false
        navigation = next
        destination = if (next.current == DestinationRoute.Send && sendContent == SendContent.NOTES) Destination.Notes else next.current.destination()
        return true
    }

    fun validateRecipient() {
        val name = sendDraft.recipient.username
        val revision = recipientRevision
        val conflict = sendDraft.recipientConflict()
        if (name.isBlank()) return
        if (conflict != null) { updateSend { it.copy(recipient = it.recipient.copy(status = RecipientStatus.CONFLICT, error = conflict)) }; return }
        updateSend { it.copy(recipient = it.recipient.copy(status = RecipientStatus.VALIDATING, error = null)) }
        coordinator.validateRecipient(name) { identity, error ->
            val sameRequest = recipientRevision == revision && sendDraft.recipient.username == name
            val currentOrigin = runCatching { AccountSessionRegistry.normalizeOrigin(settings.value.instance) }.getOrNull()
            if (!sameRequest || identity?.origin != currentOrigin) return@validateRecipient
            updateSend {
                it.copy(recipient = it.recipient.copy(status = if (error == null) RecipientStatus.VALIDATED else RecipientStatus.INVALID,
                    error = error, identity = identity?.copy(revision = revision)))
            }
        }
    }

    fun updateNote(transform: (NoteDraft) -> NoteDraft) { noteDraft = transform(noteDraft); persistDrafts() }
    /** Stable, separate link-key preferences for file and note composers. */
    fun setFileIncludeKey(include: Boolean) = updateSend { it.copy(includeKeyInLink = include) }
    fun setNoteIncludeKey(include: Boolean) = updateNote { it.copy(includeKeyInLink = include) }
    fun setFileDriver(driver: String?) = updateSend { it.copy(driver = driver) }
    fun setNoteDriver(driver: String?) = updateNote { it.copy(driver = driver) }

    /** Settings UI updates only this draft; no connection state changes until check/commit succeeds. */
    fun updateInstanceInput(instance: String = instanceTransaction.draftInstance, relayOnly: Boolean = instanceTransaction.draftRelayOnly) {
        instanceTransaction = instanceTransaction.withInput(instance, relayOnly)
    }

    fun checkInstanceInput() {
        val normalized = runCatching { AccountSessionRegistry.normalizeOrigin(instanceTransaction.draftInstance) }
        if (normalized.isFailure) {
            instanceTransaction = instanceTransaction.copy(status = InstanceTransactionStatus.Error(instanceErrorFor(normalized.exceptionOrNull()!!)))
            return
        }
        instanceTransaction = instanceTransaction.copy(draftInstance = normalized.getOrThrow(), status = InstanceTransactionStatus.Checking)
        coordinator.discoverSendInstance(normalized.getOrThrow()) { result ->
            instanceTransaction = result.fold(
                onSuccess = { instanceTransaction.copy(status = InstanceTransactionStatus.ReadyToCommit) },
                onFailure = { instanceTransaction.copy(status = InstanceTransactionStatus.Error(instanceErrorFor(it))) },
            )
        }
    }

    /** Returns a confirmation state rather than silently rebinding an active job. */
    fun commitCheckedInstance() {
        if (instanceTransaction.status !is InstanceTransactionStatus.ReadyToCommit) return
        if (transfers.value.busy) {
            instanceTransaction = instanceTransaction.copy(status = InstanceTransactionStatus.ConfirmActiveTransfer)
            return
        }
        commitInstanceChange()
    }

    fun confirmInstanceChangeAfterActiveTransfer() {
        if (instanceTransaction.status !is InstanceTransactionStatus.ConfirmActiveTransfer || transfers.value.busy) return
        commitInstanceChange()
    }

    fun refreshSendDiscovery() {
        val key = sendDiscoveryKey() ?: return
        sendDiscovery = sendDiscovery + (key to SendDiscoveryState.Loading)
        coordinator.discoverSendInstance(key.origin) { result ->
            val state = result.fold(
                onSuccess = { info -> SendDiscoveryState.Ready(info.toSendPolicy(key)) },
                onFailure = { SendDiscoveryState.Failed(it.message ?: "Could not load this instance's sending policy") },
            )
            sendDiscovery = sendDiscovery + (key to state)
            if (state is SendDiscoveryState.Ready && sendDraft.isFresh() && sendDraft.driver == null) {
                updateSend { it.copy(driver = state.policy.defaultDriver) }
            }
            if (state is SendDiscoveryState.Ready && noteDraft.isFresh() && noteDraft.driver == null) {
                updateNote { it.copy(driver = state.policy.defaultDriver) }
            }
        }
    }

    fun canSubmitFiles(): Boolean {
        val policy = (activeSendDiscovery as? SendDiscoveryState.Ready)?.policy ?: return false
        if (!policy.anonymousUploads && !hasSessionFor(policy.key)) return false
        if (transport !in policy.enabledTransports) return false
        return sendDraft.limitStatus(policy) !is SendLimitStatus.Exceeds
    }

    fun canSubmitNote(): Boolean {
        val policy = (activeSendDiscovery as? SendDiscoveryState.Ready)?.policy ?: return false
        if (!policy.anonymousUploads && !hasSessionFor(policy.key)) return false
        val noteTransport = if (noteDraft.live) Transport.WEB_RTC else Transport.HTTP
        return noteTransport in policy.enabledTransports && noteDraft.limitStatus(policy) !is SendLimitStatus.Exceeds
    }

    fun upload() {
        if (!canSubmitFiles()) {
            if (activeSendDiscovery !is SendDiscoveryState.Loading) refreshSendDiscovery()
            coordinator.message("Load this instance's sending policy and sign in if required before sending")
            return
        }
        retentionHours(retentionHours).onFailure { coordinator.message(it.message); return }
        val recipient = sendDraft.recipient
        sendDraft.recipientConflict()?.let { coordinator.message(it); return }
        if (recipient.username.isNotBlank() && (recipient.status != RecipientStatus.VALIDATED || recipient.identity == null)) {
            coordinator.message("Validate the recipient before sending")
            return
        }
        val policy = (activeSendDiscovery as? SendDiscoveryState.Ready)?.policy ?: return
        val driver = sendDraft.driver ?: policy.defaultDriver
        coordinator.upload(UploadRequest(sendDraft.sources.map(SelectedSource::uri), transportForDriver(driver), archive, turboTransfer, passwordProtected,
            retentionHours(retentionHours).getOrNull(), recipients = recipient.username.takeIf(String::isNotBlank)?.let(::listOf) ?: emptyList(), recipient = recipient.identity,
            includeKeyInLink = sendDraft.includeKeyInLink, driver = driver))
        navigate(Destination.Transfers)
    }

    fun download() {
        val ingress = parseReceiveIngress(link)
        if (ingress == null || ingress.kind == ReceiveKind.UNKNOWN) { coordinator.message("Enter a supported transfer link or ID"); return }
        receiveIngress = ingress
        coordinator.download(ingress.raw)
        navigate(Destination.Transfers)
    }
    /** Legacy Settings entry is intentionally check-only; unchecked instance changes never commit. */
    fun saveSettings(value: AppSettings) { updateInstanceInput(value.instance, value.relayOnly); checkInstanceInput() }

    private fun commitInstanceChange() {
        val next = AppSettings(instanceTransaction.draftInstance.trim().trimEnd('/'), instanceTransaction.draftRelayOnly)
        val changedOrigin = next.instance != instanceTransaction.committed.instance
        if (changedOrigin) {
            sendDiscovery = emptyMap()
            recipientRevision++
            updateSend { it.copy(recipient = RecipientDraft(it.recipient.username, recipientStatus(it.recipient.username, it.transport, it.turbo, it.passwordProtected)), driver = null) }
            updateNote { it.copy(driver = null) }
        }
        instanceTransaction = InstanceSettingsTransaction(committed = next)
        viewModelScope.launch { app.settings.save(next) }
    }

    private fun updateSend(transform: (SendDraft) -> SendDraft) { sendDraft = transform(sendDraft); persistDrafts() }
    private fun persistDrafts() {
        draftRevision++
        draftWrites.trySend(sendDraft to noteDraft)
    }
    private fun restore(value: JSONObject) {
        sendDraft = SendDraft(
            sources = value.optJSONArray("sources")?.let { array -> (0 until array.length()).map { index -> array.getJSONObject(index) }.map { source ->
                SelectedSource(Uri.parse(source.getString("uri")), source.getString("identity"), source.getString("name"), source.optLong("size").takeIf { source.has("size") }, source.optString("path").takeIf(String::isNotBlank), source.optString("error").takeIf(String::isNotBlank))
            } } ?: emptyList(),
            transport = if (value.optString("transport") == Transport.WEB_RTC.name) Transport.WEB_RTC else Transport.HTTP,
            archive = value.optBoolean("archive"), turbo = value.optBoolean("turbo"), passwordProtected = value.optBoolean("password"), includeKeyInLink = value.optBoolean("includeKey", true), driver = value.optString("driver").takeIf(String::isNotBlank),
            retentionHours = value.optString("retention"), recipient = value.optJSONObject("recipientIdentity")?.let { identity ->
                RecipientDraft(identity.getString("username"), RecipientStatus.VALIDATED, identity = ValidatedRecipient(identity.getString("username"), identity.getString("origin"), identity.getString("id").toULong(), identity.getString("bundle").toULong(), identity.getString("publicKey"), 0))
            } ?: RecipientDraft(value.optString("recipient"), RecipientStatus.UNVALIDATED),
        )
        sendContent = if (value.optString("sendContent") == SendContent.NOTES.name) SendContent.NOTES else SendContent.FILES
        value.optJSONObject("note")?.let { note -> noteDraft = NoteDraft(title = note.optString("title"), body = note.optString("body"), passwordEnabled = note.optBoolean("passwordEnabled"), retentionHours = note.optString("retention"), language = note.optString("language", "plain"), burnAfterRead = note.optBoolean("burn"), live = note.optBoolean("live"), includeKeyInLink = note.optBoolean("includeKey", true), driver = note.optString("driver").takeIf(String::isNotBlank)) }
    }
    private fun sourceMetadata(uri: Uri, identity: String, path: String?): SelectedSource {
        var name = uri.lastPathSegment ?: "file"
        var size: Long? = null
        var error: String? = null
        runCatching { getApplication<Application>().contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor -> if (cursor.moveToFirst()) { cursor.getString(0)?.let { name = it }; if (!cursor.isNull(1)) size = cursor.getLong(1) } } }.onFailure { error = it.message ?: "Source metadata is unavailable" }
        return SelectedSource(uri, identity, name, size, path, error)
    }
    private fun SendDraft.toJson() = JSONObject().put("sources", JSONArray(sources.map { source -> JSONObject().put("uri", source.uri).put("identity", source.identity).put("name", source.displayName).putOpt("size", source.sizeBytes).putOpt("path", source.relativePath).putOpt("error", source.error) }))
        .put("transport", transport.name).put("archive", archive).put("turbo", turbo).put("password", passwordProtected).put("retention", retentionHours).put("recipient", recipient.username).put("includeKey", includeKeyInLink).putOpt("driver", driver).put("sendContent", sendContent.name)
        .putOpt("recipientIdentity", recipient.identity?.let { identity -> JSONObject().put("username", identity.username).put("origin", identity.origin).put("id", identity.id.toString()).put("bundle", identity.accountKeyBundleId.toString()).put("publicKey", identity.publicKey) })
    private fun NoteDraft.toJson() = JSONObject().put("title", title).put("body", body).put("passwordEnabled", passwordEnabled).put("retention", retentionHours).put("language", language).put("burn", burnAfterRead).put("live", live).put("includeKey", includeKeyInLink).putOpt("driver", driver)

    override fun onCleared() {
        draftWrites.close()
    }
}

private fun FilebeamViewModel.sendDiscoveryKey(): SendDiscoveryKey? {
    val origin = runCatching { AccountSessionRegistry.normalizeOrigin(settings.value.instance) }.getOrNull() ?: return null
    val account = accounts.state.value as? ServiceState.Ready
    return SendDiscoveryKey(origin, account?.value?.takeIf { it.instance == origin }?.id)
}

private fun FilebeamViewModel.hasSessionFor(key: SendDiscoveryKey) = key.accountId != null
private fun SendDraft.isFresh() = sources.isEmpty() && transport == Transport.HTTP && !archive && !turbo && !passwordProtected && retentionHours.isBlank() && recipient.username.isBlank() && includeKeyInLink
private fun NoteDraft.isFresh() = title.isBlank() && body.isBlank() && !passwordEnabled && retentionHours.isBlank() && language == "plain" && !burnAfterRead && !live && includeKeyInLink
internal fun transportForDriver(driver: String) = when (driver) {
    "http" -> Transport.HTTP
    "webrtc" -> Transport.WEB_RTC
    else -> throw IllegalArgumentException("Selected driver is unsupported")
}
private fun io.filebeam.rust.InstanceInfo.toSendPolicy(key: SendDiscoveryKey) = SendInstancePolicy(
    key, anonymousUploads, enabledTransports.mapNotNull { value -> when (value.lowercase()) { "http" -> Transport.HTTP; "webrtc" -> Transport.WEB_RTC; else -> null } }.toSet(), retentionHours,
    retentionOptionsHours.toSet(), defaultDriver, chunkBytes, drivers.associate { driver -> driver.driver to SendDriverPolicy(driver.driver, driver.maximumTransferBytes, driver.maximumFileCount, driver.maximumNoteBytes) },
)

private fun RecipientDraft.unvalidated(transport: Transport, turbo: Boolean, password: Boolean) =
    copy(status = recipientStatus(username, transport, turbo, password), error = null, identity = null)

private fun recipientStatus(name: String, transport: Transport, turbo: Boolean, password: Boolean) = when {
    name.isBlank() -> RecipientStatus.NONE
    transport != Transport.HTTP || turbo || password -> RecipientStatus.CONFLICT
    else -> RecipientStatus.UNVALIDATED
}
private fun Destination.route() = when (this) { Destination.Send, Destination.Notes, Destination.Turbo -> DestinationRoute.Send; Destination.Receive -> DestinationRoute.Receive; Destination.Transfers -> DestinationRoute.Transfers; Destination.Inbox -> DestinationRoute.Inbox; Destination.Settings -> DestinationRoute.Settings; Destination.StorageUsage -> DestinationRoute.StorageUsage; Destination.ReviewTransfers -> DestinationRoute.ReviewTransfers; Destination.Account -> DestinationRoute.Account }
private fun DestinationRoute.destination() = when (this) { DestinationRoute.Send -> Destination.Send; DestinationRoute.Receive, DestinationRoute.NoteViewer -> Destination.Receive; DestinationRoute.Transfers, DestinationRoute.TransferDetail -> Destination.Transfers; DestinationRoute.Inbox -> Destination.Inbox; DestinationRoute.Settings -> Destination.Settings; DestinationRoute.StorageUsage -> Destination.StorageUsage; DestinationRoute.ReviewTransfers -> Destination.ReviewTransfers; DestinationRoute.Account -> Destination.Account }
