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
import io.filebeam.rust.ClientConfig
import io.filebeam.rust.JobState
import io.filebeam.rust.SavedTransfer
import io.filebeam.rust.TransferClient
import io.filebeam.rust.TransferJob
import io.filebeam.rust.TransferSnapshot
import io.filebeam.rust.Transport
import io.filebeam.rust.UploadOptions
import io.filebeam.rust.UploadAuthentication
import io.filebeam.rust.SecretStoreCallback
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
)

/** Durable upload intent. Password text is intentionally not an option: native prompts hold it only in RAM. */
data class UploadRequest(
    val uris: List<Uri>,
    val transport: Transport,
    val archive: Boolean,
    val passwordProtected: Boolean = false,
    val retentionHours: ULong? = null,
    val account: String? = null,
    val recipients: List<String> = emptyList(),
)

/** Process-scoped owner. Activities collect state; Android jobs/services execute work. */
class TransferCoordinator(private val context: Context, private val settings: SettingsStore) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val mutable = MutableStateFlow(TransferUiState())
    val state = mutable.asStateFlow()
    private val execution = ExecutionGate()
    private val pending = PendingTransferStore(context)
    val storage = DocumentStorage(context)
    private var active: TransferJob? = null
    @Volatile private var pauseRequested = false
    @Volatile private var pausedByUser = false

    init { refresh() }

    private fun client(relayOnly: Boolean = false): TransferClient {
        val root = File(context.noBackupFilesDir, "transfers")
        val config = ClientConfig(root.absolutePath, 128u, 2u, relayOnly, BuildConfig.DEBUG)
        val secrets = CheckpointSecretStore(context, root)
        return TransferClient.newWithSecretStore(config, object : SecretStoreCallback {
            override fun loadOrCreate(scope: String): ByteArray = secrets.loadOrCreate(scope)
            override fun remove(scope: String) = secrets.remove(scope)
        })
    }

    fun upload(uris: List<Uri>, transport: Transport, archive: Boolean) = upload(UploadRequest(uris, transport, archive))

    fun upload(request: UploadRequest) = submit { config ->
        JSONObject().put("kind", "upload").put("uris", JSONArray(request.uris.map(Uri::toString)))
            .put("live", request.transport == Transport.WEB_RTC).put("archive", request.archive)
            .put("passwordProtected", request.passwordProtected).putOpt("retentionHours", request.retentionHours?.toString())
            .putOpt("account", request.account).put("recipients", JSONArray(request.recipients))
            .put("instance", config.instance).put("relay", config.relayOnly)
    }

    fun download(link: String) = submit { config ->
        JSONObject().put("kind", "download").put("link", link)
            .put("instance", config.instance).put("relay", config.relayOnly)
    }

    fun resume(id: String) = submit { config ->
        val association = association(id).load()
        JSONObject().put("kind", association?.optString("kind") ?: "resume")
            .put("id", association?.optString("id") ?: UUID.randomUUID().toString())
            .put("checkpoint", id).put("live", association?.optBoolean("live") ?: false)
            .put("relay", config.relayOnly).put("instance", config.instance)
    }

    private fun submit(clearSnapshot: Boolean = true, command: suspend (AppSettings) -> JSONObject) {
        if (mutable.value.busy) { message(context.getString(R.string.work_already_running)); return }
        mutable.update { it.copy(busy = true, snapshot = if (clearSnapshot) null else it.snapshot, preparedBytes = 0, phase = "preparing", message = null) }
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
        var nativeClient: TransferClient? = null
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
                mutable.update { it.copy(phase = "complete", message = context.getString(R.string.saved_file)) }
                return@run
            }
            nativeClient = client(request.optBoolean("relay"))
            val api = nativeClient
            val id = request.getString("id")
            val checkpoint = request.optString("checkpoint").takeIf(String::isNotBlank)
            active = withContext(Dispatchers.IO) {
                if (pauseRequested) throw CancellationException()
                when {
                    request.getString("kind") == "live-end" -> api.endLive(request.getString("checkpoint"))
                    request.getString("kind") == "revoke" -> api.revokeUpload(request.getString("checkpoint"))
                    checkpoint != null -> api.resume(checkpoint)
                    request.getString("kind") == "upload" -> {
                        val uris = request.getJSONArray("uris").let { values ->
                            (0 until values.length()).map { Uri.parse(values.getString(it)) }
                        }
                        val info = api.discover(request.getString("instance"))
                        check(info.anonymousUploads) { "This instance requires an account to send files" }
                        val live = request.optBoolean("live")
                        check(!request.has("account") && request.optJSONArray("recipients")?.length() == 0) {
                            "Username recipients require a newer native upload contract"
                        }
                        check((if (live) "webrtc" else "http") in info.enabledTransports) { "This transport is disabled by the instance" }
                        val limit = if (live) info.webrtcMaximumFileCount else info.maximumFileCount
                        val count = if (request.optBoolean("archive")) 1uL else uris.size.toULong()
                        check(limit == null || count <= limit) { "The selection exceeds this instance's file-count limit" }
                        val paths = storage.importFiles(id, uris, live, {
                            if (pauseRequested) throw CancellationException()
                        }) { bytes -> mutable.update { it.copy(preparedBytes = bytes) } }
                        api.startUploadWithOptions(request.getString("instance"), paths, UploadOptions(
                            transport = if (live) Transport.WEB_RTC else Transport.HTTP,
                            archive = request.optBoolean("archive"),
                            password = request.optBoolean("passwordProtected"),
                            retentionHours = request.optString("retentionHours").takeIf(String::isNotBlank)?.toULong(),
                            authentication = UploadAuthentication(null, null),
                            recipient = null,
                        ))
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
                    withContext(Dispatchers.IO) {
                        pending.save(request)
                        association(newCheckpoint).save(JSONObject().put("id", id)
                            .put("kind", request.getString("kind")).put("live", request.optBoolean("live")))
                    }
                }
                mutable.update { it.copy(snapshot = snapshot, phase = snapshot.phase) }
                if (snapshot.state !in listOf(JobState.RUNNING, JobState.PAUSING)) {
                    withContext(Dispatchers.IO) {
                        if (snapshot.state == JobState.COMPLETE) {
                            pending.clear()
                            if (request.getString("kind") == "upload") File(storage.root, "$id/sources").deleteRecursively()
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
            nativeClient?.destroy()
            mutable.update { it.copy(busy = false) }
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

    private fun control(kind: String, checkpointId: String) = submit(clearSnapshot = false) { config ->
        require(checkpointId.isNotBlank())
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

    fun discard(id: String) {
        if (mutable.value.busy) return
        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val api = client()
                    try { api.discard(id) } finally { api.destroy() }
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
                    val api = client()
                    try { api.savedTransfers() to (pending.load() != null) } finally { api.destroy() }
                }
                mutable.update { it.copy(saved = saved, pending = hasPending) }
            } catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
        }
    }

    fun checkInstance(instance: String) = scope.launch {
        try {
            val info = withContext(Dispatchers.IO) {
                val api = client()
                try { api.discover(instance) } finally { api.destroy() }
            }
            message(context.getString(R.string.connected_instance, info.name))
        } catch (error: Exception) { message(error.message ?: context.getString(R.string.unknown_error)) }
    }

    fun export(source: String, uri: Uri) = submit(clearSnapshot = false) {
        JSONObject().put("kind", "export").put("source", source).put("uri", uri.toString())
    }

    private fun association(id: String): PendingTransferStore {
        require(UUID.fromString(id).toString() == id)
        return PendingTransferStore(context, "transfer-$id")
    }
    fun message(value: String?) { mutable.update { it.copy(message = value) } }
}
