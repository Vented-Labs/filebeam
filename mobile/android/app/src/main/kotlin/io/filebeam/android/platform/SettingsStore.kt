package io.filebeam.android.platform

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.remove
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map

private val Context.preferences by preferencesDataStore("settings")
internal val retiredDynamicColor = booleanPreferencesKey("dynamic_color")

internal fun migrateAppearance(prefs: MutablePreferences) {
    prefs.remove(retiredDynamicColor)
}

data class AppSettings(
    val instance: String = "https://filebeam.io",
    val relayOnly: Boolean = false,
    /** Retained only so independently-owned legacy Settings UI compiles; it is never persisted or honored. */
    @Deprecated("Appearance always follows the device") val dynamicColor: Boolean = true,
)

class SettingsStore(private val context: Context) {
    private val instance = stringPreferencesKey("instance")
    private val relay = booleanPreferencesKey("relay_only")
    val values = flow {
        // Dynamic color is now mandatory on supported Android versions. Remove only its retired opt-out.
        context.preferences.edit(::migrateAppearance)
        emitAll(context.preferences.data.map(::readSettings))
    }

    suspend fun save(value: AppSettings) {
        context.preferences.edit {
            it[instance] = value.instance.trim().trimEnd('/')
            it[relay] = value.relayOnly
            migrateAppearance(it)
        }
    }

    private fun readSettings(prefs: Preferences) = AppSettings(
        instance = prefs[instance] ?: "https://filebeam.io",
        relayOnly = prefs[relay] ?: false,
    )

}
