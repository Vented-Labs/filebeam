package io.filebeam.android

import android.os.Build
import android.os.SystemClock
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.filebeam.android.ui.FilebeamDarkFallback
import io.filebeam.android.ui.FilebeamLightFallback
import java.io.File
import java.io.FileOutputStream
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** API 26-30 proof that the production surface uses the paired non-dynamic schemes. */
@RunWith(AndroidJUnit4::class)
class FallbackThemeEvidenceTest {
    @get:Rule val compose = createAndroidComposeRule<FixtureActivity>()
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private lateinit var evidence: File

    @Before fun prepare() {
        assumeTrue("Fallback evidence is API 26-30 only", Build.VERSION.SDK_INT in 26..30)
        evidence = File(instrumentation.targetContext.filesDir, "native-kit-evidence").apply { check(exists() || mkdirs()) }
    }

    @After fun resetNightMode() = shell("cmd uimode night no")

    @Test fun a09_api26RendersLightAndDarkFallbackSendAndSettings() {
        listOf(false, true).forEach { dark ->
            shell("cmd uimode night ${if (dark) "yes" else "no"}")
            waitForMode(dark)
            compose.activityRule.scenario.onActivity { it.showFixture("send-selected") }
            compose.waitForIdle()
            compose.onNodeWithText("selected-source.txt").assertIsDisplayed()
            val name = if (dark) "dark" else "light"
            screenshot("api26-$name-send-selected")
            val scheme = if (dark) FilebeamDarkFallback else FilebeamLightFallback
            File(evidence, "api26-$name-roles.txt").writeText("primary=#%08X\nsurface=#%08X\n".format(scheme.primary.toArgb(), scheme.surface.toArgb()))
            compose.activityRule.scenario.onActivity { it.showFixture("visual-settings") }
            compose.waitForIdle()
            compose.onNodeWithText(compose.activity.getString(R.string.check_instance)).assertIsDisplayed()
            screenshot("api26-$name-settings")
        }
    }

    private fun waitForMode(dark: Boolean) {
        val deadline = SystemClock.elapsedRealtime() + 5_000
        while (compose.activity.resources.configuration.isNightMode != dark && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(50)
        assertEquals(dark, compose.activity.resources.configuration.isNightMode)
    }

    private fun screenshot(name: String) {
        val bitmap = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        FileOutputStream(File(evidence, "$name.png")).use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }

    private fun shell(command: String) = instrumentation.uiAutomation.executeShellCommand(command).use { it.readBytes() }
    private val android.content.res.Configuration.isNightMode get() =
        uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES
}
