package io.filebeam.android.ui.feature.receive

import android.content.ClipboardManager
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Emblem
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.SmoothProgress
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.design.code.CodeEditor
import io.filebeam.android.ui.design.progressLabel
import io.filebeam.android.ui.feature.send.CliCard
import io.filebeam.android.ui.feature.send.copySensitive
import io.filebeam.android.ui.feature.send.fileIcon
import io.filebeam.android.ui.feature.send.phaseLabel
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.Incoming
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalHost
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.ScreenHeader
import io.filebeam.android.ui.shell.Staggered
import io.filebeam.android.ui.shell.Tab
import io.filebeam.android.ui.shell.TabScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.DownloadItem
import java.io.File

@Composable
fun ReceiveScreen(model: ReceiveViewModel = viewModel()) {
    val app = LocalAppModel.current
    val host = LocalHost.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val state by model.state.collectAsStateCompat()
    val transfer by model.transfer.collectAsStateCompat()
    val incoming by app.incoming.collectAsStateCompat()
    var reporting by rememberSaveable { mutableStateOf(false) }
    val stage = state.stage

    LaunchedEffect(incoming) {
        app.claim(Incoming.TransferLink::class.java)?.let { model.open(it.link) }
    }
    // Decrypted notes and key entry are kept out of screenshots and the recents preview.
    if (stage is ReceiveStage.Note || stage is ReceiveStage.NeedKey || stage is ReceiveStage.NeedPassword) SecureWindowEffect()

    TabScreen {
        item(key = "body") {
            AnimatedContent(
                stage,
                contentKey = { it::class },
                transitionSpec = {
                    (fadeIn(Motion.enter(Motion.PANE, 60)) + slideInVertically(Motion.spatial()) { it / 14 } + scaleIn(Motion.spatial(), initialScale = 0.98f)) togetherWith
                        (fadeOut(Motion.exit()) + scaleOut(Motion.exit(), targetScale = 0.99f))
                },
                label = "receive",
            ) { current ->
                Column(verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                    when (current) {
                        ReceiveStage.Idle -> LinkEntry(state.input, model::setInput, onOpen = { model.open() }) {
                            val clip = context.getSystemService(ClipboardManager::class.java).primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()
                            if (clip.isNullOrBlank()) app.toast(context.getString(R.string.receive_clipboard_empty)) else {
                                model.setInput(clip)
                                model.open(clip)
                            }
                        }
                        ReceiveStage.Inspecting -> Working(stringResource(R.string.receive_loading_title), stringResource(R.string.receive_loading_body))
                        is ReceiveStage.Unlocking -> Working(phaseLabel(current.phase), stringResource(R.string.receive_unlocking_body))
                        is ReceiveStage.NeedKey -> KeyStep(current.error, model::submitKey) { model.reset() }
                        is ReceiveStage.NeedPassword -> PasswordStep(current.error, model::submitPassword, onChangeKey = model::changeKey) { model.reset() }
                        is ReceiveStage.NeedConsent -> ConsentStep(current.peer, model::submitConsent)
                        is ReceiveStage.NoteLocked -> NoteLocked(current.burnOnRead, model::decryptNote) { model.reset() }
                        is ReceiveStage.Files -> FilesReady(current.items, state.live, state.attachedNote, onDownloadAll = { model.download(null) }, onDownloadOne = { model.download(setOf(it)) })
                        is ReceiveStage.Downloading -> DownloadProgress(current, transfer.phase, transfer.snapshot?.let { s -> s.total?.takeIf { it > 0uL }?.let { (s.done.toDouble() / it.toDouble()).toFloat() } }, state.live, model::cancelDownload)
                        is ReceiveStage.Saved -> SavedFiles(current, state.attachedNote, onSave = { file ->
                            host.createDocument(file.name, "application/octet-stream") { uri -> if (uri != null) model.save(file, uri) }
                        })
                        is ReceiveStage.Note -> NoteView(current, model::retryBurn)
                        is ReceiveStage.Unavailable -> Unavailable(current.message) {
                            model.reset()
                            navigator.select(Tab.Send)
                        }
                    }
                    if (current is ReceiveStage.Files || current is ReceiveStage.Saved || current is ReceiveStage.Note || current is ReceiveStage.NoteLocked) {
                        if (current is ReceiveStage.Files) {
                            CliCard(state.input, available = !state.live && state.input.contains('#'), reason = R.string.cli_unavailable_live)
                        }
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                            FbButton(stringResource(R.string.receive_another), { model.reset() }, Modifier.weight(1f), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_link)
                            FbButton(stringResource(R.string.report_action), { reporting = true }, Modifier.weight(1f), style = ButtonStyle.Quiet, icon = R.drawable.ic_approved_alert)
                        }
                    }
                }
            }
        }
    }
    if (reporting) ReportSheet(state, onDismiss = {
        reporting = false
        model.clearReport()
    }, onSubmit = model::report)
}

