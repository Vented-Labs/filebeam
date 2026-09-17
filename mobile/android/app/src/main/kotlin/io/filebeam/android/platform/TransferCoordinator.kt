package io.filebeam.android.platform

import android.content.Context
import android.net.Uri
import io.filebeam.android.BuildConfig
import io.filebeam.android.R
import io.filebeam.android.platform.background.TransferScheduler
import io.filebeam.android.platform.background.ExecutionGate
import io.filebeam.android.platform.security.CheckpointSecretStore
import io.filebeam.android.platform.security.PendingTransferStore
import io.filebeam.android.platform.storage.DocumentStorage
import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.android.platform.services.AccountService
import io.filebeam.android.platform.services.NoteContent
import io.filebeam.android.ui.JobCapabilities
import io.filebeam.android.platform.services.AccountSessionRegistry.Companion.normalizeOrigin
import io.filebeam.android.ui.ValidatedRecipient
import io.filebeam.rust.ClientConfig
import io.filebeam.rust.ClientException
import io.filebeam.rust.DirectoryChoice
import io.filebeam.rust.JobState
import io.filebeam.rust.LinkInspection
import io.filebeam.rust.SavedTransfer
import io.filebeam.rust.TransferClient
import io.filebeam.rust.NativeRuntime
import io.filebeam.rust.TransferJob
import io.filebeam.rust.TransferSnapshot
import io.filebeam.rust.Transport
import io.filebeam.rust.UploadOptions
import io.filebeam.rust.UploadAuthentication
import io.filebeam.rust.UploadRecipient
import io.filebeam.rust.SecretStoreCallback
import io.filebeam.rust.SourceCallback
import io.filebeam.rust.SourceKind
import io.filebeam.rust.UploadSource
import io.filebeam.rust.NoteRequest as NativeNoteRequest
import io.filebeam.rust.splitShareLink
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

data class TransferUiState(
    val busy: Boolean = false,
    val phase: String = "preparing",
    val preparedBytes: Long = 0,
    val snapshot: TransferSnapshot? = null,
    val saved: List<SavedTransfer> = emptyList(),
    val pending: Boolean = false,
    val message: String? = null,
    val receipt: TransferReceipt? = null,
    val current: CurrentTransfer? = null,
    /** Process-memory presentation only. Never written into checkpoints or receipts. */
    val sharePresentation: SharePresentation? = null,
    /** Compatibility projection while focused UI consumers migrate to CurrentTransfer. */
    val capabilities: JobCapabilities = JobCapabilities(),
)

data class SharePresentation(val link: String, val separateKey: String? = null)

data class TransferReceipt(val id: String, val state: ReceiptState, val result: String? = null, val message: String? = null) {
    val disposition: String get() = when (state) {
        ReceiptState.VERIFIED_PRIVATE -> "private-verified"
        ReceiptState.APP_PRIVATE_PUBLISHED -> "app-private-published"
        ReceiptState.SAF_EXPORTED -> "exported"
        ReceiptState.FAILED -> "failed"
    }
}

/** Opaque lifecycle identity. Presentation links, including keyless links, are never lifecycle keys. */
data class LiveNoteHandle(val id: String, internal val job: TransferJob)

/** Durable upload intent. Password text is intentionally not an option: native prompts hold it only in RAM. */
data class UploadRequest(
    val uris: List<Uri>,
    val transport: Transport,
    val archive: Boolean,
    val turbo: Boolean = false,
    val passwordProtected: Boolean = false,
    val retentionHours: ULong? = null,
    val account: String? = null,
    val recipients: List<String> = emptyList(),
    val recipient: ValidatedRecipient? = null,
    val includeKeyInLink: Boolean = true,
    val driver: String,
)

