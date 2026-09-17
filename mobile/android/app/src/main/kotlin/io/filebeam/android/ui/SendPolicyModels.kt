package io.filebeam.android.ui

import io.filebeam.rust.Transport
import io.filebeam.rust.estimateUploadCiphertextBytes

data class SendDiscoveryKey(val origin: String, val accountId: ULong?)

data class SendDriverPolicy(
    val name: String,
    val maximumTransferBytes: ULong?,
    val maximumFileCount: ULong?,
    val maximumNoteBytes: ULong?,
)

data class SendInstancePolicy(
    val key: SendDiscoveryKey,
    val anonymousUploads: Boolean,
    val enabledTransports: Set<Transport>,
    val retentionHours: ULong,
    val retentionOptionsHours: Set<ULong>,
    val defaultDriver: String,
    val chunkBytes: ULong,
    val drivers: Map<String, SendDriverPolicy>,
)

sealed interface SendDiscoveryState {
    data object Loading : SendDiscoveryState
    data class Ready(val policy: SendInstancePolicy) : SendDiscoveryState
    data class Failed(val message: String) : SendDiscoveryState
}

sealed interface SendLimitStatus {
    data object UnknownSize : SendLimitStatus
    data class Exceeds(val reason: String) : SendLimitStatus
    data object WithinKnownLimits : SendLimitStatus
}

/** No encrypted overhead is guessed. ZIP and unknown sources remain explicitly unknown until planning is available. */
fun SendDraft.limitStatus(policy: SendInstancePolicy): SendLimitStatus {
    return fileLimitStatus(policy, driver, archive, sources.map { it.sizeBytes })
}

internal fun fileLimitStatus(
    policy: SendInstancePolicy,
    selectedDriver: String?,
    archive: Boolean,
    sourceSizes: List<Long?>,
): SendLimitStatus {
    val driver = policy.drivers[selectedDriver ?: policy.defaultDriver] ?: return SendLimitStatus.Exceeds("Selected driver is unavailable")
    val count = if (archive) 1uL else sourceSizes.size.toULong()
    if (driver.maximumFileCount?.let { count > it } == true) return SendLimitStatus.Exceeds("The selection exceeds this driver's file-count limit")
    if (archive || sourceSizes.any { it == null }) return SendLimitStatus.UnknownSize
    if (driver.maximumTransferBytes == null) return SendLimitStatus.WithinKnownLimits
    val sizes = sourceSizes.map { it!!.toULong() }
    val estimated = runCatching { estimateUploadCiphertextBytes(sizes, policy.chunkBytes) }.getOrNull()
        ?: return SendLimitStatus.UnknownSize
    return if (estimated > driver.maximumTransferBytes) SendLimitStatus.Exceeds("The encrypted selection exceeds this driver's byte limit")
    else SendLimitStatus.WithinKnownLimits
}

fun NoteDraft.limitStatus(policy: SendInstancePolicy): SendLimitStatus {
    val driver = policy.drivers[driver ?: policy.defaultDriver] ?: return SendLimitStatus.Exceeds("Selected driver is unavailable")
    val bytes = body.toByteArray(Charsets.UTF_8).size.toULong()
    return if (driver.maximumNoteBytes?.let { bytes > it } == true) SendLimitStatus.Exceeds("The note exceeds this driver's limit") else SendLimitStatus.WithinKnownLimits
}
