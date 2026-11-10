package io.filebeam.android.data

import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.android.ui.shell.InstanceProfile
import io.filebeam.android.ui.shell.RetentionPolicy
import io.filebeam.android.ui.shell.TransportLimits
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/** Loads what an instance allows and how it is branded, from its public policy document. */
class InstanceRepository(private val sessions: AccountSessionRegistry) {
    private val mutable = MutableStateFlow(InstanceProfile())
    val profile: StateFlow<InstanceProfile> = mutable.asStateFlow()
    private val lock = Mutex()

    suspend fun refresh(instance: String) = lock.withLock {
        val origin = runCatching { AccountSessionRegistry.normalizeOrigin(instance) }.getOrElse {
            mutable.value = InstanceProfile(origin = instance, error = it.message)
            return@withLock
        }
        if (mutable.value.origin != origin) mutable.value = InstanceProfile(origin = origin, loading = true)
        else mutable.value = mutable.value.copy(loading = true, error = null)
        mutable.value = withContext(Dispatchers.IO) {
            runCatching { parse(origin, JSONObject(sessions.service(origin).accountPolicyJson())) }
                .getOrElse { mutable.value.copy(loading = false, error = it.message ?: "This instance could not be reached") }
        }
    }

    companion object {
        private fun JSONArray?.longs(): List<Long> = if (this == null) emptyList() else (0 until length()).map { getLong(it) }
        private fun JSONObject.optLongOrNull(name: String): Long? = if (has(name) && !isNull(name)) getLong(name) else null

        internal fun parse(origin: String, json: JSONObject): InstanceProfile {
            val instance = json.optJSONObject("instance") ?: JSONObject()
            val branding = json.optJSONObject("branding") ?: JSONObject()
            val account = json.optJSONObject("account") ?: JSONObject()
            val transfer = json.optJSONObject("transfer") ?: JSONObject()
            val limits = transfer.optJSONObject("limits") ?: JSONObject()
            val retention = transfer.optJSONObject("retention") ?: JSONObject()
            fun limit(name: String) = limits.optJSONObject(name)?.let {
                TransportLimits(it.optLongOrNull("maximum_transfer_bytes"), it.optLongOrNull("maximum_file_count"), it.optLongOrNull("maximum_note_bytes"))
            } ?: TransportLimits()
            fun retention(name: String, fallback: Long) = retention.optJSONObject(name)?.let {
                RetentionPolicy(it.optLong("defaultHours", fallback), it.optJSONArray("options").longs().ifEmpty { listOf(it.optLong("defaultHours", fallback)) })
            } ?: RetentionPolicy(fallback, listOf(fallback))
            val enabled = transfer.optJSONArray("enabledDrivers")?.let { drivers -> (0 until drivers.length()).map { drivers.getString(it) } } ?: listOf("http")
            val links = branding.optJSONArray("communityLinks")?.let { array ->
                (0 until array.length()).mapNotNull { index ->
                    val item = array.optJSONObject(index) ?: return@mapNotNull null
                    val url = item.optString("url").takeIf(String::isNotBlank) ?: return@mapNotNull null
                    (item.optString("type").ifBlank { item.optString("name") }.ifBlank { "website" }) to url
                }
            } ?: emptyList()
            return InstanceProfile(
                origin = origin,
                loaded = true,
                name = instance.optString("name").ifBlank { "Filebeam" },
                primaryColor = branding.optString("primaryColor").takeIf { it.matches(Regex("#[0-9a-fA-F]{6}")) }?.removePrefix("#")?.toInt(16),
                version = instance.optString("version").takeIf(String::isNotBlank),
                copyright = branding.optString("copyright").takeIf(String::isNotBlank),
                githubUrl = branding.optString("githubUrl").takeIf(String::isNotBlank),
                communityLinks = links,
                changelogEnabled = instance.optBoolean("changelogEnabled", false),
                registrationEnabled = account.optBoolean("registrationEnabled", true),
                usernameRoutingEnabled = account.optBoolean("usernameRoutingEnabled", true),
                deviceRendezvousEnabled = account.optBoolean("deviceRendezvousEnabled", false),
                anonymousUploads = transfer.optBoolean("anonymousUploadsEnabled", true),
                defaultDriver = transfer.optString("defaultDriver").ifBlank { "http" },
                enabledDrivers = enabled,
                chunkBytes = transfer.optLong("chunkBytes", 0),
                http = limit("http"),
                webrtc = limit("webrtc"),
                fileRetention = retention("files", 24),
                noteRetention = retention("notes", 720),
            )
        }
    }
}
