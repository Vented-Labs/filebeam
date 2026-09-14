package io.filebeam.android

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativeScreensTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun nativeNavigationOpensReceiveAndSettings() {
        compose.onNodeWithText(compose.activity.getString(R.string.send_title)).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.receive)).performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.transfer_link)).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.settings)).performClick()
        compose.onNodeWithText(compose.activity.getString(R.string.check_instance)).assertIsDisplayed()
    }
}
