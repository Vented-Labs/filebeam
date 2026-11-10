package io.filebeam.android.ui.feature.inbox

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatAbsolute
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.common.formatRelative
import io.filebeam.android.ui.common.parseInstant
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.feature.account.LinkRow
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalHost
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.ScreenHeader
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.Tab
import io.filebeam.android.ui.shell.TabScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.InboxTransfer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

private data class Staged(val id: String, val bytes: Long, val state: String, val bundleId: ULong)

/** Files other people sent to this account. Everything stays encrypted until it is unlocked here. */
@Composable
fun InboxScreen() {
    val app = LocalAppModel.current
    val host = LocalHost.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val account by app.account.collectAsStateCompat()
    var items by remember { mutableStateOf<List<InboxTransfer>?>(null) }
    var staged by remember { mutableStateOf<List<Staged>>(emptyList()) }
    var automatic by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var revision by remember { mutableIntStateOf(0) }
    var confirming by remember { mutableStateOf<String?>(null) }
    var opening by remember { mutableStateOf<InboxTransfer?>(null) }
    var unlocking by remember { mutableStateOf<Staged?>(null) }
    var verified by remember { mutableStateOf<List<File>>(emptyList()) }
    val current = account
    val generic = stringResource(R.string.unknown_error)

    fun parseStaged(json: String) {
        val state = JSONObject(json)
        automatic = state.optBoolean("enabled", false)
        staged = state.optJSONArray("entries")?.let { array ->
            (0 until array.length()).map { array.getJSONObject(it) }.filter { it.optString("state") != "dismissed" }
                .map { Staged(it.getString("id"), it.optLong("bytes"), it.optString("state"), it.optLong("key_bundle_id").toULong()) }
        } ?: emptyList()
    }

    LaunchedEffect(current?.id, revision) {
        if (current == null) return@LaunchedEffect
        error = null
        runCatching { withContext(Dispatchers.IO) { app.app.accountSessions.service(current.instance).accountInbox() } }
            .onSuccess { items = it }
            .onFailure { error = it.message ?: generic }
        runCatching { parseStaged(app.app.accounts.automaticReceiving(null)) }
        // Seeing the inbox is what clears its unread badge.
        runCatching { app.app.inbox.acknowledgeNotificationsRead(current.instance) }
        app.refreshUnread()
    }

    TabScreen(actions = { if (current != null) IconAction(R.drawable.ic_approved_rotate_right, stringResource(R.string.refresh), { revision++ }) }) {
        item(key = "header") { Staggered(0) { ScreenHeader(stringResource(R.string.tab_inbox), subtitle = stringResource(R.string.inbox_subtitle)) } }
        if (current == null) {
            item(key = "gate") {
                EmptyState(R.drawable.ic_approved_archive, stringResource(R.string.inbox_gate_title), body = stringResource(R.string.inbox_gate_body)) {
                    FbButton(stringResource(R.string.auth_sign_in), { navigator.push(Route.Auth()) }, icon = R.drawable.ic_approved_login)
                }
            }
            return@TabScreen
        }
        if (!current.inboxEnabled) {
            item(key = "disabled") {
                Notice(stringResource(R.string.inbox_disabled), tone = Tone.Warning) {
                    FbButton(stringResource(R.string.inbox_set_up), { navigator.push(Route.SecureInbox) }, style = ButtonStyle.Secondary, height = 40.dp)
                }
            }
        }
        error?.let { message -> item(key = "error") { Notice(message, tone = Tone.Danger) { FbButton(stringResource(R.string.retry), { revision++ }, style = ButtonStyle.Secondary, height = 40.dp) } } }
        val loaded = items
        when {
            loaded == null && error == null -> item(key = "loading") { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
            loaded != null && loaded.isEmpty() -> item(key = "empty") {
                EmptyState(R.drawable.ic_approved_archive, stringResource(R.string.inbox_empty_title), body = stringResource(R.string.inbox_empty_body))
            }
            loaded != null -> items(loaded, key = { it.id }) { item ->
                InboxRow(
                    item, confirming == item.id,
                    Modifier.animateItem(fadeInSpec = Motion.enter(), placementSpec = Motion.spatial(), fadeOutSpec = Motion.exit()),
                    onOpen = { opening = item },
                    onAskDelete = { confirming = item.id },
                    onCancelDelete = { confirming = null },
                    onDelete = {
                        scope.launch {
                            runCatching { withContext(Dispatchers.IO) { app.app.accountSessions.service(current.instance).accountDeleteInboxItem(item.id) } }
                                .onSuccess {
                                    items = items?.filterNot { it.id == item.id }
                                    app.toast(resources.getString(R.string.inbox_deleted), Tone.Success)
                                }
                                .onFailure { error = it.message ?: generic }
                            confirming = null
                        }
                    },
                )
            }
        }

        item(key = "staging-label") { SectionLabel(stringResource(R.string.inbox_staging_section)) }
        item(key = "staging") {
            Panel(Modifier.fillMaxWidth()) {
                SwitchRow(
                    stringResource(R.string.inbox_auto_stage), automatic,
                    { enabled -> scope.launch { runCatching { parseStaged(app.app.accounts.automaticReceiving(enabled)) }.onFailure { error = it.message ?: generic } } },
                    Modifier.padding(horizontal = Space.lg),
                    description = stringResource(R.string.inbox_auto_stage_description), icon = R.drawable.ic_approved_download,
                )
                staged.forEach { entry ->
                    Hairline()
                    Row(Modifier.fillMaxWidth().padding(Space.lg), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        FbIcon(R.drawable.ic_approved_lock)
                        Column(Modifier.weight(1f)) {
                            FbText(formatBytes(entry.bytes), style = Fb.type.bodyStrong)
                            FbText(stringResource(if (entry.state == "staged-locked") R.string.inbox_staged_ready else R.string.inbox_staged_waiting), style = Fb.type.small, color = Fb.palette.textMuted)
                        }
                        if (entry.state == "staged-locked") FbButton(stringResource(R.string.receive_unlock), { unlocking = entry }, style = ButtonStyle.Secondary, height = 40.dp)
                        IconAction(R.drawable.ic_approved_trash, stringResource(R.string.inbox_staged_remove), {
                            scope.launch { runCatching { app.app.accounts.dismissStagedInbox(entry.id); parseStaged(app.app.accounts.automaticReceiving(null)) }.onFailure { error = it.message ?: generic } }
                        })
                    }
                }
                verified.forEach { file ->
                    Hairline()
                    Row(Modifier.fillMaxWidth().padding(Space.lg), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        FbIcon(R.drawable.ic_approved_file, tint = Fb.palette.accentText)
                        FbText(file.name, Modifier.weight(1f), style = Fb.type.bodyStrong, maxLines = 1)
                        FbButton(stringResource(R.string.receive_save), {
                            host.createDocument(file.name, "application/octet-stream") { uri -> if (uri != null) app.app.transfers.export(file.absolutePath, uri) }
                        }, style = ButtonStyle.Secondary, height = 40.dp)
                    }
                }
            }
        }
        item(key = "links") {
            Panel(Modifier.fillMaxWidth()) {
                LinkRow(R.drawable.ic_approved_users, stringResource(R.string.inbox_manage_friends), { navigator.push(Route.Contacts) })
                Hairline()
                LinkRow(R.drawable.ic_approved_shield, stringResource(R.string.inbox_receiving_settings), { navigator.push(Route.SecureInbox) })
            }
        }
    }

    opening?.let { item ->
        UnlockSheet(stringResource(R.string.inbox_unlock_title), stringResource(R.string.inbox_unlock_body), onDismiss = { opening = null }) { password ->
            app.app.inbox.download(current!!.instance, item.id, password)
            opening = null
            navigator.select(Tab.Transfers)
        }
    }
    unlocking?.let { entry ->
        UnlockSheet(stringResource(R.string.inbox_unlock_title), stringResource(R.string.inbox_unlock_staged_body), onDismiss = { unlocking = null }) { password ->
            verified = app.app.accounts.verifyStagedInbox(entry.id, entry.bundleId, password).map(::File)
            unlocking = null
            parseStaged(app.app.accounts.automaticReceiving(null))
        }
    }
}

@Composable
private fun InboxRow(item: InboxTransfer, confirming: Boolean, modifier: Modifier, onOpen: () -> Unit, onAskDelete: () -> Unit, onCancelDelete: () -> Unit, onDelete: () -> Unit) {
    val palette = Fb.palette
    val context = LocalContext.current
    val resources = LocalResources.current
    val uploading = item.completedAt.isBlank()
    Panel(modifier.fillMaxWidth(), shape = Shapes.card) {
        Column(Modifier.padding(Space.md), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                Box(Modifier.size(44.dp).clip(Shapes.control), contentAlignment = Alignment.Center) { FbIcon(R.drawable.ic_approved_lock, tint = palette.accentText, size = 24.dp) }
                Column(Modifier.weight(1f)) {
                    FbText(
                        if (uploading) stringResource(R.string.inbox_turbo_in_progress) else pluralStringResource(R.plurals.history_files, item.itemCount.toInt(), item.itemCount.toInt()),
                        style = Fb.type.bodyStrong, maxLines = 1,
                    )
                    val received = parseInstant(item.completedAt)
                    val expires = parseInstant(item.expiresAt)
                    FbText(
                        listOfNotNull(
                            formatBytes(item.ciphertextBytes.toLong()),
                            received?.let { stringResource(R.string.inbox_received, formatRelative(context, it)) },
                        ).joinToString(" · "),
                        style = Fb.type.small, color = palette.textMuted, maxLines = 1,
                    )
                    if (expires != null) FbText(stringResource(R.string.history_expires, formatAbsolute(expires)), style = Fb.type.caption, color = palette.textSubtle, maxLines = 1)
                }
            }
            AnimatedContent(confirming, transitionSpec = { fadeIn(Motion.enter()) togetherWith fadeOut(Motion.exit()) }, label = "delete") { asking ->
                if (asking) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        FbText(stringResource(R.string.inbox_delete_question), Modifier.weight(1f), style = Fb.type.smallStrong, color = palette.danger.ink)
                        FbButton(stringResource(R.string.cancel), onCancelDelete, style = ButtonStyle.Quiet, height = 40.dp)
                        FbButton(stringResource(R.string.inbox_delete_files), onDelete, style = ButtonStyle.Danger, height = 40.dp)
                    }
                } else {
                    Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                        FbButton(stringResource(R.string.inbox_open), onOpen, Modifier.weight(1f), icon = R.drawable.ic_approved_unlock, height = 44.dp)
                        FbButton(stringResource(R.string.history_delete), onAskDelete, style = ButtonStyle.Danger, height = 44.dp)
                    }
                }
            }
        }
    }
}

/** Asks for the account password when the receiving key is password-protected; a blank value uses a key held on this device. */
@Composable
private fun UnlockSheet(title: String, body: String, onDismiss: () -> Unit, onUnlock: suspend (String?) -> Unit) {
    SecureWindowEffect()
    val scope = rememberCoroutineScope()
    var password by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val generic = stringResource(R.string.unknown_error)
    FbSheet({ if (!busy) onDismiss() }, title = title, subtitle = body) {
        PasswordField(
            password, { password = it }, label = stringResource(R.string.inbox_key_password), supporting = stringResource(R.string.inbox_key_password_hint), error = error,
            showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
        )
        FbButton(stringResource(R.string.receive_unlock), {
            busy = true
            error = null
            scope.launch {
                runCatching { onUnlock(password.takeIf(String::isNotEmpty)) }.onFailure { error = it.message ?: generic }
                busy = false
            }
        }, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_unlock, loading = busy)
        FbText(stringResource(R.string.inbox_unlock_footer), style = Fb.type.caption, color = Fb.palette.textMuted)
    }
}
