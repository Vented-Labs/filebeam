package io.filebeam.android.platform.storage

import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeFalse
import org.junit.Test

class StorageUsageTest {
    @Test fun categorizes_document_snapshots_native_recovery_and_verified_outputs() {
        val root = Files.createTempDirectory("usage").toFile()
        try {
            root.resolve("documents/job/sources").apply { mkdirs() }.resolve("source").writeBytes(ByteArray(3))
            root.resolve("documents/job/downloads").apply { mkdirs() }.resolve("verified").writeBytes(ByteArray(5))
            root.resolve("transfers/checkpoint").apply { mkdirs() }.resolve("state").writeBytes(ByteArray(7))

            val usage = StorageUsageScanner.scan(root.resolve("documents"), root.resolve("transfers"))
            assertEquals(StorageUsageBucket(1, 3), usage.snapshots)
            assertEquals(StorageUsageBucket(1, 7), usage.recovery)
            assertEquals(StorageUsageBucket(1, 5), usage.verified)
        } finally { root.deleteRecursively() }
    }

    @Test fun ignores_symlinks_without_following_outside_the_app_roots() {
        val root = Files.createTempDirectory("usage").toFile()
        val outside = Files.createTempFile("outside", ".bin")
        try {
            val source = root.resolve("documents/job/sources").apply { mkdirs() }
            val link = source.resolve("outside").toPath()
            runCatching { Files.createSymbolicLink(link, outside) }.getOrElse { assumeFalse("symlinks are unavailable", true) }

            val usage = StorageUsageScanner.scan(root.resolve("documents"), root.resolve("transfers"))
            assertEquals(StorageUsageBucket(), usage.snapshots)
        } finally { root.deleteRecursively(); Files.deleteIfExists(outside) }
    }
}
