package io.filebeam.android.ui.feature.receive

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import io.filebeam.android.R
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.FilterChip
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Tone

private val categories = listOf(
    "spam" to R.string.report_spam, "malware" to R.string.report_malware, "illegal" to R.string.report_illegal,
    "privacy" to R.string.report_privacy, "copyright" to R.string.report_copyright, "other" to R.string.report_other,
)
private const val DESCRIPTION_MAX = 2000

/** Reports a transfer to the instance's moderators. Only its identifier and this text are sent. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ReportSheet(state: ReceiveUiState, onDismiss: () -> Unit, onSubmit: (String, String, String?) -> Unit) {
    var category by rememberSaveable { mutableStateOf("spam") }
    var description by rememberSaveable { mutableStateOf("") }
    var email by rememberSaveable { mutableStateOf("") }
    FbSheet(onDismiss, title = stringResource(R.string.report_title), subtitle = if (state.reportSent) null else stringResource(R.string.report_subtitle)) {
        if (state.reportSent) {
            EmptyState(R.drawable.ic_approved_check, stringResource(R.string.report_thanks_title), body = stringResource(R.string.report_thanks_body), tone = Tone.Success)
            FbButton(stringResource(R.string.done), onDismiss, Modifier.fillMaxWidth())
            return@FbSheet
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.sm), verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            categories.forEach { (key, label) -> FilterChip(stringResource(label), key == category, { category = key }) }
        }
        Column(verticalArrangement = Arrangement.spacedBy(Space.lg)) {
            FbTextField(
                description, { if (it.length <= DESCRIPTION_MAX) description = it },
                label = stringResource(R.string.report_description), placeholder = stringResource(R.string.report_description_placeholder),
                supporting = "${description.length} / $DESCRIPTION_MAX", singleLine = false, minLines = 4,
            )
            FbTextField(
                email, { email = it.trim() },
                label = stringResource(R.string.report_email), placeholder = stringResource(R.string.report_email_placeholder),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
            )
            state.reportError?.let { Notice(it.ifBlank { stringResource(R.string.report_failed) }, tone = Tone.Danger) }
            FbButton(
                stringResource(R.string.report_submit), { onSubmit(category, description.trim(), email) }, Modifier.fillMaxWidth(),
                enabled = description.isNotBlank(), loading = state.reportBusy,
            )
        }
    }
}
