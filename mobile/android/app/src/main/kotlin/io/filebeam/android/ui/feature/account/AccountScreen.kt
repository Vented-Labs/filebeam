package io.filebeam.android.ui.feature.account

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbDialog
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.feature.send.Avatar
import io.filebeam.android.ui.feature.send.CopyField
import io.filebeam.android.ui.feature.send.copySensitive
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlinx.coroutines.launch

/** A navigation row inside a panel: icon, title, optional detail and a chevron. */
@Composable
fun LinkRow(icon: Int, title: String, onClick: () -> Unit, detail: String? = null, tint: androidx.compose.ui.graphics.Color = Fb.palette.textMuted) {
    val palette = Fb.palette
    Row(
        Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = onClick).padding(horizontal = Space.lg, vertical = Space.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        FbIcon(icon, tint = tint)
        FbText(title, Modifier.weight(1f), style = Fb.type.bodyStrong, maxLines = 1)
        if (detail != null) FbText(detail, style = Fb.type.small, color = palette.textMuted, maxLines = 1)
        FbIcon(R.drawable.ic_approved_arrow_right, tint = palette.textSubtle, size = 16.dp)
    }
}

/** The account hub: identity, the profile link, and the way into everything account-related. */
@Composable
fun AccountScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val account by app.account.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    var deleting by remember { mutableStateOf(false) }
    val current = account

    // Signing out or deleting the account leaves nothing to show here.
    LaunchedEffect(current == null) { if (current == null) navigator.replace(Route.Auth()) }
    if (current == null) return

    SubScreen(stringResource(R.string.account_title)) {
        item {
            Staggered(0) {
                Panel(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.lg)) {
                            Avatar(current.username, Modifier.size(56.dp))
                            Column(Modifier.weight(1f)) {
                                FbText(current.name.ifBlank { current.username }, style = Fb.type.title, maxLines = 1)
                                FbText("@${current.username}", style = Fb.type.small, color = Fb.palette.textMuted, maxLines = 1)
                                if (current.email.isNotBlank()) FbText(current.email, style = Fb.type.small, color = Fb.palette.textMuted, maxLines = 1)
                            }
                        }
                        if (current.emailVerifiedAt != null) {
                            StatusPill(stringResource(R.string.account_verified), tone = Tone.Success, icon = R.drawable.ic_approved_check)
                        } else {
                            Notice(stringResource(R.string.account_verification_pending), tone = Tone.Warning) {
                                FbButton(stringResource(R.string.account_verify_email), { navigator.push(Route.Auth("verify")) }, style = ButtonStyle.Secondary, height = 40.dp)
                            }
                        }
                        current.profileUrl?.takeIf { profile.usernameRoutingEnabled }?.let { url ->
                            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                                FbText(stringResource(R.string.account_profile_link), style = Fb.type.smallStrong)
                                CopyField(R.drawable.ic_approved_link, url, stringResource(R.string.copy), primary = false) {
                                    copySensitive(context, context.getString(R.string.account_profile_link), url)
                                    app.toast(context.getString(R.string.copied), Tone.Success)
                                }
                                FbText(stringResource(R.string.account_profile_link_hint), style = Fb.type.caption, color = Fb.palette.textMuted)
                            }
                        }
                    }
                }
            }
        }
        item { SectionLabel(stringResource(R.string.account_section_sharing)) }
        item {
            Staggered(1) {
                Panel(Modifier.fillMaxWidth()) {
                    LinkRow(R.drawable.ic_approved_shield, stringResource(R.string.secure_inbox_title), { navigator.push(Route.SecureInbox) }, detail = stringResource(if (current.inboxEnabled) R.string.state_on else R.string.state_off))
                    Hairline()
                    LinkRow(R.drawable.ic_approved_users, stringResource(R.string.contacts_title), { navigator.push(Route.Contacts) })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_clock, stringResource(R.string.history_title), { navigator.push(Route.History) })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_mobile, stringResource(R.string.devices_title), { navigator.push(Route.Devices) })
                }
            }
        }
        item { SectionLabel(stringResource(R.string.account_section_app)) }
        item {
            Staggered(2) {
                Panel(Modifier.fillMaxWidth()) {
                    LinkRow(R.drawable.ic_approved_brush, stringResource(R.string.appearance_title), { navigator.push(Route.Appearance) })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_settings, stringResource(R.string.settings_title), { navigator.push(Route.Settings) })
                    Hairline()
                    LinkRow(R.drawable.ic_approved_info, stringResource(R.string.about_title), { navigator.push(Route.About) })
                }
            }
        }
        item {
            Staggered(3) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FbButton(stringResource(R.string.account_sign_out), {
                        scope.launch {
                            runCatching { app.app.accounts.signOut() }.onFailure { app.toast(it.message ?: context.getString(R.string.unknown_error), Tone.Danger) }
                        }
                    }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_logout)
                    FbButton(stringResource(R.string.account_delete), { deleting = true }, Modifier.fillMaxWidth(), style = ButtonStyle.Danger)
                }
            }
        }
    }

    if (deleting) {
        SecureWindowEffect()
        var password by remember { mutableStateOf("") }
        var busy by remember { mutableStateOf(false) }
        var error by remember { mutableStateOf<String?>(null) }
        FbDialog(
            { if (!busy) deleting = false }, stringResource(R.string.account_delete_title), stringResource(R.string.account_delete_confirm),
            {
                busy = true
                error = null
                scope.launch {
                    runCatching { app.app.accounts.deleteAccount(password) }
                        .onSuccess {
                            deleting = false
                            app.toast(context.getString(R.string.account_deleted), Tone.Success)
                        }
                        .onFailure { error = it.message ?: context.getString(R.string.unknown_error) }
                    busy = false
                }
            },
            body = stringResource(R.string.account_delete_body), dismissLabel = stringResource(R.string.cancel), destructive = true,
            confirmEnabled = password.isNotEmpty(), confirmLoading = busy,
        ) {
            PasswordField(
                password, { password = it }, label = stringResource(R.string.account_current_password), error = error,
                showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
            )
        }
    }
}