/** Process-scoped owner. Activities collect state; Android jobs/services execute work. */
class TransferCoordinator(
    private val context: Context,
    private val settings: SettingsStore,
    private val accountSessions: AccountSessionRegistry,
    private val runtime: NativeRuntime,
    private val accounts: AccountService,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutable = MutableStateFlow(TransferUiState())
    val state = mutable.asStateFlow()
    private val execution = ExecutionGate()
    private val pending = PendingTransferStore(context)
    val storage = DocumentStorage(context)
    private var active: TransferJob? = null
    private var nativeClient: TransferClient? = null
    private var nativeClientRelayOnly: Boolean? = null
    @Volatile private var pauseRequested = false
    @Volatile private var pausedByUser = false
    @Volatile private var activeCheckpoint: String? = null
    private var controlIntent: Pair<String, String>? = null
    private val noteSubmission = Mutex()
    private var inboxCredentials: InboxCredentials? = null
    @Volatile private var activeLiveNoteId: String? = null

    private data class InboxCredentials(val transferId: String, val key: ByteArray, val cookie: String)

    init { refresh() }

    @Synchronized private fun client(relayOnly: Boolean = false): TransferClient {
        nativeClient?.takeIf { nativeClientRelayOnly == relayOnly }?.let { return it }
        check(active == null) { "Cannot replace the native scheduler while a transfer is active" }
        nativeClient?.destroy()
        val root = File(context.noBackupFilesDir, "transfers")
        val config = ClientConfig(root.absolutePath, 128u, 2u, relayOnly, BuildConfig.DEBUG)
        val secrets = CheckpointSecretStore(context, root)
        return TransferClient.newWithRuntimeCallbacks(config, runtime, object : SecretStoreCallback {
            override fun loadOrCreate(scope: String): ByteArray = secrets.loadOrCreate(scope)
            override fun remove(scope: String) = secrets.remove(scope)
        }, object : SourceCallback {
            override fun open(identity: String): Long = storage.openProviderDescriptor(identity).toLong()
            override fun mutationToken(identity: String): String = storage.providerMutationToken(identity)
        }).also { nativeClient = it; nativeClientRelayOnly = relayOnly }
    }
    private fun platformClient() = client(nativeClientRelayOnly ?: false)

    fun upload(uris: List<Uri>, transport: Transport, archive: Boolean) = upload(UploadRequest(uris, transport, archive, driver = if (transport == Transport.WEB_RTC) "webrtc" else "http"))

    fun upload(request: UploadRequest) = submit { config ->
        require(request.uris.isNotEmpty()) { "Select at least one source" }
        require(request.recipients.size <= 1) { "Filebeam currently supports one recipient" }
        require(request.recipient == null || request.recipients == listOf(request.recipient.username)) { "Recipient identity does not match the upload request" }
        require(request.retentionHours == null || request.retentionHours > 0uL) { "Retention must be positive" }
        require(request.transport == Transport.HTTP || !request.turbo) { "Turbo Transfer requires HTTP" }
        require(request.recipients.isEmpty() || (!request.passwordProtected && !request.turbo && request.transport == Transport.HTTP)) {
            "Recipient delivery conflicts with the selected transfer options"
        }
        val durableUris: List<Uri> = request.uris.takeIf { uris -> uris.all(storage::hasPersistedReadGrant) } ?: emptyList()
        val providers = runCatching { JSONArray(durableUris.map { uri ->
            val input = storage.providerInput(uri, uri.lastPathSegment ?: "file")
            JSONObject().put("identity", input.identity).put("name", input.displayName)
                .put("offset", input.offset).put("length", input.length).put("mutation", input.mutationToken)
        }) }.getOrNull()
        JSONObject().put("kind", "upload").put("uris", JSONArray(request.uris.map(Uri::toString))).putOpt("providers", providers)
            .put("live", request.transport == Transport.WEB_RTC).put("archive", request.archive)
            .put("turbo", request.turbo)
            .put("passwordProtected", request.passwordProtected).putOpt("retentionHours", request.retentionHours?.toString())
            .putOpt("account", request.account).put("recipients", JSONArray(request.recipients))
            .put("includeKey", request.includeKeyInLink)
            .put("driver", request.driver)
            .putOpt("recipient", request.recipient?.let { recipient -> JSONObject().put("username", recipient.username).put("origin", recipient.origin)
                .put("id", recipient.id.toString()).put("bundle", recipient.accountKeyBundleId.toString()).put("publicKey", recipient.publicKey) })
            .put("instance", config.instance).put("relay", config.relayOnly)
    }

    fun download(link: String) = submit { config ->
        JSONObject().put("kind", "download").put("link", link)
            .put("instance", config.instance).put("relay", config.relayOnly)
    }

    /** Credentials stay in memory while this process starts the authenticated job. */
    @Synchronized fun inboxDownload(instance: String, transferId: String, key: ByteArray, cookie: String) {
        check(key.size == 32 && cookie.isNotBlank()) { "Sign in and unlock the inbox key again" }
        inboxCredentials?.key?.fill(0)
        inboxCredentials = InboxCredentials(transferId, key.copyOf(), cookie)
        submit { config ->
            JSONObject().put("kind", "inbox-download").put("transfer", transferId)
                .put("instance", AccountSessionRegistry.normalizeOrigin(instance)).put("relay", config.relayOnly)
        }
    }

    /** Starts immediately so password and peer-consent prompts remain RAM-only;
     * the foreground service only observes this process-owned live sender. */
    @Synchronized fun startLiveNote(instance: String, request: NativeNoteRequest): LiveNoteHandle {
        check(!mutable.value.busy && active == null) { context.getString(R.string.work_already_running) }
        val job = accountSessions.service(instance).startLiveNote(request)
        val id = UUID.randomUUID().toString()
        active = job
        activeLiveNoteId = id
        mutable.update { it.copy(busy = true, phase = "preparing", snapshot = null, message = null,
            current = CurrentTransfer(id, TransferKind.NOTE_LIVE, Transport.WEB_RTC, TransferActions(pause = true))) }
        scope.launch {
            try {
                val config = settings.values.first()
                withContext(Dispatchers.IO) {
                    pending.save(JSONObject().put("kind", "note-live").put("id", id)
                        .put("instance", instance).put("relay", config.relayOnly))
                }
                TransferScheduler.start(context, true)
                // Do not wait for a composable or service callback to begin polling this native job.
                execute()
            } catch (error: Exception) {
                job.destroy()
                if (active === job) active = null
                activeLiveNoteId = null
                mutable.update { it.copy(busy = false, current = null, phase = "failed", message = error.message) }
            }
        }
        return LiveNoteHandle(id, job)
    }

    /** A note body remains in the native job's RAM-only result, never in a checkpoint or receipt. */
    suspend fun openNote(instance: String, link: String, password: String?): NoteContent = noteSubmission.withLock {
        check(!mutable.value.busy && active == null) { context.getString(R.string.work_already_running) }
        val relayOnly = settings.values.first().relayOnly
        val id = UUID.randomUUID().toString()
        val job = withContext(Dispatchers.IO) {
            accountSessions.service(instance).openNoteJob(link, password?.takeIf(String::isNotBlank), relayOnly)
        }
        active = job
        mutable.update { it.copy(busy = true, phase = "opening-note", message = null,
            current = CurrentTransfer(id, TransferKind.CONTROL, Transport.WEB_RTC, TransferActions(pause = true))) }
        try {
            while (true) {
                val snapshot = withContext(Dispatchers.IO) { job.snapshot() }
                mutable.update { it.copy(snapshot = snapshot, phase = snapshot.phase) }
                if (snapshot.state !in listOf(JobState.RUNNING, JobState.PAUSING)) {
                    if (snapshot.state != JobState.COMPLETE) error(snapshot.error ?: "Unable to open note")
                    val opened = withContext(Dispatchers.IO) { job.readNoteResult() }
                        ?: error("Note verification did not produce a body")
                    val receipt = TransferReceipt(id, ReceiptState.VERIFIED_PRIVATE, null, "Note verified in memory")
                    saveReceipt(receipt)
                    mutable.update { it.copy(receipt = receipt) }
                    return@withLock NoteContent(opened.id, opened.text, opened.title, opened.language, opened.consumed)
                }
                delay(100)
            }
            error("Note job loop ended unexpectedly")
        } finally {
            job.destroy()
            if (active === job) active = null
            mutable.update { it.copy(busy = false, current = null) }
        }
    }

    /** Signals the retained job; `execute()` remains responsible for terminal acknowledgement and cleanup. */
    fun endLiveNote(id: String) {
        if (id != activeLiveNoteId) { message("This live note is no longer active"); return }
        active?.endLive()
        mutable.update { it.copy(phase = "ending-live") }
    }

    /** HTTP notes share the same single-job arbitration as live notes and transfers. */
    suspend fun createHttpNote(block: suspend () -> io.filebeam.rust.CreatedNote): io.filebeam.rust.CreatedNote = noteSubmission.withLock {
        val id = UUID.randomUUID().toString()
        synchronized(this@TransferCoordinator) {
            check(!mutable.value.busy && active == null) { context.getString(R.string.work_already_running) }
            mutable.update { it.copy(busy = true, phase = "creating-note", message = null,
                current = CurrentTransfer(id, TransferKind.CONTROL, Transport.HTTP, TransferActions())) }
        }
        try {
            block().also { note ->
                val receipt = TransferReceipt(note.id, ReceiptState.APP_PRIVATE_PUBLISHED, null, null)
                saveReceipt(receipt)
                mutable.update { it.copy(phase = "complete", receipt = receipt) }
            }
        } finally {
            mutable.update { it.copy(busy = false, current = null) }
        }
    }

    fun resume(id: String) = submit { config ->
        resumeRequest(id, association(id).load(), config.instance, config.relayOnly)
    }

    private fun submit(clearSnapshot: Boolean = true, command: suspend (AppSettings) -> JSONObject) {
        synchronized(this) {
            if (mutable.value.busy) { message(context.getString(R.string.work_already_running)); return }
            mutable.update { it.copy(busy = true, snapshot = if (clearSnapshot) null else it.snapshot, preparedBytes = 0, phase = "preparing", message = null, sharePresentation = null) }
        }
        scope.launch {
            try {
                val config = settings.values.first()
                val request = withContext(Dispatchers.IO) {
                    command(config).apply {
                        if (!has("id")) put("id", UUID.randomUUID().toString())
                    }.also(pending::save)
                }
                pauseRequested = false
                pausedByUser = false
                TransferScheduler.start(context, request.optBoolean("live") || request.optString("kind") == "export")
            } catch (error: Exception) {
                mutable.update { it.copy(busy = false, message = error.message ?: context.getString(R.string.background_unavailable)) }
            }
        }
    }

    fun resumePending() {
        if (mutable.value.busy) return
        scope.launch {
            try {
                val request = withContext(Dispatchers.IO) { pending.load() } ?: return@launch
                pauseRequested = false
                pausedByUser = false
                mutable.update { it.copy(busy = true, message = null) }
                TransferScheduler.start(context, request.optBoolean("live") || request.optString("kind") == "export")
            } catch (error: Exception) {
                mutable.update { it.copy(busy = false, message = error.message) }
            }
        }
    }

    /** Called only by the active JobService/foreground service. */
    suspend fun execute() = execution.run {
        try {
            if (pausedByUser) return@run
            pauseRequested = false
            val request = withContext(Dispatchers.IO) { pending.load() } ?: return@run
            mutable.update { it.copy(busy = true, pending = false, message = null) }
            if (request.getString("kind") == "export") {
                mutable.update { it.copy(phase = "exporting") }
                storage.export(request.getString("source"), Uri.parse(request.getString("uri"))) {
                    if (pauseRequested) throw CancellationException()
                }
                withContext(Dispatchers.IO) { pending.clear() }
                val receipt = TransferReceipt(request.optString("id"), ReceiptState.SAF_EXPORTED, request.getString("uri"), context.getString(R.string.saved_file))
                saveReceipt(receipt)
                mutable.update { it.copy(phase = "complete", message = context.getString(R.string.saved_file), receipt = receipt) }
                return@run
            }
            val api = if (active == null) client(request.optBoolean("relay")) else null
            val id = request.getString("id")
            val checkpoint = request.optString("checkpoint").takeIf(String::isNotBlank)
            activeCheckpoint = checkpoint
            active = active ?: withContext(Dispatchers.IO) {
                val api = requireNotNull(api)
                if (pauseRequested) throw CancellationException()
                when {
                    request.getString("kind") == "live-end" -> api.endLive(request.getString("checkpoint"))
                    request.getString("kind") == "revoke" -> api.revokeUpload(request.getString("checkpoint"))
                    checkpoint != null && request.getString("kind") == "inbox-download" -> {
                        val origin = AccountSessionRegistry.normalizeOrigin(request.getString("instance"))
                        val transfer = request.getString("transfer")
                        val credentials = accounts.inboxDownloadCredentials(origin, transfer)
                        try {
                            api.resumeInboxDownload(checkpoint, origin, credentials.workingKey, credentials.cookie)
                        } finally {
                            credentials.workingKey.fill(0)
                        }
                    }
                    checkpoint != null -> api.resume(checkpoint)
                    request.getString("kind") == "upload" -> {
                        val uris = request.getJSONArray("uris").let { values ->
                            (0 until values.length()).map { Uri.parse(values.getString(it)) }
                        }
                        val info = api.discover(request.getString("instance"))
                        val driver = request.getString("driver")
                        val live = driver == "webrtc"
                        check(driver in info.enabledTransports && driver in setOf("http", "webrtc")) { "This selected driver is disabled or unsupported by the instance" }
                        check(!request.optBoolean("turbo") || !live) { "Turbo Transfer requires HTTP" }
                        val origin = AccountSessionRegistry.normalizeOrigin(request.getString("instance"))
                        val cookie = accountSessions.cookie(origin)
                        check(info.anonymousUploads || cookie != null) { "This instance requires an account to send files" }
                        val limit = if (live) info.webrtcMaximumFileCount else info.maximumFileCount
                        val count = if (request.optBoolean("archive")) 1uL else uris.size.toULong()
                        check(limit == null || count <= limit) { "The selection exceeds this instance's file-count limit" }
                        val recipientName = request.optJSONArray("recipients")?.optString(0)?.trim()?.takeIf(String::isNotBlank)
                        val recipient = recipientName?.let { username ->
                            val resolved = accountSessions.service(origin).accountRecipient(username)
                            val expected = requireNotNull(request.optJSONObject("recipient")) { "Validate the recipient before sending" }
                            require(expected.getString("origin") == origin && expected.getString("username") == resolved.username && expected.getString("id").toULong() == resolved.id &&
                                expected.getString("bundle").toULong() == resolved.accountKeyBundleId && expected.getString("publicKey") == resolved.publicKey) {
                                "Recipient identity changed. Validate it again before sending"
                            }
                            UploadRecipient(expected.getString("username"), expected.getString("id").toULong(), expected.getString("bundle").toULong(), expected.getString("publicKey"))
                        }
                        val options = UploadOptions(
                            transport = if (live) Transport.WEB_RTC else Transport.HTTP,
                            archive = request.optBoolean("archive"),
                            turbo = request.optBoolean("turbo"),
                            password = request.optBoolean("passwordProtected"),
                            retentionHours = request.optString("retentionHours").takeIf(String::isNotBlank)?.toULong(),
                            authentication = UploadAuthentication(null, cookie),
                            recipient = recipient,
                        )
                        val sources = request.optJSONArray("providers")?.let { values ->
                            (0 until values.length()).map { index -> values.getJSONObject(index) }.map { value ->
                                UploadSource(SourceKind.PROVIDER, value.getString("name"), value.getString("identity"),
                                    value.getLong("offset").toULong(), value.getLong("length").toULong(), value.getString("mutation"))
                            }
                        }
                        if (!sources.isNullOrEmpty()) {
                            api.startUploadSources(request.getString("instance"), sources, options)
                        } else {
                            val paths = storage.importFiles(id, uris, live, {
                                if (pauseRequested) throw CancellationException()
                            }) { bytes -> mutable.update { it.copy(preparedBytes = bytes) } }
                            api.startUploadWithOptions(request.getString("instance"), paths, options)
                        }
                    }
                    request.getString("kind") == "inbox-download" -> {
                        val credentials = synchronized(this@TransferCoordinator) {
                            inboxCredentials?.takeIf { it.transferId == request.getString("transfer") }
                        } ?: error("Inbox download paused. Sign in and open the delivery again to reauthenticate.")
                        try {
                            api.startInboxDownload(request.getString("instance"), credentials.transferId, credentials.key, credentials.cookie, storage.outputDirectory(id))
                        } finally {
                            credentials.key.fill(0)
                            synchronized(this@TransferCoordinator) { inboxCredentials = null }
                        }
                    }
                    else -> api.startDownload(request.getString("instance"), request.getString("link"), storage.outputDirectory(id))
                }
            }
            while (true) {
                if (pauseRequested) active?.pause()
                val snapshot = withContext(Dispatchers.IO) { active!!.snapshot() }
                val newCheckpoint = snapshot.checkpointId
                if (newCheckpoint != null && request.optString("checkpoint") != newCheckpoint) {
                    request.put("checkpoint", newCheckpoint)
                    activeCheckpoint = newCheckpoint
                    withContext(Dispatchers.IO) {
                        pending.save(request)
                        association(newCheckpoint).save(JSONObject().put("id", id)
                            .put("kind", request.getString("kind")).put("live", request.optBoolean("live"))
                            .put("instance", request.getString("instance"))
                            .putOpt("transfer", request.optString("transfer").takeIf(String::isNotBlank)))
                    }
                }
                val current = currentPresentation(request, snapshot.state)
                val share = snapshot.shareUrl?.let { raw ->
                    if (request.optString("kind") == "upload" && !request.optBoolean("includeKey", true)) {
                        runCatching { splitShareLink(request.getString("instance"), raw) }
                            .getOrNull()?.let { SharePresentation(it.link, it.separateKey) }
                    } else SharePresentation(raw)
                }
                mutable.update { it.copy(snapshot = snapshot, phase = snapshot.phase, current = current, sharePresentation = share,
                    capabilities = JobCapabilities(canPause = current.actions.pause)) }
                if (snapshot.state !in listOf(JobState.RUNNING, JobState.PAUSING)) {
                    withContext(Dispatchers.IO) {
                        if (snapshot.state == JobState.COMPLETE) {
                            pending.clear()
                            if (request.getString("kind") == "upload") File(storage.root, "$id/sources").deleteRecursively()
                            val result = snapshot.results.firstOrNull()
                            val receipt = TransferReceipt(id,
                                if (request.getString("kind") in setOf("download", "inbox-download")) ReceiptState.VERIFIED_PRIVATE else ReceiptState.APP_PRIVATE_PUBLISHED,
                                result, null)
                            saveReceipt(receipt)
                            mutable.update { it.copy(receipt = receipt) }
                        }
                    }
                    break
                }
                delay(250)
            }
        } catch (_: CancellationException) {
            awaitNativeStop()
            mutable.update { it.copy(phase = "paused") }
        } catch (error: Exception) {
            awaitNativeStop()
            mutable.update { it.copy(phase = "failed", message = error.message ?: context.getString(R.string.unknown_error)) }
        } finally {
            active?.destroy()
            active = null
            activeCheckpoint = null
            activeLiveNoteId = null
            mutable.update { it.copy(busy = false, current = null) }
            refresh()
        }
    }

    /** The execution gate is released only after Rust confirms that its worker left RUNNING/PAUSING. */
    private suspend fun awaitNativeStop() {
        val job = active ?: return
        job.pause()
        withContext(NonCancellable + Dispatchers.IO) {
            while (job.snapshot().state in listOf(JobState.RUNNING, JobState.PAUSING)) delay(50)
        }
    }

    fun pause() { pausedByUser = true; stopExecution() }
    fun systemStop() { stopExecution() }

    /** Native transports own reconnects; this records an Android network loss without discarding recovery. */
    fun networkChanged(available: Boolean) {
        if (!available && mutable.value.busy) {
            mutable.update { it.copy(phase = "reconnecting", message = context.getString(R.string.network_unavailable)) }
        }
    }

    /** Remote live-end is a native control job, distinct from pausing the local sender. */
    fun endLive(checkpointId: String) = control("live-end", checkpointId)

    /** Remote revocation consumes the checkpoint's delete token, not local recovery data. */
    fun revoke(checkpointId: String) = control("revoke", checkpointId)

    private fun control(kind: String, checkpointId: String) {
        require(checkpointId.isNotBlank())
        if (!mutable.value.busy) {
            submitControl(kind, checkpointId)
            return
        }
        synchronized(this) {
            if (!canControlActive(activeCheckpoint, checkpointId, controlIntent)) {
                message(if (controlIntent != null) "A remote control request is already pending" else "This action does not match the active transfer")
                return
            }
            controlIntent = kind to checkpointId
        }
        scope.launch {
            try {
                mutable.update { it.copy(phase = "pausing", message = null) }
                awaitNativeStop()
                while (mutable.value.busy) delay(20)
                submitControl(kind, checkpointId)
            } catch (error: Exception) {
                message(error.message ?: context.getString(R.string.unknown_error))
            } finally {
                synchronized(this@TransferCoordinator) { controlIntent = null }
            }
        }
    }

    private fun submitControl(kind: String, checkpointId: String) = submit(clearSnapshot = false) { config ->
        JSONObject().put("kind", kind).put("checkpoint", checkpointId)
            .put("instance", config.instance).put("relay", config.relayOnly)
    }

    private fun stopExecution() {
        if (!mutable.value.busy) return
        pauseRequested = true
        active?.pause()
        mutable.update { it.copy(phase = "pausing") }
    }

    fun respond(id: ULong, value: String) {
        try { active?.respond(id, value) }
        catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
    }

    /** Directory mode is a typed native choice, never an Android document-tree URI. */
    fun respondDirectory(id: ULong, choice: DirectoryChoice) {
        try { active?.respondDirectory(id, choice) }
        catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
    }

    /** Consent values remain explicit so a generic prompt can never approve peer exposure. */
    fun respondConsent(id: ULong, allowed: Boolean) {
        try { active?.respondConsent(id, allowed) }
        catch (error: ClientException) { mutable.update { it.copy(message = error.message ?: "Unable to answer prompt") } }
    }

    /** Read-only server metadata; link parsing preserves its supplied origin and fragment. */
    fun inspectLink(input: String, complete: (Result<LinkInspection>) -> Unit) = scope.launch {
        val result = runCatching {
            val config = settings.values.first()
            withContext(Dispatchers.IO) { platformClient().inspectLink(config.instance, input) }
        }
        complete(result)
    }

    fun discard(id: String) {
        if (mutable.value.busy) return
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val api = platformClient()
                    api.discard(id)
                    val association = association(id)
                    association.load()?.optString("id")?.takeIf { UUID.fromString(it).toString() == it }?.let {
                        File(storage.root, it).deleteRecursively()
                    }
                    association.clear()
                    if (pending.load()?.optString("checkpoint") == id) pending.clear()
                }
                refresh()
            } catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
        }
    }

    fun refresh() {
        scope.launch {
            try {
                val (saved, hasPending) = withContext(Dispatchers.IO) {
                    platformClient().savedTransfers() to (pending.load() != null)
                }
                val activeId = mutable.value.snapshot?.checkpointId
                mutable.update { it.copy(saved = saved.filterNot { it.id == activeId }, pending = hasPending) }
            } catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
        }
    }

    fun checkInstance(instance: String) = scope.launch {
        try {
            val info = withContext(Dispatchers.IO) {
                platformClient().discover(instance)
            }
            message(context.getString(R.string.connected_instance, info.name))
        } catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
    }

    /** Discovery-only UI hook. It does not start, pause, or rebind a transfer. */
    fun discoverSendInstance(instance: String, complete: (Result<io.filebeam.rust.InstanceInfo>) -> Unit) = scope.launch {
        complete(runCatching {
            val origin = AccountSessionRegistry.normalizeOrigin(instance)
            withContext(Dispatchers.IO) { platformClient().discover(origin) }
        })
    }

    /** Uses the same origin-scoped service and recipient contract that upload uses. */
    fun validateRecipient(username: String, complete: (ValidatedRecipient?, String?) -> Unit) = scope.launch {
        val result = runCatching {
            val config = settings.values.first()
            withContext(Dispatchers.IO) {
                val origin = normalizeOrigin(config.instance)
                accountSessions.service(origin).accountRecipient(username.trim()).let { recipient ->
                    ValidatedRecipient(recipient.username, origin, recipient.id, recipient.accountKeyBundleId, recipient.publicKey, 0)
                }
            }
        }
        complete(result.getOrNull(), result.exceptionOrNull()?.message)
    }

    fun export(source: String, uri: Uri) = submit(clearSnapshot = false) {
        JSONObject().put("kind", "export").put("id", UUID.randomUUID().toString()).put("source", source).put("uri", uri.toString())
    }

    /** Re-validates native capability before exposing a fresh, cancellable SAF save. */
    fun retrySafExport(id: String, complete: (Result<String>) -> Unit) = scope.launch {
        complete(runCatching {
            withContext(Dispatchers.IO) {
                val details = platformClient().savedTransferDetails(id)
                check(details.canRetrySave) { "This transfer is not available to save" }
                storage.verifiedOutputs(details.id).singleOrNull()?.absolutePath
                    ?: error("This verified download is no longer available to save")
            }
        })
    }

    /** Authenticated checkpoint detail, deliberately excluding keys, cookies, and action tokens. */
    fun savedDetails(id: String, complete: (Result<SavedTransferPresentation>) -> Unit) = scope.launch {
        complete(runCatching { withContext(Dispatchers.IO) { platformClient().savedTransferDetails(id).presentation() } })
    }

    private fun association(id: String): PendingTransferStore {
        require(UUID.fromString(id).toString() == id)
        return PendingTransferStore(context, "transfer-$id")
    }
    /** Notification intents carry only opaque IDs; the activity reads authoritative encrypted state. */
    fun notificationRoute(): String? = state.value.snapshot?.prompt?.id?.let { "prompt:$it" }
        ?: state.value.receipt?.id?.let { "transfer:$it" }
        ?: activeCheckpoint?.let { "transfer:$it" }
    private suspend fun saveReceipt(receipt: TransferReceipt) = withContext(Dispatchers.IO) {
        PendingTransferStore(context, "receipt-${receipt.id}").save(JSONObject().put("id", receipt.id)
            .put("state", receipt.state.name).putOpt("result", receipt.result).putOpt("message", receipt.message))
    }
    fun message(value: String?) { mutable.update { it.copy(message = value) } }
}

