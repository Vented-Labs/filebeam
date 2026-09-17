package io.filebeam.android.platform.storage

import android.content.Context
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import androidx.core.net.toUri
import android.os.storage.StorageManager
import io.filebeam.android.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.util.ArrayDeque
import org.json.JSONObject

/** Sources are durable seekable snapshots; bytes never pass through the FFI. */
class DocumentStorage(private val context: Context) {
    val root = File(context.noBackupFilesDir, "documents").apply { mkdirs() }
    private val treeNames = mutableMapOf<String, String>()

    data class DocumentSelection(val uri: Uri, val relativePath: String)
    data class ProviderInput(
        val identity: String,
        val displayName: String,
        val offset: Long,
        val length: Long,
        val mutationToken: String,
    )

    /** Optional provider transaction supplied by a DocumentsProvider integration. */
    interface ProviderPublication {
        fun commit()
        fun abort()
    }

    private data class ExportJournal(val source: String, val uri: String, val offset: Long, val sourceHash: String, val destinationHash: String)

    init { cleanupAbandonedImports(emptySet()) }

    suspend fun importFiles(id: String, uris: List<Uri>, live: Boolean, ensureRunning: () -> Unit, progress: (Long) -> Unit): List<String> = withContext(Dispatchers.IO) {
        val directory = File(root, "$id/sources").apply { check(mkdirs() || isDirectory) }
        var copied = 0L
        var nextSpaceCheck = 0L
        uris.mapIndexed { index, uri ->
            ensureRunning()
            retainGrant(uri, IntentFlags.READ)
            var name = "file-${index + 1}"
            var size: Long? = null
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (nameColumn >= 0 && !cursor.isNull(nameColumn)) name = cursor.getString(nameColumn)
                    val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                    if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) size = cursor.getLong(sizeColumn)
                }
            }
            val safeName = safeName(name)
            val itemDirectory = File(directory, index.toString()).apply { mkdirs() }
            val target = File(itemDirectory, safeName)
            if (!target.exists()) {
                // Leave room for encrypted live artifacts as well as the snapshot.
                val required = size?.takeIf { it >= 0 }?.let { Math.multiplyExact(it, if (live) 2L else 1L) }
                if (required != null) {
                    val manager = context.getSystemService(StorageManager::class.java)
                    val volume = manager.getUuidForPath(root)
                    if (required > manager.getAllocatableBytes(volume) - RESERVE) error(context.getString(R.string.insufficient_space))
                    manager.allocateBytes(volume, Math.addExact(required, RESERVE))
                }
                val partial = File(itemDirectory, ".$safeName.part")
                try {
                    context.contentResolver.openInputStream(uri)?.use { input ->
                        FileOutputStream(partial).use { output ->
                            val buffer = ByteArray(64 * 1024)
                            while (true) {
                                currentCoroutineContext().ensureActive()
                                ensureRunning()
                                val count = input.read(buffer)
                                if (count < 0) break
                                if (copied >= nextSpaceCheck) {
                                    if (root.usableSpace < RESERVE + count) error(context.getString(R.string.insufficient_space))
                                    nextSpaceCheck = copied + 16L * 1024 * 1024
                                }
                                output.write(buffer, 0, count)
                                copied += count
                                progress(copied)
                            }
                            output.fd.sync()
                        }
                    } ?: error(context.getString(R.string.source_unavailable))
                    check(partial.renameTo(target)) { "Could not finish the source snapshot" }
                } finally { partial.delete() }
            }
            target.absolutePath
        }
    }

    fun outputDirectory(id: String) = File(root, "$id/downloads").apply { mkdirs() }.absolutePath

    /** Verified downloads stay app-private until the user explicitly selects a SAF destination. */
    fun verifiedOutputs(id: String): List<File> {
        val directory = File(root, "$id/downloads")
        if (!directory.isDirectory) return emptyList()
        val canonicalRoot = root.canonicalFile.toPath()
        return directory.walkTopDown().filter { it.isFile }.map { it.canonicalFile }
            .filter { it.toPath().startsWith(canonicalRoot) }.toList()
    }

    suspend fun export(source: String, uri: Uri, ensureRunning: () -> Unit) = withContext(Dispatchers.IO) {
        val file = File(source).canonicalFile
        require(file.toPath().startsWith(root.canonicalFile.toPath()))
        retainGrant(uri, IntentFlags.WRITE)
        val journalFile = exportJournal(uri)
        val sourceHash = fingerprint(file)
        val previous = loadJournal(journalFile)?.takeIf { it.source == file.absolutePath && it.uri == uri.toString() && it.sourceHash == sourceHash }
        // A provider FD permits a durable offset journal. Providers without one
        // use the safe truncating stream fallback rather than pretending resume works.
        val descriptor = context.contentResolver.openFileDescriptor(uri, "rw")
        if (descriptor == null) {
            context.contentResolver.openOutputStream(uri, "wt")?.use { output ->
                copyExport(file, output, ensureRunning)
            } ?: error("The destination could not be opened")
            journalFile.delete()
            return@withContext
        }
        descriptor.use { pfd ->
            val readable = ParcelFileDescriptor.AutoCloseInputStream(ParcelFileDescriptor.dup(pfd.fileDescriptor)).channel
            val destinationSize = readable.size()
            val offset = previous?.offset?.takeIf {
                it in 0..file.length() && destinationSize >= it && fingerprint(readable, it) == previous.destinationHash
            } ?: 0L
            readable.close()
            FileOutputStream(pfd.fileDescriptor).channel.use { channel ->
                // Discard bytes written after the last durable journal checkpoint.
                channel.truncate(offset)
                channel.position(offset)
                file.inputStream().use { input ->
                    if (offset > 0) skipFully(input, offset)
                    val buffer = ByteArray(64 * 1024)
                    var written = offset
                    var journaled = offset
                    val destinationDigest = java.security.MessageDigest.getInstance("SHA-256")
                    if (offset > 0) updateDigest(readableChannel(pfd), offset, destinationDigest)
                    while (true) {
                        currentCoroutineContext().ensureActive()
                        ensureRunning()
                        val count = input.read(buffer)
                        if (count < 0) break
                        val bytes = java.nio.ByteBuffer.wrap(buffer, 0, count)
                        while (bytes.hasRemaining()) channel.write(bytes)
                        written += count
                        destinationDigest.update(buffer, 0, count)
                        if (written - journaled >= JOURNAL_INTERVAL) {
                            channel.force(true)
                            saveJournal(journalFile, ExportJournal(file.absolutePath, uri.toString(), written, sourceHash, digestHex(destinationDigest)))
                            journaled = written
                        }
                    }
                    channel.force(true)
                    if (written != journaled) saveJournal(journalFile, ExportJournal(file.absolutePath, uri.toString(), written, sourceHash, digestHex(destinationDigest)))
                }
            }
        }
        journalFile.delete()
    }

    /** Calls provider commit/abort around the durable export journal when supported. */
    suspend fun publish(source: String, uri: Uri, ensureRunning: () -> Unit, provider: ProviderPublication) {
        try {
            export(source, uri, ensureRunning)
            provider.commit()
        } catch (error: Throwable) {
            runCatching { provider.abort() }
            throw error
        }
    }

    /** Flatten a document tree to path-safe selections for ZIP or individual upload preparation. */
    fun flattenDocumentTree(tree: Uri): List<DocumentSelection> {
        retainGrant(tree, IntentFlags.READ)
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        val pending = ArrayDeque(listOf(rootId to ""))
        val selections = mutableListOf<DocumentSelection>()
        while (pending.isNotEmpty()) {
            val (id, prefix) = pending.removeFirst()
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id)
            context.contentResolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE), null, null, null)?.use { cursor ->
                while (cursor.moveToNext()) {
                    val childId = cursor.getString(0)
                    val name = safeName(cursor.getString(1))
                    val path = if (prefix.isEmpty()) name else "$prefix/$name"
                    val child = DocumentsContract.buildDocumentUriUsingTree(tree, childId)
                    if (DocumentsContract.Document.MIME_TYPE_DIR == cursor.getString(2)) pending.add(childId to path)
                    else selections += DocumentSelection(child, safeRelativePath(path))
                }
            }
        }
        return selections.sortedBy { it.relativePath }.also { flattened ->
            flattened.forEach { treeNames[it.uri.toString()] = it.relativePath }
        }
    }

    /** A seekable provider input for the FFI source callback; no bytes are copied. */
    fun providerInput(uri: Uri, displayName: String, offset: Long = 0): ProviderInput {
        require(offset >= 0) { "Source offset must not be negative" }
        retainGrant(uri, IntentFlags.READ)
        val descriptor = context.contentResolver.openFileDescriptor(uri, "r")
            ?: error("The source could not be opened")
        val length = descriptor.use { it.statSize }
        check(length >= 0 && offset <= length) { "The provider does not expose a bounded seekable source" }
        val token = context.contentResolver.query(uri, arrayOf(OpenableColumns.SIZE, DocumentsContract.Document.COLUMN_LAST_MODIFIED), null, null, null)?.use { cursor ->
            check(cursor.moveToFirst()) { "The source is unavailable" }
            "${cursor.getLong(0)}:${if (cursor.isNull(1)) 0 else cursor.getLong(1)}"
        } ?: "$length:0"
        return ProviderInput(uri.toString(), treeNames[uri.toString()] ?: safeRelativePath(displayName), offset, length - offset, token)
    }

    /** Callback sources must be reopenable after the picker activity has gone away. */
    fun hasPersistedReadGrant(uri: Uri): Boolean = context.contentResolver.persistedUriPermissions.any { grant ->
        grant.isReadPermission && (grant.uri == uri || grant.uri.authority == uri.authority && uri.toString().startsWith(grant.uri.toString()))
    }

    /** Tree leaves retain relative paths for native ZIP and collision-safe individual names. */
    fun treeProviderInputs(tree: Uri): List<ProviderInput> = flattenDocumentTree(tree).map {
        providerInput(it.uri, it.relativePath)
    }

    /** Reopens and transfers ownership of a duplicate FD to the native callback. */
    fun openProviderDescriptor(identity: String): Int {
        val descriptor = context.contentResolver.openFileDescriptor(identity.toUri(), "r")
            ?: error("The source could not be reopened")
        return descriptor.detachFd()
    }

    fun providerMutationToken(identity: String): String = providerInput(identity.toUri(), "source").mutationToken

    /** Deletes only interrupted partial imports; completed snapshots remain resumable. */
    fun cleanupAbandonedImports(recoverableIds: Set<String>) {
        val documents = root.listFiles() ?: return
        documents.filter { it.name !in recoverableIds }.forEach { job ->
            val sources = File(job, "sources")
            sources.walkTopDown().filter { it.isFile && it.name.endsWith(".part") }.forEach(File::delete)
            if (sources.exists() && sources.walkTopDown().none { it.isFile && !it.name.endsWith(".part") }) sources.deleteRecursively()
        }
    }

    private fun copyExport(source: File, output: java.io.OutputStream, ensureRunning: () -> Unit) {
        source.inputStream().use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                ensureRunning()
                val count = input.read(buffer)
                if (count < 0) break
                output.write(buffer, 0, count)
            }
            output.flush()
        }
    }

    private fun exportJournal(uri: Uri) = File(root, "exports/${uri.toString().hashCode().toUInt().toString(16)}.json").apply { parentFile?.mkdirs() }
    private fun loadJournal(file: File) = runCatching { JSONObject(file.readText()).let { ExportJournal(it.getString("source"), it.getString("uri"), it.getLong("offset"), it.getString("sourceHash"), it.getString("destinationHash")) } }.getOrNull()
    private fun saveJournal(file: File, journal: ExportJournal) {
        val partial = File(file.parentFile, ".${file.name}.part")
        FileOutputStream(partial).use { output ->
            output.write(JSONObject().put("source", journal.source).put("uri", journal.uri).put("offset", journal.offset).put("sourceHash", journal.sourceHash).put("destinationHash", journal.destinationHash).toString().toByteArray())
            output.fd.sync()
        }
        check(partial.renameTo(file)) { "Could not persist export journal" }
    }

    private fun retainGrant(uri: Uri, flags: Int) {
        runCatching { context.contentResolver.takePersistableUriPermission(uri, flags) }
    }

    private fun skipFully(input: java.io.InputStream, offset: Long) {
        var remaining = offset
        while (remaining > 0) {
            val skipped = input.skip(remaining)
            if (skipped > 0) remaining -= skipped
            else if (input.read() < 0) error("The export source was truncated")
            else remaining--
        }
    }

    private fun fingerprint(file: File): String = file.inputStream().use { input -> fingerprint(input) }
    private fun fingerprint(channel: java.nio.channels.FileChannel, length: Long): String {
        val digest = java.security.MessageDigest.getInstance("SHA-256")
        val buffer = java.nio.ByteBuffer.allocate(64 * 1024)
        var remaining = length
        var position = 0L
        while (remaining > 0) {
            buffer.clear().limit(minOf(remaining, buffer.capacity().toLong()).toInt())
            val count = channel.read(buffer, position)
            check(count > 0) { "The export destination was truncated" }
            digest.update(buffer.array(), 0, count)
            remaining -= count
            position += count
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }
    private fun readableChannel(pfd: ParcelFileDescriptor) = ParcelFileDescriptor.AutoCloseInputStream(ParcelFileDescriptor.dup(pfd.fileDescriptor)).channel
    private fun updateDigest(channel: java.nio.channels.FileChannel, length: Long, digest: java.security.MessageDigest) {
        try {
            var remaining = length
            var position = 0L
            val buffer = java.nio.ByteBuffer.allocate(64 * 1024)
            while (remaining > 0) {
                buffer.clear().limit(minOf(remaining, buffer.capacity().toLong()).toInt())
                val count = channel.read(buffer, position)
                check(count > 0) { "The export destination was truncated" }
                digest.update(buffer.array(), 0, count)
                remaining -= count
                position += count
            }
        } finally { channel.close() }
    }
    private fun digestHex(digest: java.security.MessageDigest): String =
        (digest.clone() as java.security.MessageDigest).digest().joinToString("") { "%02x".format(it) }
    private fun fingerprint(input: java.io.InputStream): String {
        val digest = java.security.MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(64 * 1024)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            digest.update(buffer, 0, count)
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    companion object {
        private const val RESERVE = 64L * 1024 * 1024
        private const val JOURNAL_INTERVAL = 1L * 1024 * 1024
        private object IntentFlags {
            const val READ = android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION
            const val WRITE = android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        }
        fun safeName(name: String): String {
            var result = name.map { if (it == '/' || it == '\\' || it.isISOControl()) '_' else it }.joinToString("").trim()
            while (result.toByteArray(Charsets.UTF_8).size > 180) result = result.substring(0, result.offsetByCodePoints(result.length, -1))
            return result.takeUnless { it.isBlank() || it == "." || it == ".." } ?: "download"
        }
        fun safeRelativePath(path: String): String = path.split('/').filter { it.isNotBlank() && it != "." && it != ".." }
            .joinToString("/") { safeName(it) }.ifBlank { "download" }
    }
}
