package io.filebeam.android.ui.feature.contacts

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.services.Contact
import io.filebeam.android.platform.services.ContactsState
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Segment
import io.filebeam.android.ui.design.SegmentedRail
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.feature.account.ReceivingDefaultsPanel
import io.filebeam.android.ui.feature.account.receivingPolicies
import io.filebeam.android.ui.feature.send.Avatar
import io.filebeam.android.ui.feature.send.ChoiceRow
import io.filebeam.android.ui.feature.send.copySensitive
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.Incoming
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.Tab
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlinx.coroutines.launch

private val usernameRule = Regex("^[a-z0-9_]{3,24}$")
private enum class ContactsTab { Friends, Requests, Blocked }

/** Friends, pending requests and blocked accounts, with per-friend receiving preferences. */
@Composable
fun ContactsScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    val account by app.account.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    val accounts = app.app.accounts
    var state by remember { mutableStateOf<ContactsState?>(null) }
    var tab by rememberSaveable { mutableStateOf(ContactsTab.Friends) }
    var error by remember { mutableStateOf<String?>(null) }
    var revision by remember { mutableIntStateOf(0) }
    var username by rememberSaveable { mutableStateOf("") }
    var addError by remember { mutableStateOf<String?>(null) }
    var adding by remember { mutableStateOf(false) }
    var search by rememberSaveable { mutableStateOf("") }
    var confirming by remember { mutableStateOf<Pair<String, String>?>(null) }
    var preferences by remember { mutableStateOf<Contact?>(null) }
    var defaultsOpen by rememberSaveable { mutableStateOf(false) }
    val current = account
    val generic = stringResource(R.string.unknown_error)

    LaunchedEffect(current == null) { if (current == null) navigator.replace(Route.Auth()) }
    if (current == null) return

    LaunchedEffect(revision) {
        runCatching { accounts.contacts() }.onSuccess { state = it; error = null }.onFailure { error = it.message ?: generic }
    }

    /** Runs one contact action, announces the outcome for screen readers, and refreshes on failure. */
    fun act(name: String, action: String, done: String, canSend: Boolean? = null, autoDownload: Boolean? = null) {
        scope.launch {
            runCatching { accounts.contactAction(name, action, canSend, autoDownload) }
                .onSuccess {
                    state = it
                    error = null
                    app.toast(done, Tone.Success)
                    view.announceForAccessibility(done)
                }
                .onFailure {
                    error = it.message ?: generic
                    revision++
                }
            confirming = null
        }
    }

    val loaded = state
    val friends = loaded?.contacts?.filter { it.status == "accepted" }.orEmpty()
    val incoming = loaded?.contacts?.filter { it.status == "incoming" }.orEmpty()
    val outgoing = loaded?.contacts?.filter { it.status == "outgoing" }.orEmpty()
    val blocked = loaded?.blocked.orEmpty()

    val alreadyFriends = stringResource(R.string.contacts_already_friends)
    val alreadyIncoming = stringResource(R.string.contacts_already_incoming)
    val alreadyOutgoing = stringResource(R.string.contacts_already_outgoing)
    val submit = {
        val name = username.trim().removePrefix("@").lowercase()
        addError = when {
            !usernameRule.matches(name) -> resources.getString(R.string.auth_username_rule)
            friends.any { it.username == name } -> alreadyFriends
            incoming.any { it.username == name } -> alreadyIncoming
            outgoing.any { it.username == name } -> alreadyOutgoing
            else -> null
        }
        if (addError == null) {
            adding = true
            scope.launch {
                runCatching { accounts.contactAction(name, "request") }
                    .onSuccess {
                        state = it
                        username = ""
                        val message = resources.getString(R.string.contacts_request_sent, name)
                        app.toast(message, Tone.Success)
                        view.announceForAccessibility(message)
                    }
                    .onFailure { addError = it.message ?: generic }
                adding = false
            }
        }
    }

    SubScreen(stringResource(R.string.contacts_title)) {
        item {
            Panel(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
                    FbTextField(
                        username, {
                            username = it.lowercase().filter { char -> char.isLetterOrDigit() || char == '_' || char == '@' }.take(25)
                            addError = null
                        },
                        label = stringResource(R.string.contacts_add_label), placeholder = stringResource(R.string.friend_username_placeholder),
                        leadingIcon = R.drawable.ic_approved_user, error = addError,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Send),
                        keyboardActions = KeyboardActions(onSend = { submit() }),
                    )
                    FbButton(stringResource(R.string.contacts_add), submit, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_plus, enabled = username.trim('@').length >= 3, loading = adding)
                }
            }
        }
        error?.let { message -> item { Notice(message, tone = Tone.Danger) { FbButton(stringResource(R.string.retry), { revision++ }, style = ButtonStyle.Secondary, height = 40.dp) } } }
        item {
            SegmentedRail(
                listOf(
                    Segment(ContactsTab.Friends, stringResource(R.string.contacts_tab_friends, friends.size)),
                    Segment(ContactsTab.Requests, if (incoming.isEmpty()) stringResource(R.string.contacts_tab_requests) else pluralStringResource(R.plurals.contacts_tab_requests_incoming, incoming.size, incoming.size)),
                    Segment(ContactsTab.Blocked, stringResource(R.string.contacts_tab_blocked, blocked.size)),
                ),
                tab, { tab = it },
            )
        }
        if (loaded == null && error == null) {
            item { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
        }
        when (tab) {
            ContactsTab.Friends -> {
                if (friends.size > 8) {
                    item { FbTextField(search, { search = it }, placeholder = stringResource(R.string.contacts_search), leadingIcon = R.drawable.ic_approved_search) }
                }
                val shown = friends.filter { search.isBlank() || it.username.contains(search.trim().removePrefix("@"), ignoreCase = true) }
                when {
                    loaded != null && friends.isEmpty() -> item { EmptyState(R.drawable.ic_approved_users, stringResource(R.string.contacts_empty_title), body = stringResource(R.string.contacts_empty_body)) }
                    loaded != null && shown.isEmpty() -> item { EmptyState(R.drawable.ic_approved_search, stringResource(R.string.contacts_no_match), tone = Tone.Neutral) }
                    else -> items(shown, key = { "friend-${it.id}" }) { friend ->
                        ContactCard(
                            friend.username,
                            badge = when {
                                !friend.effectiveSend -> stringResource(R.string.contacts_badge_receiving_off) to Tone.Warning
                                friend.canSend != null || friend.autoDownload != null -> stringResource(R.string.contacts_badge_custom) to Tone.Accent
                                else -> null
                            },
                            confirming = confirming?.takeIf { it.first == friend.username }?.second,
                            modifier = Modifier.animateItem(fadeInSpec = Motion.enter(), placementSpec = Motion.spatial(), fadeOutSpec = Motion.exit()),
                            onCancel = { confirming = null },
                            onConfirm = { action ->
                                act(friend.username, action, resources.getString(if (action == "block") R.string.contacts_blocked_done else R.string.contacts_removed_done, friend.username))
                            },
                        ) {
                            FbButton(stringResource(R.string.contacts_send_files), {
                                app.deliver(Incoming.Recipient(friend.username))
                                app.navigate(tab = Tab.Send)
                            }, Modifier.weight(1f), icon = R.drawable.ic_approved_upload, height = 44.dp)
                            IconAction(R.drawable.ic_approved_settings, stringResource(R.string.contacts_preferences), { preferences = friend }, bordered = true)
                            IconAction(R.drawable.ic_approved_link, stringResource(R.string.contacts_copy_profile), {
                                copySensitive(context, resources.getString(R.string.account_profile_link), "${current.instance}/u/${friend.username}")
                                app.toast(resources.getString(R.string.copied), Tone.Success)
                            }, bordered = true)
                            IconAction(R.drawable.ic_approved_trash, stringResource(R.string.contacts_remove), { confirming = friend.username to "remove" }, bordered = true)
                            IconAction(R.drawable.ic_approved_slash, stringResource(R.string.contacts_block), { confirming = friend.username to "block" }, bordered = true, tint = Fb.palette.danger.ink)
                        }
                    }
                }
            }
            ContactsTab.Requests -> {
                if (loaded != null && incoming.isEmpty() && outgoing.isEmpty()) {
                    item { EmptyState(R.drawable.ic_approved_bell, stringResource(R.string.contacts_requests_empty)) }
                }
                if (incoming.isNotEmpty()) item { SectionLabel(stringResource(R.string.contacts_incoming)) }
                items(incoming, key = { "in-${it.id}" }) { request ->
                    ContactCard(
                        request.username, null, confirming?.takeIf { it.first == request.username }?.second, Modifier.animateItem(placementSpec = Motion.spatial()),
                        onCancel = { confirming = null },
                        onConfirm = { act(request.username, "block", resources.getString(R.string.contacts_blocked_done, request.username)) },
                    ) {
                        FbButton(stringResource(R.string.contacts_accept), { act(request.username, "accept", resources.getString(R.string.contacts_accepted_done, request.username)) }, Modifier.weight(1f), height = 44.dp)
                        FbButton(stringResource(R.string.contacts_decline), { act(request.username, "decline", resources.getString(R.string.contacts_declined_done, request.username)) }, style = ButtonStyle.Secondary, height = 44.dp)
                        IconAction(R.drawable.ic_approved_slash, stringResource(R.string.contacts_block), { confirming = request.username to "block" }, bordered = true, tint = Fb.palette.danger.ink)
                    }
                }
                if (outgoing.isNotEmpty()) item { SectionLabel(stringResource(R.string.contacts_sent)) }
                items(outgoing, key = { "out-${it.id}" }) { request ->
                    ContactCard(
                        request.username, stringResource(R.string.contacts_pending) to Tone.Neutral, confirming?.takeIf { it.first == request.username }?.second, Modifier.animateItem(placementSpec = Motion.spatial()),
                        onCancel = { confirming = null },
                        onConfirm = { act(request.username, "block", resources.getString(R.string.contacts_blocked_done, request.username)) },
                    ) {
                        FbButton(stringResource(R.string.contacts_cancel_request), { act(request.username, "cancel", resources.getString(R.string.contacts_cancelled_done, request.username)) }, Modifier.weight(1f), style = ButtonStyle.Secondary, height = 44.dp)
                        IconAction(R.drawable.ic_approved_slash, stringResource(R.string.contacts_block), { confirming = request.username to "block" }, bordered = true, tint = Fb.palette.danger.ink)
                    }
                }
            }
            ContactsTab.Blocked -> {
                if (loaded != null && blocked.isEmpty()) item { EmptyState(R.drawable.ic_approved_slash, stringResource(R.string.contacts_blocked_empty), tone = Tone.Neutral) }
                items(blocked, key = { "blocked-$it" }) { name ->
                    ContactCard(
                        name, stringResource(R.string.contacts_blocked_badge) to Tone.Danger, confirming?.takeIf { it.first == name }?.second, Modifier.animateItem(placementSpec = Motion.spatial()),
                        onCancel = { confirming = null },
                        onConfirm = { act(name, "unblock", resources.getString(R.string.contacts_unblocked_done, name)) },
                    ) {
                        FbButton(stringResource(R.string.contacts_unblock), { confirming = name to "unblock" }, Modifier.weight(1f), style = ButtonStyle.Secondary, height = 44.dp)
                    }
                }
            }
        }
        item { SectionLabel(stringResource(R.string.receiving_defaults)) }
        item {
            Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
                val policy = receivingPolicies.firstOrNull { it.first == loaded?.policy }?.second
                FbButton(
                    (policy?.let { stringResource(it) } ?: stringResource(R.string.receiving_defaults)) + if (loaded?.autoDownloadFriends == true) " · " + stringResource(R.string.contacts_auto_on) else "",
                    { defaultsOpen = !defaultsOpen }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary,
                    trailingIcon = if (defaultsOpen) R.drawable.ic_approved_chevron_up else R.drawable.ic_approved_chevron_down,
                )
                AnimatedVisibility(defaultsOpen, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.enter()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
                    ReceivingDefaultsPanel(loaded) { chosen, auto ->
                        scope.launch {
                            runCatching { accounts.receivingDefaults(chosen, auto) }.onFailure { error = it.message ?: generic }
                            revision++
                        }
                    }
                }
            }
        }
        if (!profile.usernameRoutingEnabled) item { Notice(stringResource(R.string.secure_inbox_unavailable), tone = Tone.Warning) }
    }

    preferences?.let { friend ->
        val latest = loaded?.contacts?.firstOrNull { it.id == friend.id } ?: friend
        FbSheet({ preferences = null }, title = stringResource(R.string.contacts_preferences_title, latest.username)) {
            val saved = stringResource(R.string.contacts_preferences_saved)
            FbText(stringResource(R.string.contacts_can_send), style = Fb.type.bodyStrong)
            listOf(null to R.string.contacts_use_default, true to R.string.contacts_allow, false to R.string.contacts_disallow).forEach { (value, label) ->
                ChoiceRow(stringResource(label), latest.canSend == value, { act(latest.username, "preferences", saved, value, latest.autoDownload) })
            }
            FbText(stringResource(R.string.contacts_effective, stringResource(if (latest.effectiveSend) R.string.contacts_allow else R.string.contacts_disallow)), style = Fb.type.caption, color = Fb.palette.textMuted)
            FbText(stringResource(R.string.contacts_auto_download), style = Fb.type.bodyStrong)
            listOf(null to R.string.contacts_use_default, true to R.string.toggle_on, false to R.string.toggle_off).forEach { (value, label) ->
                ChoiceRow(stringResource(label), latest.autoDownload == value, { act(latest.username, "preferences", saved, latest.canSend, value) })
            }
            FbText(stringResource(R.string.contacts_effective, stringResource(if (latest.effectiveDownload) R.string.toggle_on else R.string.toggle_off)), style = Fb.type.caption, color = Fb.palette.textMuted)
            FbButton(stringResource(R.string.contacts_reset), { act(latest.username, "preferences", saved, null, null) }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet, enabled = latest.canSend != null || latest.autoDownload != null)
        }
    }
}

