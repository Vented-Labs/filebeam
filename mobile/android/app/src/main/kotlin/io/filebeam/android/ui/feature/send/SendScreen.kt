package io.filebeam.android.ui.feature.send

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.common.formatRetention
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Emblem
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Segment
import io.filebeam.android.ui.design.SegmentedRail
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.Incoming
import io.filebeam.android.ui.shell.InstanceProfile
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalHost
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.TabScreen
import io.filebeam.android.ui.shell.collectAsStateCompat

private enum class OpenSheet { None, Password, Retention, Recipient, NotePassword, NoteRetention, Language }

@Composable
fun SendScreen(model: SendViewModel = viewModel()) {
    val app = LocalAppModel.current
    val host = LocalHost.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val state by model.state.collectAsStateCompat()
    val transfer by model.transfer.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    val account by app.account.collectAsStateCompat()
    val settings by app.settings.collectAsStateCompat()
    val incoming by app.incoming.collectAsStateCompat()
    var sheet by rememberSaveable { mutableStateOf(OpenSheet.None) }
    val signedIn = account != null
    val fileDriver = state.files.driver ?: profile.defaultDriver
    val fileLimits = profile.limits(fileDriver)

    val duplicateMessage: (Int) -> String = { resources.getQuantityString(R.plurals.send_duplicates_skipped, it, it) }
    val limitMessage: (Int, Long) -> String = { room, maximum -> resources.getQuantityString(R.plurals.send_limit_reached, room, room, maximum) }
    val addFiles = { host.pickFiles { uris -> model.addFiles(uris, emptyMap(), fileLimits.maximumFileCount, duplicateMessage, limitMessage) } }
    val addFolder = {
        host.pickFolder { tree ->
            if (tree != null) model.addFolder(tree, fileLimits.maximumFileCount, duplicateMessage, limitMessage) { app.toast(it ?: resources.getString(R.string.send_folder_failed), Tone.Danger) }
        }
    }

    // Files and text shared into the app land here once.
    LaunchedEffect(incoming) {
        app.claim(Incoming.Files::class.java)?.let { model.addFiles(it.uris, emptyMap(), fileLimits.maximumFileCount, duplicateMessage, limitMessage) }
        app.claim(Incoming.Recipient::class.java)?.let {
            model.setMode(SendMode.Files)
            model.chooseRecipient(it.username)
        }
        app.claim(Incoming.Text::class.java)?.let { shared ->
            model.setMode(SendMode.Notes)
            model.updateNote { it.copy(body = if (it.body.isEmpty()) shared.text else it.body + "\n" + shared.text) }
        }
    }
    LaunchedEffect(state.error) {
        state.error?.let {
            app.toast(it, Tone.Danger)
            model.clearError()
        }
    }

    val fileBlocker = model.fileBlocker(profile, signedIn, state)
    val noteBlocker = model.noteBlocker(profile, signedIn, state)
    val startFiles: (Boolean) -> Unit = { turbo -> host.withNotifications { model.sendFiles(profile, turbo) } }
    val startNote = { host.withNotifications { model.sendNote(profile, settings.instance) } }

    AnimatedContent(
        state.stage is SendStage.Sharing,
        transitionSpec = {
            (fadeIn(Motion.enter(Motion.PANE, 80)) + slideInVertically(Motion.spatialSlow()) { it / 12 } + scaleIn(Motion.spatialSlow(), initialScale = 0.96f)) togetherWith
                (fadeOut(Motion.exit()) + scaleOut(Motion.exit(), targetScale = 0.98f))
        },
        label = "send-stage",
    ) { sharing ->
        if (sharing) {
            ShareView(model, state, transfer, profile)
        } else {
            TabScreen(
                Modifier.onPreviewKeyEvent { event ->
                    // Hardware keyboards: Enter sends, Shift+Enter starts a Turbo Transfer.
                    if (state.mode == SendMode.Files && event.type == KeyEventType.KeyUp && event.key == Key.Enter && fileBlocker == null && sheet == OpenSheet.None) {
                        startFiles(event.isShiftPressed && fileDriver == "http" && state.files.recipient == null)
                        true
                    } else false
                },
            ) {
                item(key = "mode") {
                    Staggered(0) {
                        SegmentedRail(
                            listOf(
                                Segment(SendMode.Files, stringResource(R.string.send_mode_files), R.drawable.ic_approved_folder),
                                Segment(SendMode.Notes, stringResource(R.string.send_mode_notes), R.drawable.ic_approved_note),
                            ),
                            state.mode, model::setMode,
                        )
                    }
                }
                if (!profile.anonymousUploads && !signedIn && profile.loaded) {
                    item(key = "gate") { Staggered(1) { SignInGate { navigator.push(Route.Auth()) } } }
                } else if (state.mode == SendMode.Files) {
                    filesComposer(
                        state, profile, fileDriver, fileBlocker, signedIn, model,
                        onChooseFiles = addFiles, onChooseFolder = addFolder,
                        onOpen = { sheet = it }, onSend = startFiles,
                    )
                } else {
                    notesComposer(state, profile, noteBlocker, model, onSend = startNote, onOpen = { request ->
                        sheet = when (request) {
                            OpenSheetRequest.Password -> OpenSheet.NotePassword
                            OpenSheetRequest.Retention -> OpenSheet.NoteRetention
                            OpenSheetRequest.Language -> OpenSheet.Language
                        }
                    })
                }
            }
        }
    }

    when (sheet) {
        OpenSheet.Password -> PasswordSheet(state.files.password, { sheet = OpenSheet.None }, model::setFilePassword)
        OpenSheet.NotePassword -> PasswordSheet(state.note.password, { sheet = OpenSheet.None }) { password -> model.updateNote { it.copy(password = password?.takeIf(String::isNotEmpty)) } }
        OpenSheet.Retention -> RetentionSheet(
            profile.fileRetention.options, state.files.retentionHours ?: profile.fileRetention.defaultHours,
            live = fileDriver == "webrtc", onDismiss = { sheet = OpenSheet.None }, onSelect = model::setFileRetention,
        )
        OpenSheet.NoteRetention -> RetentionSheet(
            profile.noteRetention.options, state.note.retentionHours ?: profile.noteRetention.defaultHours,
            live = (state.note.driver ?: profile.defaultDriver) == "webrtc", onDismiss = { sheet = OpenSheet.None },
        ) { hours -> model.updateNote { it.copy(retentionHours = hours) } }
        OpenSheet.Recipient -> RecipientSheet(state, { sheet = OpenSheet.None }, model)
        OpenSheet.Language -> LanguageSheet(state.note.language, { sheet = OpenSheet.None }) { language -> model.updateNote { it.copy(language = language) } }
        OpenSheet.None -> Unit
    }
}

