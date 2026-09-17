package io.filebeam.android.platform.storage

import java.io.File
import java.nio.file.Files
import java.util.ArrayDeque

/** Metadata-only accounting for app-private transfer files. File contents and secrets are never opened. */
data class StorageUsage(val snapshots: StorageUsageBucket, val recovery: StorageUsageBucket, val verified: StorageUsageBucket) {
    val bytes: Long get() = snapshots.bytes + recovery.bytes + verified.bytes
}

data class StorageUsageBucket(val files: Int = 0, val bytes: Long = 0)

object StorageUsageScanner {
    fun scan(documentRoot: File, nativeRoot: File): StorageUsage {
        var snapshots = StorageUsageBucket()
        var recovery = StorageUsageBucket()
        var verified = StorageUsageBucket()
        scanRoot(documentRoot) { relative ->
            when {
                relative.split(File.separatorChar).contains("sources") -> Bucket.SNAPSHOTS
                relative.split(File.separatorChar).contains("downloads") -> Bucket.VERIFIED
                else -> Bucket.RECOVERY
            }
        }.forEach { (bucket, file) ->
            when (bucket) {
                Bucket.SNAPSHOTS -> snapshots = snapshots.add(file.length())
                Bucket.RECOVERY -> recovery = recovery.add(file.length())
                Bucket.VERIFIED -> verified = verified.add(file.length())
            }
        }
        scanRoot(nativeRoot) { Bucket.RECOVERY }.forEach { (_, file) -> recovery = recovery.add(file.length()) }
        return StorageUsage(snapshots, recovery, verified)
    }

    private fun StorageUsageBucket.add(bytes: Long) = copy(files = files + 1, bytes = this.bytes + bytes.coerceAtLeast(0))

    private fun scanRoot(root: File, classify: (String) -> Bucket): List<Pair<Bucket, File>> {
        if (!root.isDirectory || Files.isSymbolicLink(root.toPath())) return emptyList()
        val files = mutableListOf<Pair<Bucket, File>>()
        val pending = ArrayDeque(listOf(root to ""))
        while (pending.isNotEmpty()) {
            val (directory, relative) = pending.removeFirst()
            // listFiles can return null when a transfer finishes or is removed during this scan.
            directory.listFiles()?.forEach { child ->
                if (Files.isSymbolicLink(child.toPath())) return@forEach
                val childRelative = if (relative.isEmpty()) child.name else "$relative${File.separator}${child.name}"
                when {
                    child.isDirectory -> pending.add(child to childRelative)
                    child.isFile -> files += classify(childRelative) to child
                }
            }
        }
        return files
    }

    private enum class Bucket { SNAPSHOTS, RECOVERY, VERIFIED }
}