/** One person with their actions. A destructive action swaps the action row for an inline confirmation. */
@Composable
private fun ContactCard(
    username: String,
    badge: Pair<String, Tone>?,
    confirming: String?,
    modifier: Modifier,
    onCancel: () -> Unit,
    onConfirm: (String) -> Unit,
    actions: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit,
) {
    Panel(modifier.fillMaxWidth(), shape = Shapes.card) {
        Column(Modifier.padding(Space.md), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                Avatar(username)
                FbText("@$username", Modifier.weight(1f), style = Fb.type.bodyStrong, maxLines = 1)
                if (badge != null) StatusPill(badge.first, tone = badge.second)
            }
            AnimatedContent(confirming, transitionSpec = { fadeIn(Motion.enter()) togetherWith fadeOut(Motion.exit()) }, label = "confirm") { action ->
                if (action != null) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        FbText(
                            stringResource(
                                when (action) {
                                    "block" -> R.string.contacts_confirm_block
                                    "unblock" -> R.string.contacts_confirm_unblock
                                    else -> R.string.contacts_confirm_remove
                                },
                                username,
                            ),
                            Modifier.weight(1f), style = Fb.type.smallStrong, color = Fb.palette.danger.ink,
                        )
                        FbButton(stringResource(R.string.cancel), onCancel, style = ButtonStyle.Quiet, height = 40.dp)
                        FbButton(stringResource(R.string.confirm), { onConfirm(action) }, style = ButtonStyle.Danger, height = 40.dp)
                    }
                } else {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm), content = actions)
                }
            }
        }
    }
}