@Composable
private fun SignInGate(onSignIn: () -> Unit) {
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.fillMaxWidth().padding(Space.xxl), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
            Emblem()
            FbText(stringResource(R.string.send_gate_title), style = Fb.type.headline, textAlign = TextAlign.Center)
            FbText(stringResource(R.string.send_gate_body), style = Fb.type.body, color = Fb.palette.textMuted, textAlign = TextAlign.Center)
            FbButton(stringResource(R.string.auth_sign_in), onSignIn, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_login)
        }
    }
}

/** The transport choice with the limits that apply to it. */
@Composable
internal fun TransportRail(profile: InstanceProfile, driver: String, onSelect: (String) -> Unit, showFileCount: Boolean, liveCaption: String) {
    val limits = profile.limits(driver)
    Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
        SegmentedRail(
            listOf(
                Segment("http", stringResource(R.string.transport_http), R.drawable.ic_custom_http_server, stringResource(R.string.transport_http_caption), profile.httpEnabled),
                Segment("webrtc", stringResource(R.string.transport_webrtc), R.drawable.ic_custom_webrtc_p2p, liveCaption, profile.webrtcEnabled),
            ),
            driver, onSelect,
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
            FbIcon(R.drawable.ic_approved_storage, size = 18.dp)
            val size = if (showFileCount) limits.maximumTransferBytes else limits.maximumNoteBytes
            FbText(
                size?.let { stringResource(if (showFileCount) R.string.limit_per_transfer else R.string.limit_per_note, formatBytes(it)) } ?: stringResource(R.string.limit_unlimited),
                Modifier.weight(1f), style = Fb.type.smallStrong,
            )
            if (showFileCount) limits.maximumFileCount?.let {
                FbText(pluralStringResource(R.plurals.limit_file_count, it.toInt(), it.toInt()), style = Fb.type.small, color = Fb.palette.textMuted)
            }
        }
    }
}

