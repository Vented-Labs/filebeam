package io.filebeam.android

import android.app.WallpaperManager
import android.graphics.Bitmap
import android.graphics.Color
import android.os.Build
import android.os.SystemClock
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.graphics.toArgb
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.io.FileOutputStream
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Records API 31+ colors resolved by Android after real WallpaperManager changes. */
@RunWith(AndroidJUnit4::class)
class DynamicWallpaperEvidenceTest {
    @get:Rule val compose = createAndroidComposeRule<FixtureActivity>()

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val automation get() = instrumentation.uiAutomation
    private lateinit var evidence: File

    @Before fun prepareEvidenceDirectory() {
        assumeTrue("Dynamic colors require API 31+", Build.VERSION.SDK_INT >= 31)
        evidence = File(instrumentation.targetContext.filesDir, "native-kit-evidence").apply {
            check(exists() || mkdirs())
        }
    }

    @After fun restoreSystemNightMode() = shell("cmd uimode night no")

    @Test fun a07_a08_resolvesThreeWallpapersInBothModesAndRendersSendAndSettings() {
        val palettes = listOf("coral" to 0xffd04a3a.toInt(), "cobalt" to 0xff2864c7.toInt(), "muted" to 0xff69706b.toInt())
        val observed = mutableSetOf<Int>()

        palettes.forEach { (name, color) ->
            setWallpaper(color)
            listOf(false, true).forEach { dark ->
                shell("cmd uimode night ${if (dark) "yes" else "no"}")
                waitForNightMode(dark)
                compose.activityRule.scenario.onActivity { it.showFixture("send-selected") }
                compose.waitForIdle()
                compose.onNodeWithText("selected-source.txt").assertIsDisplayed()
                lateinit var roles: Roles
                compose.activityRule.scenario.onActivity { activity -> roles = resolvedRoles(activity, dark) }
                observed += roles.primary
                writeRoles(name, dark, roles)
                screenshot("$name-${if (dark) "dark" else "light"}-send-selected")

                compose.activityRule.scenario.onActivity { it.showFixture("visual-settings") }
                compose.waitForIdle()
                screenshot("$name-${if (dark) "dark" else "light"}-settings")
            }
        }
        org.junit.Assert.assertTrue("Three wallpapers must produce at least three Android-resolved dynamic primaries", observed.size >= 3)
    }

    @Test fun a08_systemNightChangePreservesTypedNoteDraft() {
        shell("cmd uimode night no")
        waitForNightMode(false)
        compose.onNodeWithText(compose.activity.getString(R.string.notes)).performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.note_body)).performTextInput("night-mode draft")
        shell("cmd uimode night yes")
        waitForNightMode(true)
        // The activity can recreate for system UI mode; reselect the production note surface.
        compose.activityRule.scenario.onActivity { it.showFixture("notes") }
        compose.waitForIdle()
        compose.onNodeWithText(compose.activity.getString(R.string.note_body))
            .assertTextContains("night-mode draft")
    }

    private fun setWallpaper(color: Int) {
        val bitmap = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
        automation.adoptShellPermissionIdentity("android.permission.SET_WALLPAPER")
        try {
            instrumentation.targetContext.getSystemService(WallpaperManager::class.java)
                .setBitmap(bitmap, null, true, WallpaperManager.FLAG_SYSTEM)
        } finally {
            bitmap.recycle()
            automation.dropShellPermissionIdentity()
        }
        val manager = instrumentation.targetContext.getSystemService(WallpaperManager::class.java)
        val deadline = SystemClock.elapsedRealtime() + 10_000
        while (manager.getWallpaperColors(WallpaperManager.FLAG_SYSTEM)?.primaryColor?.toArgb() != color && SystemClock.elapsedRealtime() < deadline) {
            SystemClock.sleep(100)
        }
        assertEquals(color, manager.getWallpaperColors(WallpaperManager.FLAG_SYSTEM)?.primaryColor?.toArgb())
        // Overlay publication is asynchronous after WallpaperColors updates.
        SystemClock.sleep(1_000)
    }

    private fun waitForNightMode(dark: Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 5_000
        while (compose.activity.resources.configuration.isNightMode != dark && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(50)
        assertEquals(dark, compose.activity.resources.configuration.isNightMode)
    }

    private fun resolvedRoles(activity: FixtureActivity, dark: Boolean): Roles {
        val scheme = if (dark) dynamicDarkColorScheme(activity) else dynamicLightColorScheme(activity)
        val accent = activity.resources.getIdentifier("system_accent1_500", "color", "android")
        check(accent != 0) { "Android system accent resource is unavailable" }
        val systemAccent = activity.getColor(accent)
        check(systemAccent != 0) { "Android system accent did not resolve" }
        return Roles(scheme.primary.toArgb(), scheme.onPrimary.toArgb(), scheme.primaryContainer.toArgb(), scheme.surface.toArgb(), scheme.onSurface.toArgb(), systemAccent)
    }

    private fun writeRoles(name: String, dark: Boolean, roles: Roles) {
        File(evidence, "$name-${if (dark) "dark" else "light"}-roles.txt").writeText(
            "primary=${roles.primary.hex()}\nonPrimary=${roles.onPrimary.hex()}\nprimaryContainer=${roles.primaryContainer.hex()}\nsurface=${roles.surface.hex()}\nonSurface=${roles.onSurface.hex()}\nsystemAccent1_500=${roles.systemAccent.hex()}\n",
        )
    }

    private fun screenshot(name: String) {
        val bitmap = checkNotNull(automation.takeScreenshot()) { "UiAutomation did not capture $name" }
        FileOutputStream(File(evidence, "$name.png")).use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }

    private fun shell(command: String) = automation.executeShellCommand(command).use { it.readBytes() }

    private val android.content.res.Configuration.isNightMode get() =
        uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES

    private data class Roles(val primary: Int, val onPrimary: Int, val primaryContainer: Int, val surface: Int, val onSurface: Int, val systemAccent: Int)
    private fun Int.hex() = "#%08X".format(this)
}
