package io.filebeam.android.platform.storage

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
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

    private data class ExportJournal(val source: String, val uri: String, val offset: Long)

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

    suspend fun export(source: String, uri: Uri, ensureRunning: () -> Unit) = withContext(Dispatchers.IO) {
        val file = File(source).canonicalFile
        require(file.toPath().startsWith(root.canonicalFile.toPath()))
        retainGrant(uri, IntentFlags.WRITE)
        val journalFile = exportJournal(uri)
        val previous = loadJournal(journalFile)?.takeIf { it.source == file.absolutePath && it.uri == uri.toString() }
        // A provider FD permits a durable offset journal. Providers without one
        // use the safe truncating stream fallback rather than pretending resume works.
        val descriptor = context.contentResolver.openFileDescriptor(uri, "rw")
        if (descriptor == null) {
            context.contentResolver.openOutputStream(uri, "wt")?.use { output ->
                copyExport(file, output, 0, journalFile, uri, ensureRunning)
            } ?: error("The destination could not be opened")
            journalFile.delete()
            return@withContext
        }
        descriptor.use { pfd ->
            FileOutputStream(pfd.fileDescriptor).channel.use { channel ->
                val destinationSize = channel.size()
                val offset = previous?.offset?.takeIf { it in 0..file.length() && destinationSize >= it } ?: 0L
                if (offset == 0L) channel.truncate(0)
                channel.position(offset)
                file.inputStream().use { input ->
                    if (offset > 0) skipFully(input, offset)
                    val buffer = ByteArray(64 * 1024)
                    var written = offset
                    while (true) {
                        currentCoroutineContext().ensureActive()
                        ensureRunning()
                        val count = input.read(buffer)
                        if (count < 0) break
                        val bytes = java.nio.ByteBuffer.wrap(buffer, 0, count)
                        while (bytes.hasRemaining()) channel.write(bytes)
                        written += count
                        // Never advertise an offset which is still only in the
                        // provider's write cache: recovery trusts this journal.
                        channel.force(true)
                        saveJournal(journalFile, ExportJournal(file.absolutePath, uri.toString(), written))
                    }
                    channel.force(true)
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
        return selections.sortedBy { it.relativePath }
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
        return ProviderInput(uri.toString(), safeName(displayName), offset, length - offset, token)
    }

    /** Reopens and transfers ownership of a duplicate FD to the native callback. */
    fun openProviderDescriptor(identity: String): Int {
        val descriptor = context.contentResolver.openFileDescriptor(Uri.parse(identity), "r")
            ?: error("The source could not be reopened")
        return descriptor.detachFd()
    }

    fun providerMutationToken(identity: String): String = providerInput(Uri.parse(identity), "source").mutationToken

    /** Deletes only interrupted partial imports; completed snapshots remain resumable. */
    fun cleanupAbandonedImports(recoverableIds: Set<String>) {
        val documents = root.listFiles() ?: return
        documents.filter { it.name !in recoverableIds }.forEach { job ->
            val sources = File(job, "sources")
            sources.walkTopDown().filter { it.isFile && it.name.endsWith(".part") }.forEach(File::delete)
            if (sources.exists() && sources.walkTopDown().none { it.isFile && !it.name.endsWith(".part") }) sources.deleteRecursively()
        }
    }

    private fun copyExport(source: File, output: java.io.OutputStream, offset: Long, journal: File, uri: Uri, ensureRunning: () -> Unit) {
        source.inputStream().use { input ->
            if (offset > 0) skipFully(input, offset)
            val buffer = ByteArray(64 * 1024)
            var written = offset
            while (true) {
                ensureRunning()
                val count = input.read(buffer)
                if (count < 0) break
                output.write(buffer, 0, count)
                written += count
                saveJournal(journal, ExportJournal(source.absolutePath, uri.toString(), written))
            }
            output.flush()
        }
    }

    private fun exportJournal(uri: Uri) = File(root, "exports/${uri.toString().hashCode().toUInt().toString(16)}.json").apply { parentFile?.mkdirs() }
    private fun loadJournal(file: File) = runCatching { JSONObject(file.readText()).let { ExportJournal(it.getString("source"), it.getString("uri"), it.getLong("offset")) } }.getOrNull()
    private fun saveJournal(file: File, journal: ExportJournal) {
        val partial = File(file.parentFile, ".${file.name}.part")
        FileOutputStream(partial).use { output ->
            output.write(JSONObject().put("source", journal.source).put("uri", journal.uri).put("offset", journal.offset).toString().toByteArray())
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

    companion object {
        private const val RESERVE = 64L * 1024 * 1024
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
