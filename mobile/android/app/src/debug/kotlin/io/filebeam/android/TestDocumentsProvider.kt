package io.filebeam.android

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File
import java.io.IOException

class TestDocumentsProvider : ContentProvider() {
    override fun onCreate() = true
    override fun query(uri: android.net.Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor {
        val columns = projection ?: COLUMNS
        return MatrixCursor(columns).apply {
            addRow(columns.map<_, Any?> { column -> when (column) {
                OpenableColumns.SIZE -> file.length()
                "last_modified" -> file.lastModified()
                else -> null
            } }.toTypedArray())
        }
    }
    override fun getType(uri: android.net.Uri) = "application/octet-stream"
    override fun insert(uri: android.net.Uri, values: ContentValues?) = error("read-only")
    override fun delete(uri: android.net.Uri, selection: String?, selectionArgs: Array<out String>?) = error("read-only")
    override fun update(uri: android.net.Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?) = error("read-only")
    override fun openFile(uri: android.net.Uri, mode: String): ParcelFileDescriptor {
        check(!revoked) { "grant revoked" }
        if (nonSeekable && mode == "r") {
            val pipe = ParcelFileDescriptor.createPipe()
            Thread {
                try {
                    ParcelFileDescriptor.AutoCloseOutputStream(pipe[1]).use { output -> file.inputStream().copyTo(output) }
                } catch (_: IOException) {
                    // Snapshot fallback may close the read end after inspecting the pipe.
                }
            }.start()
            return pipe[0]
        }
        val flags = if (mode.contains('w')) ParcelFileDescriptor.MODE_CREATE or ParcelFileDescriptor.MODE_READ_WRITE else ParcelFileDescriptor.MODE_READ_ONLY
        return ParcelFileDescriptor.open(file, flags)
    }

    companion object {
        lateinit var file: File
        @Volatile var revoked = false
        @Volatile var nonSeekable = false
        private val COLUMNS = arrayOf(OpenableColumns.SIZE, "last_modified")
    }
}