private fun LazyListScope.filesComposer(
    state: SendUiState,
    profile: InstanceProfile,
    driver: String,
    blocker: SendBlocker?,
    signedIn: Boolean,
    model: SendViewModel,
    onChooseFiles: () -> Unit,
    onChooseFolder: () -> Unit,
    onOpen: (OpenSheet) -> Unit,
    onSend: (Boolean) -> Unit,
) {
    val draft = state.files
    item(key = "transport") {
        Staggered(1) {
            TransportRail(profile, driver, model::setFileDriver, showFileCount = true, liveCaption = stringResource(R.string.transport_webrtc_caption))
        }
    }
    if (draft.sources.isEmpty()) {
        item(key = "hero") { Staggered(2) { FilesHero(onChooseFiles, onChooseFolder) } }
    } else {
        item(key = "queue-header") {
            Row(Modifier.fillMaxWidth().animateItem(), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    FbText(stringResource(R.string.send_queue_title), style = Fb.type.title)
                    FbText(
                        pluralStringResource(R.plurals.send_queue_ready, draft.sources.size, draft.sources.size) + if (draft.sizeKnown) " · " + formatBytes(draft.knownBytes) else "",
                        style = Fb.type.small, color = Fb.palette.textMuted,
                    )
                }
                FbButton(stringResource(R.string.send_add_more), onChooseFiles, style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_plus, height = 44.dp)
            }
        }
        items(draft.sources, key = { it.identity }) { source ->
            FileRow(source, Modifier.animateItem(fadeInSpec = Motion.enter(), placementSpec = Motion.spatial(), fadeOutSpec = Motion.exit())) { model.removeSource(source.identity) }
        }
        state.queueMessage?.let { message ->
            item(key = "queue-message") { Notice(message, Modifier.animateItem(), Tone.Info) }
        }
    }
    item(key = "attach") {
        Staggered(3) {
            AttachedNoteSection(draft.attachedNote, model::setAttachedNote)
        }
    }
    item(key = "options") {
        Staggered(4) {
            val context = LocalContext.current
            val resources = LocalResources.current
            Panel(Modifier.fillMaxWidth()) {
                val friend = draft.recipient
                OptionRow(
                    R.drawable.ic_approved_key, stringResource(R.string.option_password),
                    when {
                        friend != null -> stringResource(R.string.option_unavailable_friend)
                        draft.password != null -> stringResource(R.string.option_password_set)
                        else -> stringResource(R.string.option_password_none)
                    },
                    enabled = friend == null, highlighted = draft.password != null,
                ) { onOpen(OpenSheet.Password) }
                Hairline()
                OptionRow(
                    R.drawable.ic_approved_clock,
                    stringResource(if (driver == "webrtc") R.string.option_link_lifetime else R.string.option_retention),
                    formatRetention(context, draft.retentionHours ?: profile.fileRetention.defaultHours),
                ) { onOpen(OpenSheet.Retention) }
                if (signedIn) {
                    Hairline()
                    OptionRow(
                        R.drawable.ic_approved_users, stringResource(R.string.option_friend),
                        friend?.let { stringResource(R.string.option_friend_selected, it.username) } ?: stringResource(R.string.option_friend_none),
                        highlighted = friend != null,
                    ) { onOpen(OpenSheet.Recipient) }
                }
                AnimatedVisibility(friend == null, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.fade()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
                    Column {
                        Hairline()
                        SwitchRow(
                            stringResource(R.string.option_include_key), draft.includeKey, model::setFileIncludeKey,
                            Modifier.padding(horizontal = Space.lg),
                            description = if (draft.includeKey) null else stringResource(R.string.option_include_key_off),
                            icon = R.drawable.ic_approved_link,
                        )
                    }
                }
                AnimatedVisibility(draft.sources.size > 1, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.fade()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
                    Column {
                        Hairline()
                        SwitchRow(
                            stringResource(R.string.option_zip), draft.archive, model::setArchive,
                            Modifier.padding(horizontal = Space.lg), description = stringResource(R.string.option_zip_description), icon = R.drawable.ic_approved_archive,
                        )
                    }
                }
            }
        }
    }
    blockerMessage(blocker)?.let { (text, useWebRtc) ->
        item(key = "blocker") {
            Notice(text(), Modifier.animateItem(), Tone.Danger) {
                if (useWebRtc) FbButton(stringResource(R.string.send_use_webrtc), { model.setFileDriver("webrtc") }, style = ButtonStyle.Secondary, height = 40.dp)
            }
        }
    }
    item(key = "actions") {
        Staggered(5) {
            Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
                val canTurbo = driver == "http" && draft.recipient == null
                FbButton(
                    stringResource(if (draft.recipient != null) R.string.send_to_inbox else R.string.send_encrypted), { onSend(false) }, Modifier.fillMaxWidth(),
                    icon = R.drawable.ic_approved_lock, enabled = blocker == null,
                )
                AnimatedVisibility(canTurbo, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.fade()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
                    FbButton(
                        stringResource(R.string.send_turbo), { onSend(true) }, Modifier.fillMaxWidth(),
                        style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_bolt, enabled = blocker == null,
                    )
                }
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
                    FbIcon(R.drawable.ic_approved_shield, size = 16.dp)
                    FbText(
                        if (canTurbo) stringResource(R.string.send_footer_turbo) else stringResource(R.string.send_footer),
                        style = Fb.type.caption, color = Fb.palette.textMuted, textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }
}

/** Maps a blocker to its message and whether live sending would get around it. Quiet blockers return null. */
internal fun blockerMessage(blocker: SendBlocker?): Pair<@Composable () -> String, Boolean>? = when (blocker) {
    null, SendBlocker.NothingSelected, SendBlocker.LoadingPolicy, SendBlocker.SignInRequired -> null
    is SendBlocker.PolicyUnavailable -> Pair({ blocker.detail ?: stringResource(R.string.send_policy_unavailable) }, false)
    SendBlocker.TransportDisabled -> Pair({ stringResource(R.string.send_transport_disabled) }, false)
    is SendBlocker.TooManyFiles -> Pair({ pluralStringResource(R.plurals.send_too_many_files, blocker.maximum.toInt(), blocker.maximum.toInt()) }, false)
    is SendBlocker.TooLarge -> Pair({ stringResource(R.string.send_too_large, formatBytes(blocker.limitBytes)) }, blocker.webrtcFits)
    is SendBlocker.NoteTooLarge -> Pair({ stringResource(R.string.send_note_too_large, formatBytes(blocker.limitBytes)) }, false)
    SendBlocker.AttachedNoteInvalid -> Pair({ stringResource(R.string.send_attached_note_invalid) }, false)
    SendBlocker.PasswordTooShort -> Pair({ stringResource(R.string.password_length_rule) }, false)
    SendBlocker.SourceError -> Pair({ stringResource(R.string.send_source_error) }, false)
}

@Composable
private fun FilesHero(onChooseFiles: () -> Unit, onChooseFolder: () -> Unit) {
    val palette = Fb.palette
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    Panel(Modifier.fillMaxWidth()) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.xxl),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            Emblem(Modifier.clip(Shapes.panel).clickable(interaction, indication = null, role = Role.Button, onClickLabel = stringResource(R.string.send_choose_files), onClick = onChooseFiles), engaged = pressed)
            androidx.compose.material3.Text(
                buildAnnotatedString {
                    append(stringResource(R.string.send_hero_lead))
                    append(' ')
                    withStyle(SpanStyle(color = palette.accentText)) { append(stringResource(R.string.send_hero_accent)) }
                },
                style = Fb.type.display, color = palette.text, textAlign = TextAlign.Center,
            )
            FbText(stringResource(R.string.send_hero_body), style = Fb.type.body, color = palette.textMuted, textAlign = TextAlign.Center)
            FbButton(stringResource(R.string.send_choose_files), onChooseFiles, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_folder, trailingIcon = R.drawable.ic_approved_arrow_up)
            FbButton(stringResource(R.string.send_choose_folder), onChooseFolder, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_archive)
        }
    }
}

