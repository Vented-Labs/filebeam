package io.filebeam.android.platform

import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.mutablePreferencesOf
import androidx.datastore.preferences.core.stringPreferencesKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class SettingsAppearanceMigrationTest {
    @Test fun retiringAppearanceOptOutPreservesUnrelatedSettings() {
        val instance = stringPreferencesKey("instance")
        val relay = booleanPreferencesKey("relay_only")
        val preferences = mutablePreferencesOf(instance to "https://example.test", relay to true, retiredDynamicColor to false)

        migrateAppearance(preferences)

        assertEquals("https://example.test", preferences[instance])
        assertEquals(true, preferences[relay])
        assertFalse(preferences.asMap().containsKey(retiredDynamicColor))
    }
}
