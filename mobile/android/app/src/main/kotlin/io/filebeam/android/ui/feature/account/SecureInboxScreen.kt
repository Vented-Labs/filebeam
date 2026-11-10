package io.filebeam.android.ui.feature.account

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import io.filebeam.android.R
import io.filebeam.android.platform.services.AccountKeySituation
import io.filebeam.android.platform.services.ContactsState
import io.filebeam.android.platform.services.KeyCustody
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.feature.send.ChoiceRow
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalHost
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.AccountKeyBundle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal val receivingPolicies = listOf(
    "anyone" to R.string.policy_anyone, "authenticated" to R.string.policy_authenticated,
    "friends" to R.string.policy_friends, "nobody" to R.string.policy_nobody,
)

/** "Who can send me files" and automatic downloads from friends. Shared with the Contacts screen. */
@Composable
internal fun ReceivingDefaultsPanel(state: ContactsState?, onChange: (String, Boolean) -> Unit) {
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            FbText(stringResource(R.string.receiving_who), style = Fb.type.bodyStrong)
            if (state == null) {
                Box(Modifier.fillMaxWidth().padding(Space.lg), contentAlignment = Alignment.Center) { Spinner() }
            } else {
                receivingPolicies.forEach { (key, label) ->
                    ChoiceRow(stringResource(label), state.policy == key, { onChange(key, state.autoDownloadFriends) })
                }
                Hairline()
                SwitchRow(
                    stringResource(R.string.receiving_auto_download), state.autoDownloadFriends, { onChange(state.policy, it) },
                    description = stringResource(R.string.receiving_auto_download_description), icon = R.drawable.ic_approved_download,
                )
            }
        }
    }
}

/** Everything about receiving into this account: who may send, the receiving key, and notifications. */
@Composable
fun SecureInboxScreen() {
    val app = LocalAppModel.current
    val host = LocalHost.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val account by app.account.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    val accounts = app.app.accounts
    var contacts by remember { mutableStateOf<ContactsState?>(null) }
    var situation by remember { mutableStateOf<AccountKeySituation?>(null) }
    var activeKey by remember { mutableStateOf<AccountKeyBundle?>(null) }
    var revision by remember { mutableIntStateOf(0) }
    var wizard by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val current = account
    val generic = stringResource(R.string.unknown_error)

    LaunchedEffect(current == null) { if (current == null) navigator.replace(Route.Auth()) }
    if (current == null) return

    LaunchedEffect(revision, current.id) {
        runCatching { contacts = accounts.contacts() }
        runCatching {
            situation = accounts.keySituation()
            activeKey = withContext(Dispatchers.IO) { app.app.accountSessions.service(current.instance).accountKeys().firstOrNull { it.isActive } }
        }.onFailure { error = it.message ?: generic }
    }
    fun act(block: suspend () -> Unit) {
        error = null
        scope.launch {
            runCatching { block() }.onFailure { error = it.message ?: generic }
            revision++
        }
    }

    SubScreen(stringResource(R.string.secure_inbox_title)) {
        error?.let { message -> item { Notice(message, tone = Tone.Danger) } }
        if (!profile.usernameRoutingEnabled) {
            item { Notice(stringResource(R.string.secure_inbox_unavailable), tone = Tone.Warning) }
        }
        item { SectionLabel(stringResource(R.string.secure_inbox_key_section)) }
        item {
            val key = activeKey
            val hasKey = situation?.activeBundleId != null
            Panel(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        Column(Modifier.weight(1f)) {
                            FbText(stringResource(if (hasKey && current.inboxEnabled) R.string.secure_inbox_active else if (hasKey) R.string.secure_inbox_paused else R.string.secure_inbox_not_set_up), style = Fb.type.title)
                            if (key != null) FbText(stringResource(R.string.secure_inbox_key_version, key.version.toInt()), style = Fb.type.small, color = Fb.palette.textMuted)
                        }
                        StatusPill(
                            stringResource(if (hasKey && current.inboxEnabled) R.string.state_on else R.string.state_off),
                            tone = if (hasKey && current.inboxEnabled) Tone.Success else Tone.Neutral,
                        )
                    }
                    if (key != null) FbText(key.fingerprint.chunked(4).joinToString(" "), style = Fb.type.codeSmall, color = Fb.palette.textMuted)
                    if (hasKey) {
                        Hairline()
                        SwitchRow(
                            stringResource(R.string.secure_inbox_accept), current.inboxEnabled, { enabled -> act { accounts.setInboxEnabled(enabled) } },
                            description = stringResource(R.string.secure_inbox_accept_description), icon = R.drawable.ic_approved_archive,
                        )
                    }
                    FbButton(
                        stringResource(if (hasKey) R.string.secure_inbox_replace else R.string.secure_inbox_activate), { wizard = true }, Modifier.fillMaxWidth(),
                        style = if (hasKey) ButtonStyle.Secondary else ButtonStyle.Primary, icon = R.drawable.ic_approved_key, enabled = profile.usernameRoutingEnabled || hasKey,
                    )
                    if (hasKey) {
                        Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                            FbButton(stringResource(R.string.secure_inbox_export), {
                                host.createDocument("filebeam-account-key.fbsk1", "text/plain") { uri ->
                                    if (uri != null) act { accounts.exportKeyTo(uri); app.toast(context.getString(R.string.secure_inbox_exported), Tone.Success) }
                                }
                            }, Modifier.weight(1f), style = ButtonStyle.Quiet)
                            FbButton(stringResource(R.string.secure_inbox_import), {
                                host.openDocument(arrayOf("text/plain", "application/octet-stream")) { uri ->
                                    if (uri != null) act {
                                        val text = withContext(Dispatchers.IO) { context.contentResolver.openInputStream(uri)?.use { it.readBytes().take(16 * 1024).toByteArray().toString(Charsets.US_ASCII) } } ?: error(generic)
                                        accounts.importKey(text.trim())
                                        app.toast(context.getString(R.string.secure_inbox_imported), Tone.Success)
                                    }
                                }
                            }, Modifier.weight(1f), style = ButtonStyle.Quiet)
                        }
                    }
                }
            }
        }
        item { SectionLabel(stringResource(R.string.receiving_defaults)) }
        item { ReceivingDefaultsPanel(contacts) { policy, auto -> act { accounts.receivingDefaults(policy, auto) } } }
        item { SectionLabel(stringResource(R.string.notifications_section)) }
        item {
            Panel(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    FbText(stringResource(R.string.notifications_new_transfer), style = Fb.type.bodyStrong)
                    ChoiceRow(stringResource(R.string.notifications_email), current.notificationChannel == "mail", { act { accounts.setNotificationChannel("mail") } }, icon = R.drawable.ic_approved_bell)
                    ChoiceRow(stringResource(R.string.notifications_in_app), current.notificationChannel == "database", { act { accounts.setNotificationChannel("database") } }, icon = R.drawable.ic_approved_notification)
                    FbText(stringResource(R.string.notifications_privacy), style = Fb.type.caption, color = Fb.palette.textMuted)
                }
            }
        }
    }

    if (wizard) {
        KeyWizard(situation, onDismiss = { wizard = false }) { custody, password, acknowledged ->
            accounts.setupReceivingKey(custody, password, acknowledged)
            if (!current.inboxEnabled) accounts.setInboxEnabled(true)
            revision++
        }
    }
}

