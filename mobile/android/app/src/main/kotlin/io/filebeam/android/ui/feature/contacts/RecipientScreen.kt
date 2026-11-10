package io.filebeam.android.ui.feature.contacts

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.filebeam.android.R
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Emblem
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.Incoming
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.Tab
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.Recipient
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** The landing screen for a profile link: confirm who the files go to, then choose them. */
@Composable
fun RecipientScreen(username: String) {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val settings by app.settings.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    var recipient by remember { mutableStateOf<Recipient?>(null) }
    var failed by remember { mutableStateOf(false) }
    var showFingerprint by remember { mutableStateOf(false) }

    LaunchedEffect(username, settings.instance) {
        runCatching { withContext(Dispatchers.IO) { app.app.accountSessions.service(settings.instance).accountRecipient(username) } }
            .onSuccess { recipient = it }
            .onFailure { failed = true }
    }

    SubScreen(stringResource(R.string.recipient_title, username)) {
        val found = recipient
        when {
            failed -> item {
                EmptyState(R.drawable.ic_approved_alert, stringResource(R.string.recipient_unavailable_title), body = stringResource(R.string.recipient_unavailable_body), tone = Tone.Danger) {
                    FbButton(stringResource(R.string.receive_start_new), { navigator.select(Tab.Send) }, style = ButtonStyle.Secondary)
                }
            }
            !profile.httpEnabled && profile.loaded -> item {
                EmptyState(R.drawable.ic_approved_alert, stringResource(R.string.recipient_stored_unavailable), tone = Tone.Warning)
            }
            found == null -> item { Column(Modifier.fillMaxWidth().padding(Space.xxxl), horizontalAlignment = Alignment.CenterHorizontally) { Spinner() } }
            else -> item {
                Panel(Modifier.fillMaxWidth()) {
                    Column(Modifier.fillMaxWidth().padding(Space.xl), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                        Emblem()
                        FbText(stringResource(R.string.recipient_heading, found.username), style = Fb.type.headline, textAlign = TextAlign.Center)
                        FbText(stringResource(R.string.recipient_body), style = Fb.type.body, color = Fb.palette.textMuted, textAlign = TextAlign.Center)
                        FbButton(stringResource(R.string.send_choose_files), {
                            app.deliver(Incoming.Recipient(found.username))
                            app.navigate(tab = Tab.Send)
                        }, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_folder)
                        FbButton(
                            stringResource(R.string.recipient_fingerprint), { showFingerprint = !showFingerprint }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet,
                            trailingIcon = if (showFingerprint) R.drawable.ic_approved_chevron_up else R.drawable.ic_approved_chevron_down,
                        )
                        AnimatedVisibility(showFingerprint, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.enter()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
                            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                                FbText(found.fingerprint.chunked(4).joinToString(" "), style = Fb.type.codeSmall, color = Fb.palette.textMuted, textAlign = TextAlign.Center)
                                FbText(stringResource(R.string.secure_inbox_key_version, found.version.toInt()), style = Fb.type.caption, color = Fb.palette.textSubtle)
                            }
                        }
                    }
                }
            }
        }
    }
}