internal fun currentPresentation(request: JSONObject, state: JobState): CurrentTransfer {
    val kind = when (request.getString("kind")) {
        "upload" -> TransferKind.UPLOAD; "download" -> TransferKind.DOWNLOAD; "inbox-download" -> TransferKind.INBOX_DOWNLOAD
        "note-live" -> TransferKind.NOTE_LIVE; "export" -> TransferKind.EXPORT; else -> TransferKind.CONTROL
    }
    val transport = when (kind) {
        TransferKind.UPLOAD -> if (request.optBoolean("live")) Transport.WEB_RTC else Transport.HTTP
        TransferKind.NOTE_LIVE -> Transport.WEB_RTC
        else -> null
    }
    return CurrentTransfer(request.getString("id"), kind, transport, TransferActions(
        pause = state in listOf(JobState.RUNNING, JobState.PAUSING),
        // Remote-token actions are exposed only by savedDetails(), whose FFI contract verifies token availability.
    ))
}

internal fun resumeRequest(id: String, association: JSONObject?, instance: String, relayOnly: Boolean): JSONObject {
    val kind = association?.optString("kind") ?: "resume"
    return JSONObject().put("kind", kind)
        .put("id", association?.optString("id") ?: UUID.randomUUID().toString())
        .put("checkpoint", id).put("live", association?.optBoolean("live") ?: false)
        .put("relay", relayOnly)
        .put("instance", association?.optString("instance")?.takeIf(String::isNotBlank) ?: instance)
        .putOpt("transfer", if (kind == "inbox-download") association?.optString("transfer") else null)
}

internal fun canControlActive(activeCheckpoint: String?, checkpointId: String, intent: Pair<String, String>?): Boolean =
    intent == null && activeCheckpoint == checkpointId
