package io.filebeam.android

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class FeatureNavigationTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun noteEditorAcceptsContentBeforeCreatingANativeNote() {
        compose.onNodeWithText(compose.activity.getString(R.string.notes)).performClick()
        compose.onNode(hasText(compose.activity.getString(R.string.note_body)).and(hasSetTextAction())).performTextInput("private native note")
        compose.onNodeWithText(compose.activity.getString(R.string.encrypt_and_share)).assertIsDisplayed().assertIsEnabled()
    }
}
