package io.filebeam.android

import android.view.WindowManager
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Tests the debug host only through production FilebeamScreen components. */
@RunWith(AndroidJUnit4::class)
class FixtureAccessibilityTest {
    @get:Rule val compose = createAndroidComposeRule<FixtureActivity>()

    @Test fun navigationHasLabeledActionsAndRetainsSendDraftAcrossRoutes() {
        compose.onNodeWithText(compose.activity.getString(R.string.send_title)).performScrollTo().assertIsDisplayed()
        compose.onNodeWithContentDescription(compose.activity.getString(R.string.settings)).performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.check_instance)).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.send)).performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.send_title)).performScrollTo().assertIsDisplayed()
    }

    @Test fun bottomNavigationOpensTheProductionReceiveDestination() {
        compose.onNodeWithText(compose.activity.getString(R.string.receive)).performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.transfer_link)).assertIsDisplayed()
    }

    @Test fun selectedSendUsesDebugProviderForSemanticAddAndRemove() {
        compose.activityRule.scenario.onActivity { it.showFixture("send-selected") }
        compose.waitForIdle()
        compose.onNodeWithText("selected-source.txt").assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.add_more)).performClick()
        compose.waitForIdle()
        compose.onNodeWithText("added-source.txt").assertIsDisplayed()
        compose.onNodeWithContentDescription(compose.activity.getString(R.string.remove_file, "selected-source.txt")).performClick()
        compose.waitForIdle()
        compose.onAllNodesWithText("selected-source.txt").assertCountEquals(0)
    }

    @Test fun visualFixturesUseProductionContentWithoutOnScreenQaLabels() {
        compose.activityRule.scenario.onActivity { it.showFixture("visual-receipt-verified") }
        compose.waitForIdle()
        compose.onNodeWithText(compose.activity.getString(R.string.verified_on_device)).assertIsDisplayed()
        compose.activityRule.scenario.onActivity { it.showFixture("visual-options") }
        compose.waitForIdle()
        compose.onNodeWithText(compose.activity.getString(R.string.transfer_options)).assertIsDisplayed()
    }

    @Test fun notesFixtureUsesTheProductionComposer() {
        compose.activityRule.scenario.onActivity { it.showFixture("notes") }
        compose.waitForIdle()
        compose.onNode(hasText(compose.activity.getString(R.string.note_body)).and(hasSetTextAction())).performTextInput("fixture note")
        compose.onNodeWithText(compose.activity.getString(R.string.encrypt_and_share)).assertIsDisplayed()
    }

    @Test fun settingsKeyErrorAndPublicAccountFixtureExposeOnlyProductionUi() {
        compose.activityRule.scenario.onActivity { it.showFixture("visual-settings-key-error") }
        compose.waitForIdle()
        compose.onNodeWithText(compose.activity.getString(R.string.instance_error_check_failed)).assertIsDisplayed()
        compose.activityRule.scenario.onActivity { it.showFixture("visual-account") }
        compose.waitForIdle()
        compose.onNodeWithText(compose.activity.getString(R.string.signed_in_as, "fixture-owner")).assertIsDisplayed()
        compose.activityRule.scenario.onActivity {
            check(it.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE == 0)
        }
    }
}