/** Creates or replaces the receiving key: choose who holds it, then confirm. */
@Composable
private fun KeyWizard(situation: AccountKeySituation?, onDismiss: () -> Unit, onCreate: suspend (KeyCustody, String?, Boolean) -> Unit) {
    SecureWindowEffect()
    val scope = rememberCoroutineScope()
    val replacing = situation?.replacementAcknowledgementRequired == true
    var custody by remember { mutableStateOf(KeyCustody.PASSWORD) }
    var password by remember { mutableStateOf("") }
    var acknowledged by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val generic = stringResource(R.string.unknown_error)
    FbSheet(
        { if (!busy) onDismiss() },
        title = stringResource(if (replacing) R.string.key_wizard_replace_title else R.string.key_wizard_title),
        subtitle = stringResource(R.string.key_wizard_subtitle),
    ) {
        if (replacing) Notice(stringResource(R.string.key_wizard_replace_warning), tone = Tone.Warning)
        ChoiceRow(stringResource(R.string.key_custody_password), custody == KeyCustody.PASSWORD, { custody = KeyCustody.PASSWORD }, description = stringResource(R.string.key_custody_password_description), icon = R.drawable.ic_approved_lock)
        ChoiceRow(stringResource(R.string.key_custody_self), custody == KeyCustody.SELF, { custody = KeyCustody.SELF }, description = stringResource(R.string.key_custody_self_description), icon = R.drawable.ic_approved_key)
        AnimatedVisibility(custody == KeyCustody.PASSWORD, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.fade()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
            PasswordField(
                password, { password = it }, label = stringResource(R.string.account_current_password),
                supporting = stringResource(R.string.key_custody_password_hint),
                showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
            )
        }
        if (replacing) SwitchRow(stringResource(R.string.key_wizard_acknowledge), acknowledged, { acknowledged = it })
        error?.let { Notice(it, tone = Tone.Danger) }
        FbButton(
            stringResource(if (replacing) R.string.secure_inbox_replace else R.string.secure_inbox_activate),
            {
                busy = true
                error = null
                scope.launch {
                    runCatching { onCreate(custody, password.takeIf { custody == KeyCustody.PASSWORD }, acknowledged) }
                        .onSuccess { onDismiss() }
                        .onFailure { error = it.message ?: generic }
                    busy = false
                }
            },
            Modifier.fillMaxWidth(),
            enabled = (custody == KeyCustody.SELF || password.isNotEmpty()) && (!replacing || acknowledged), loading = busy,
        )
        if (custody == KeyCustody.SELF) FbText(stringResource(R.string.key_custody_self_after), style = Fb.type.caption, color = Fb.palette.textMuted)
    }
}