@Composable
private fun LinkEntry(input: String, onInput: (String) -> Unit, onOpen: () -> Unit, onPaste: () -> Unit) {
    Staggered(0) { ScreenHeader(stringResource(R.string.receive_title), subtitle = stringResource(R.string.receive_subtitle)) }
    Staggered(1) {
        Panel(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                FbTextField(
                    input, onInput,
                    label = stringResource(R.string.receive_link_label), placeholder = stringResource(R.string.receive_link_placeholder),
                    leadingIcon = R.drawable.ic_approved_link,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { onOpen() }),
                )
                FbButton(stringResource(R.string.receive_open), onOpen, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_unlock, enabled = input.isNotBlank())
                FbButton(stringResource(R.string.receive_paste), onPaste, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_copy)
            }
        }
    }
    Staggered(2) {
        Row(Modifier.fillMaxWidth().padding(top = Space.sm), horizontalArrangement = Arrangement.spacedBy(Space.sm, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
            FbIcon(R.drawable.ic_approved_shield, size = 16.dp)
            FbText(stringResource(R.string.receive_footer), style = Fb.type.caption, color = Fb.palette.textMuted)
        }
    }
}

/** A calm holding state with the emblem, used while the app talks to the instance. */
@Composable
private fun Working(title: String, body: String) {
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.fillMaxWidth().padding(Space.xxl), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
            Emblem(engaged = true, size = 108.dp)
            FbText(title, style = Fb.type.headline, textAlign = TextAlign.Center)
            FbText(body, style = Fb.type.small, color = Fb.palette.textMuted, textAlign = TextAlign.Center)
            SmoothProgress(null)
        }
    }
}

@Composable
private fun StepPanel(icon: Int, title: String, body: String, content: @Composable () -> Unit) {
    val palette = Fb.palette
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.fillMaxWidth().padding(Space.xl), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
            Box(Modifier.size(56.dp).clip(Shapes.card).background(palette.accentSurface).border(1.dp, palette.choiceBorder.copy(alpha = 0.5f), Shapes.card), contentAlignment = Alignment.Center) {
                FbIcon(icon, tint = palette.accentText, size = 26.dp)
            }
            FbText(title, style = Fb.type.headline)
            FbText(body, style = Fb.type.small, color = palette.textMuted)
            content()
        }
    }
}

@Composable
private fun KeyStep(error: Boolean, onSubmit: (String) -> Unit, onCancel: () -> Unit) {
    var key by remember { mutableStateOf("") }
    StepPanel(R.drawable.ic_approved_key, stringResource(R.string.receive_key_title), stringResource(R.string.receive_key_body)) {
        FbTextField(
            key, { key = it.trim() },
            label = stringResource(R.string.receive_key_label), placeholder = "v1.…",
            error = if (error) stringResource(R.string.receive_key_invalid) else null,
            textStyle = Fb.type.code,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(onGo = { onSubmit(key) }),
        )
        FbButton(stringResource(R.string.continue_action), { onSubmit(key) }, Modifier.fillMaxWidth(), enabled = key.isNotBlank())
        FbButton(stringResource(R.string.cancel), onCancel, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
    }
}

@Composable
private fun PasswordStep(error: Boolean, onSubmit: (String) -> Unit, onChangeKey: () -> Unit, onCancel: () -> Unit) {
    var password by remember { mutableStateOf("") }
    StepPanel(R.drawable.ic_approved_lock, stringResource(R.string.receive_password_title), stringResource(R.string.receive_password_body)) {
        PasswordField(
            password, { password = it },
            label = stringResource(R.string.option_password),
            error = if (error) stringResource(R.string.receive_password_invalid) else null,
            showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(onGo = { onSubmit(password) }),
        )
        FbButton(stringResource(R.string.receive_unlock), { onSubmit(password) }, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_unlock, enabled = password.isNotEmpty())
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            FbButton(stringResource(R.string.receive_change_key), onChangeKey, Modifier.weight(1f), style = ButtonStyle.Quiet)
            FbButton(stringResource(R.string.cancel), onCancel, Modifier.weight(1f), style = ButtonStyle.Quiet)
        }
    }
}

