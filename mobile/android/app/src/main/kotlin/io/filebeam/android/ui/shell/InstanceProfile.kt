package io.filebeam.android.ui.shell

import androidx.compose.runtime.Immutable

@Immutable
data class TransportLimits(val maximumTransferBytes: Long? = null, val maximumFileCount: Long? = null, val maximumNoteBytes: Long? = null)

@Immutable
data class RetentionPolicy(val defaultHours: Long, val options: List<Long>)

/** What the connected instance allows and how it presents itself. */
@Immutable
data class InstanceProfile(
    val origin: String = "",
    val loaded: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
    val name: String = "Filebeam",
    /** Brand colour as 0xRRGGBB, or null when the instance uses the default. */
    val primaryColor: Int? = null,
    val version: String? = null,
    val copyright: String? = null,
    val githubUrl: String? = null,
    val communityLinks: List<Pair<String, String>> = emptyList(),
    val changelogEnabled: Boolean = false,
    val registrationEnabled: Boolean = true,
    val usernameRoutingEnabled: Boolean = true,
    val deviceRendezvousEnabled: Boolean = false,
    val anonymousUploads: Boolean = true,
    val defaultDriver: String = "http",
    val enabledDrivers: List<String> = listOf("http"),
    val chunkBytes: Long = 0,
    val http: TransportLimits = TransportLimits(),
    val webrtc: TransportLimits = TransportLimits(),
    val fileRetention: RetentionPolicy = RetentionPolicy(24, listOf(24)),
    val noteRetention: RetentionPolicy = RetentionPolicy(720, listOf(720)),
) {
    fun limits(driver: String) = if (driver == "webrtc") webrtc else http
    val httpEnabled get() = "http" in enabledDrivers
    val webrtcEnabled get() = "webrtc" in enabledDrivers
}