@Composable
private fun FileRow(source: SelectedSource, modifier: Modifier = Modifier, onRemove: () -> Unit) {
    val palette = Fb.palette
    Row(
        modifier
            .fillMaxWidth()
            .clip(Shapes.card)
            .background(palette.surface)
            .border(1.dp, if (source.error != null) palette.danger.border else palette.border, Shapes.card)
            .padding(start = Space.md, top = Space.md, bottom = Space.md, end = Space.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(Modifier.size(44.dp).clip(Shapes.control).background(palette.accentSurface).border(1.dp, palette.border, Shapes.control), contentAlignment = Alignment.Center) {
            FbIcon(fileIcon(source.name, source.mimeType), tint = palette.accentText, size = 22.dp)
        }
        Column(Modifier.weight(1f)) {
            FbText(source.name, style = Fb.type.bodyStrong, maxLines = 1)
            val detail = listOfNotNull(source.sizeBytes?.let(::formatBytes), source.relativePath?.substringBeforeLast('/', "")?.takeIf(String::isNotEmpty)).joinToString(" · ")
            FbText(
                source.error?.let { stringResource(R.string.send_source_unavailable) } ?: detail.ifEmpty { stringResource(R.string.send_size_unknown) },
                style = Fb.type.small, color = if (source.error != null) palette.danger.ink else palette.textMuted, maxLines = 1,
            )
        }
        IconAction(R.drawable.ic_approved_x, stringResource(R.string.send_remove_file, source.name), onRemove)
    }
}

/** A tappable settings line: icon, title, and the current value on the trailing side. */
@Composable
internal fun OptionRow(icon: Int, title: String, value: String, enabled: Boolean = true, highlighted: Boolean = false, onClick: () -> Unit) {
    val palette = Fb.palette
    Row(
        Modifier.fillMaxWidth().clickable(enabled = enabled, role = Role.Button, onClick = onClick).padding(horizontal = Space.lg, vertical = Space.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        FbIcon(icon, tint = if (!enabled) palette.disabledText else if (highlighted) palette.accentText else palette.textMuted)
        FbText(title, Modifier.weight(1f), style = Fb.type.bodyStrong, color = if (enabled) palette.text else palette.disabledText, maxLines = 1)
        FbText(value, style = Fb.type.small, color = if (!enabled) palette.disabledText else if (highlighted) palette.accentText else palette.textMuted, maxLines = 1)
        FbIcon(R.drawable.ic_approved_arrow_right, tint = if (enabled) palette.textSubtle else palette.disabledText, size = 16.dp)
    }
}