@Composable
private fun ConsentStep(peer: String?, onAnswer: (Boolean) -> Unit) {
    StepPanel(R.drawable.ic_custom_webrtc_p2p, stringResource(R.string.consent_title), stringResource(R.string.consent_body)) {
        if (peer != null) FbText(peer, style = Fb.type.codeSmall, color = Fb.palette.textMuted)
        Notice(stringResource(R.string.consent_relay_hint), tone = Tone.Info)
        FbButton(stringResource(R.string.consent_allow), { onAnswer(true) }, Modifier.fillMaxWidth())
        FbButton(stringResource(R.string.consent_deny), { onAnswer(false) }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
    }
}

@Composable
private fun NoteLocked(burnOnRead: Boolean, onDecrypt: () -> Unit, onCancel: () -> Unit) {
    StepPanel(R.drawable.ic_approved_note, stringResource(R.string.receive_note_title), stringResource(R.string.receive_note_body)) {
        if (burnOnRead) Notice(stringResource(R.string.receive_note_burn_warning), tone = Tone.Warning, icon = R.drawable.ic_approved_trash)
        FbButton(stringResource(R.string.receive_note_decrypt), onDecrypt, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_unlock)
        FbButton(stringResource(R.string.cancel), onCancel, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
    }
}

@Composable
private fun ItemRow(name: String, detail: String, trailing: @Composable () -> Unit) {
    val palette = Fb.palette
    Row(
        Modifier.fillMaxWidth().clip(Shapes.card).background(palette.surface).border(1.dp, palette.border, Shapes.card).padding(start = Space.md, top = Space.md, bottom = Space.md, end = Space.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(Modifier.size(44.dp).clip(Shapes.control).background(palette.accentSurface).border(1.dp, palette.border, Shapes.control), contentAlignment = Alignment.Center) {
            FbIcon(fileIcon(name, null), tint = palette.accentText, size = 22.dp)
        }
        Column(Modifier.weight(1f)) {
            FbText(name, style = Fb.type.bodyStrong, maxLines = 1)
            FbText(detail, style = Fb.type.small, color = palette.textMuted, maxLines = 1)
        }
        trailing()
    }
}

@Composable
private fun AttachedNoteCard(note: io.filebeam.rust.AttachedNote) {
    val context = LocalContext.current
    val app = LocalAppModel.current
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                FbIcon(R.drawable.ic_approved_note, size = 20.dp)
                FbText(note.title ?: stringResource(R.string.receive_attached_note), Modifier.weight(1f), style = Fb.type.bodyStrong, maxLines = 1)
                IconAction(R.drawable.ic_approved_copy, stringResource(R.string.copy), {
                    copySensitive(context, context.getString(R.string.receive_attached_note), note.text)
                    app.toast(context.getString(R.string.copied), Tone.Success)
                })
            }
            CodeEditor(note.text, note.language, minHeight = 96.dp, maxHeight = 260.dp)
        }
    }
}

@Composable
private fun FilesReady(items: List<DownloadItem>, live: Boolean, note: io.filebeam.rust.AttachedNote?, onDownloadAll: () -> Unit, onDownloadOne: (String) -> Unit) {
    val total = items.sumOf { it.size.toLong() }
    ScreenHeader(
        pluralStringResource(R.plurals.receive_files_ready, items.size, items.size),
        subtitle = formatBytes(total) + if (live) " · " + stringResource(R.string.receive_live_hint) else "",
    )
    if (note != null) AttachedNoteCard(note)
    items.forEachIndexed { index, item ->
        Staggered(index.coerceAtMost(8)) {
            ItemRow(item.name, formatBytes(item.size.toLong())) {
                if (items.size > 1) IconAction(R.drawable.ic_approved_download, stringResource(R.string.receive_download_named, item.name), { onDownloadOne(item.id) }, tint = Fb.palette.accentText)
            }
        }
    }
    FbButton(
        if (items.size > 1) stringResource(R.string.receive_download_all) else stringResource(R.string.receive_download),
        onDownloadAll, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_download,
    )
    FbText(stringResource(R.string.receive_progress_shared), Modifier.fillMaxWidth(), style = Fb.type.caption, color = Fb.palette.textSubtle, textAlign = TextAlign.Center)
}

