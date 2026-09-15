package io.filebeam.android

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.filebeam.rust.ClientConfig
import io.filebeam.rust.JobState
import io.filebeam.rust.PromptType
import io.filebeam.rust.TransferClient
import io.filebeam.rust.TransferJob
import io.filebeam.rust.Transport
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.FileInputStream
import java.security.MessageDigest
import java.util.UUID

/**
 * Opt-in external-peer test seam. It is compiled only into the androidTest APK,
 * never the debug or release application APK. Callers supply only a disposable
 * HTTP instance and receive a link through instrumentation output.
 */
@RunWith(AndroidJUnit4::class)
class PeerAcceptanceTest {
    private val arguments = InstrumentationRegistry.getArguments()
    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test fun transferAgainstDisposablePeer() {
        assumeTrue(arguments.getString("filebeam.acceptance") == "true")
        val instance = requireNotNull(arguments.getString("instance")) { "instance is required" }
        val mode = requireNotNull(arguments.getString("mode")) { "mode is required" }
        val transport = when (arguments.getString("transport", "http")) {
            "http" -> Transport.HTTP
            "webrtc" -> Transport.WEB_RTC
            else -> error("transport must be http or webrtc")
        }
        val stateDirectory = File(context.noBackupFilesDir, "acceptance-${UUID.randomUUID()}").apply { mkdirs() }
        val client = TransferClient(ClientConfig(
            stateDirectory = stateDirectory.absolutePath,
            memoryBudgetMib = 128u,
            maxConcurrency = 1u,
            relayOnly = arguments.getString("relayOnly") == "true",
            allowHttp = true,
        ))
        try {
            when (mode) {
                "upload", "roundtrip", "serve" -> upload(client, instance, transport, mode)
                "download" -> download(client, instance)
                else -> error("mode must be upload, roundtrip, serve, or download")
            }
        } finally {
            client.destroy()
        }
    }

    private fun upload(client: TransferClient, instance: String, transport: Transport, mode: String) {
        val source = File(context.filesDir, "acceptance-${arguments.getString("name", "fixture.bin")}")
        writeFixture(source, arguments.getString("bytes", "1048576").toLong())
        val expected = sha256(source)
        val job = client.startUpload(instance, listOf(source.absolutePath), transport, false)
        val link = waitForLink(job)
        println("FILEBEAM_ACCEPTANCE_LINK=$link")
        println("FILEBEAM_ACCEPTANCE_SOURCE_SHA256=$expected")
        when (mode) {
            "roundtrip" -> {
                // HTTP publication consumes the single scheduler worker through
                // finalization. Do not admit its download until that worker exits.
                if (transport == Transport.HTTP) waitForTerminal(job, "upload")
                val output = File(context.cacheDir, "acceptance-output-${UUID.randomUUID()}")
                output.mkdirs()
                waitForTerminal(client.startDownload(instance, link, output.absolutePath), "download")
                assertEquals(expected, sha256(File(output, source.name)))
                println("FILEBEAM_ACCEPTANCE_OUTPUT_SHA256=$expected")
            }
            "serve" -> Thread.sleep(arguments.getString("serveSeconds", "180").toLong() * 1000)
        }
    }

    private fun download(client: TransferClient, instance: String) {
        val link = requireNotNull(arguments.getString("link")) { "link is required for download" }
        val output = File(context.cacheDir, "acceptance-output-${UUID.randomUUID()}")
        output.mkdirs()
        val done = waitForTerminal(client.startDownload(instance, link, output.absolutePath), "download")
        val path = File(done.results.single())
        println("FILEBEAM_ACCEPTANCE_OUTPUT_SHA256=${sha256(path)}")
        println("FILEBEAM_ACCEPTANCE_OUTPUT_BYTES=${path.length()}")
    }

    private fun waitForLink(job: TransferJob): String {
        val deadline = System.nanoTime() + 20L * 60 * 1_000_000_000
        while (System.nanoTime() < deadline) {
            val snapshot = observe(job, "upload")
            snapshot.shareUrl?.let { return it }
            if (snapshot.state == JobState.FAILED) error(snapshot.error ?: "upload failed")
            Thread.sleep(100)
        }
        error("upload did not publish a link")
    }

    private fun waitForTerminal(job: TransferJob, label: String) = run {
        val deadline = System.nanoTime() + 30L * 60 * 1_000_000_000
        while (System.nanoTime() < deadline) {
            val snapshot = observe(job, label)
            if (snapshot.state == JobState.COMPLETE) return@run snapshot
            if (snapshot.state == JobState.FAILED) error(snapshot.error ?: "transfer failed")
            Thread.sleep(100)
        }
        error("transfer did not complete")
    }

    private fun observe(job: TransferJob, label: String): io.filebeam.rust.TransferSnapshot {
        val snapshot = job.snapshot()
        val prompt = snapshot.prompt
        Log.i(TAG, "$label state=${snapshot.state} phase=${snapshot.phase} done=${snapshot.done}/${snapshot.total} prompt=${prompt?.kind} error=${snapshot.error}")
        if (prompt != null) when (prompt.kind) {
            PromptType.PEER_CONSENT -> job.respond(prompt.id, "yes")
            PromptType.SHARE_READY -> job.respond(prompt.id, "continue")
            else -> error("$label requires unsupported prompt ${prompt.kind}")
        }
        return snapshot
    }

    private fun writeFixture(file: File, bytes: Long) {
        require(bytes >= 0) { "bytes must not be negative" }
        val buffer = ByteArray(64 * 1024) { it.toByte() }
        file.outputStream().use { output ->
            var remaining = bytes
            while (remaining > 0) {
                val count = minOf(buffer.size.toLong(), remaining).toInt()
                output.write(buffer, 0, count)
                remaining -= count
            }
        }
    }

    private fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        FileInputStream(file).use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    private companion object { const val TAG = "FilebeamPeer" }
}
