package io.filebeam.android.data

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import io.filebeam.android.ui.design.Appearance
import io.filebeam.android.ui.design.ColorSource
import io.filebeam.android.ui.design.ThemeMode
import io.filebeam.android.ui.design.color.ThemePreset
import io.filebeam.android.ui.design.materialYouAvailable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.appearancePreferences by preferencesDataStore("appearance")

/** Device-local look and first-run flags. Nothing here is synced to an account. */
class AppearanceStore(private val context: Context) {
    private val mode = stringPreferencesKey("mode")
    private val source = stringPreferencesKey("source")
    private val tourSeen = booleanPreferencesKey("tour_seen")
    private val peerExposureAccepted = stringPreferencesKey("peer_exposure_accepted_at")

    val appearance: Flow<Appearance> = context.appearancePreferences.data.map { prefs ->
        Appearance(ThemeMode.fromKey(prefs[mode]), decodeSource(prefs[source]))
    }
    val hasSeenTour: Flow<Boolean> = context.appearancePreferences.data.map { it[tourSeen] ?: false }

    /** Epoch millis of the last WebRTC address-exposure consent, or null. */
    val peerExposureAcceptedAt: Flow<Long?> = context.appearancePreferences.data.map { it[peerExposureAccepted]?.toLongOrNull() }

    suspend fun save(value: Appearance) {
        context.appearancePreferences.edit {
            it[mode] = value.mode.key
            it[source] = encodeSource(value.source)
        }
    }

    suspend fun markTourSeen() {
        context.appearancePreferences.edit { it[tourSeen] = true }
    }

    suspend fun acceptPeerExposure(atMillis: Long) {
        context.appearancePreferences.edit { it[peerExposureAccepted] = atMillis.toString() }
    }

    companion object {
        internal fun encodeSource(source: ColorSource): String = when (source) {
            ColorSource.MaterialYou -> "material_you"
            ColorSource.Instance -> "instance"
            is ColorSource.Preset -> "preset:${source.preset.key}"
            is ColorSource.Custom -> "custom:%06x".format(source.seed and 0xFFFFFF)
        }

        internal fun decodeSource(value: String?, dynamicAvailable: Boolean = materialYouAvailable): ColorSource = when {
            value == null -> if (dynamicAvailable) ColorSource.MaterialYou else ColorSource.Instance
            value == "material_you" -> if (dynamicAvailable) ColorSource.MaterialYou else ColorSource.Instance
            value.startsWith("preset:") -> ThemePreset.fromKey(value.removePrefix("preset:"))?.let(ColorSource::Preset) ?: ColorSource.Instance
            value.startsWith("custom:") -> value.removePrefix("custom:").toIntOrNull(16)?.let(ColorSource::Custom) ?: ColorSource.Instance
            else -> ColorSource.Instance
        }
    }
}