@Composable
private fun DownloadProgress(stage: ReceiveStage.Downloading, phase: String, fraction: Float?, live: Boolean, onCancel: () -> Unit) {
    val palette = Fb.palette
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.fillMaxWidth().padding(Space.xl), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
            Emblem(engaged = true, size = 108.dp)
            AnimatedContent(phaseLabel(phase), transitionSpec = { fadeIn(Motion.enter()) togetherWith fadeOut(Motion.exit()) }, label = "phase") { label ->
                FbText(label, Modifier.fillMaxWidth(), style = Fb.type.headline, textAlign = TextAlign.Center)
            }
            val count = stage.selection?.size ?: stage.items.size
            FbText(pluralStringResource(R.plurals.receive_downloading_count, count, count), style = Fb.type.small, color = palette.textMuted)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Spinner(size = 16.dp)
                FbText(if (live) stringResource(R.string.receive_live_hint) else stringResource(R.string.receive_verifying_hint), Modifier.weight(1f).padding(start = Space.sm), style = Fb.type.caption, color = palette.textMuted, maxLines = 1)
                if (fraction != null) FbText(progressLabel(fraction), style = Fb.type.numeric, color = palette.textMuted)
            }
            SmoothProgress(fraction)
            FbButton(stringResource(R.string.cancel), onCancel, Modifier.fillMaxWidth(), style = ButtonStyle.Danger)
        }
    }
}

@Composable
private fun SavedFiles(stage: ReceiveStage.Saved, note: io.filebeam.rust.AttachedNote?, onSave: (File) -> Unit) {
    ScreenHeader(stringResource(R.string.receive_done_title), subtitle = stringResource(R.string.receive_done_body))
    if (note != null) AttachedNoteCard(note)
    stage.files.forEachIndexed { index, file ->
        Staggered(index.coerceAtMost(8)) {
            ItemRow(file.name, formatBytes(file.length())) {
                FbButton(stringResource(R.string.receive_save), { onSave(file) }, style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_download, height = 40.dp)
            }
        }
    }
    if (stage.files.isEmpty()) Notice(stringResource(R.string.receive_done_missing), tone = Tone.Warning)
    StatusPill(stringResource(R.string.receive_verified), tone = Tone.Success, icon = R.drawable.ic_approved_shield)
}

@Composable
private fun NoteView(stage: ReceiveStage.Note, onRetryBurn: () -> Unit) {
    val context = LocalContext.current
    val app = LocalAppModel.current
    val host = LocalHost.current
    val note = stage.note
    ScreenHeader(note.title?.takeIf(String::isNotBlank) ?: stringResource(R.string.receive_note_default_title))
    if (note.consumed) Notice(stringResource(R.string.receive_note_burned), tone = Tone.Warning, icon = R.drawable.ic_approved_trash)
    if (stage.burnPending) {
        Notice(stringResource(R.string.receive_note_burn_pending), tone = Tone.Danger) {
            FbButton(stringResource(R.string.receive_note_retry_burn), onRetryBurn, style = ButtonStyle.Secondary, height = 40.dp)
        }
    }
    CodeEditor(note.text, note.language, maxHeight = 520.dp)
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
        FbButton(stringResource(R.string.copy), {
            copySensitive(context, context.getString(R.string.receive_note_default_title), note.text)
            app.toast(context.getString(R.string.copied), Tone.Success)
        }, Modifier.weight(1f), icon = R.drawable.ic_approved_copy)
        FbButton(stringResource(R.string.receive_save), {
            host.createDocument("note.txt", "text/plain") { uri ->
                if (uri != null) runCatching { context.contentResolver.openOutputStream(uri, "wt")?.use { it.write(note.text.toByteArray()) } }
                    .onSuccess { app.toast(context.getString(R.string.saved_file), Tone.Success) }
                    .onFailure { app.toast(it.message ?: context.getString(R.string.unknown_error), Tone.Danger) }
            }
        }, Modifier.weight(1f), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_download)
    }
}

@Composable
private fun Unavailable(message: String?, onStartNew: () -> Unit) {
    val palette = Fb.palette
    Panel(Modifier.fillMaxWidth()) {
        Column(Modifier.fillMaxWidth().padding(Space.xxl), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.lg)) {
            Box(Modifier.size(72.dp).clip(Shapes.panel).background(palette.danger.surface).border(1.dp, palette.danger.border.copy(alpha = 0.5f), Shapes.panel), contentAlignment = Alignment.Center) {
                FbIcon(R.drawable.ic_approved_alert, tint = palette.danger.ink, size = 30.dp)
            }
            FbText(stringResource(R.string.receive_unavailable_title), style = Fb.type.headline, textAlign = TextAlign.Center)
            FbText(message ?: stringResource(R.string.receive_unavailable_body), style = Fb.type.small, color = palette.textMuted, textAlign = TextAlign.Center)
            FbButton(stringResource(R.string.receive_start_new), onStartNew, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_upload)
        }
    }
}
