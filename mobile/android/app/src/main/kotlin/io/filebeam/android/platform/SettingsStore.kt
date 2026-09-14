package io.filebeam.android.platform

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.map

private val Context.preferences by preferencesDataStore("settings")

data class AppSettings(
    val instance: String = "https://filebeam.io",
    val relayOnly: Boolean = false,
    val dynamicColor: Boolean = true,
)

class SettingsStore(private val context: Context) {
    private val instance = stringPreferencesKey("instance")
    private val relay = booleanPreferencesKey("relay_only")
    private val dynamic = booleanPreferencesKey("dynamic_color")
    val values = context.preferences.data.map { prefs ->
        AppSettings(
            prefs[instance] ?: "https://filebeam.io",
            prefs[relay] ?: false,
            prefs[dynamic] ?: true,
        )
    }

    suspend fun save(value: AppSettings) {
        context.preferences.edit {
            it[instance] = value.instance.trim().trimEnd('/')
            it[relay] = value.relayOnly
            it[dynamic] = value.dynamicColor
        }
    }
}
