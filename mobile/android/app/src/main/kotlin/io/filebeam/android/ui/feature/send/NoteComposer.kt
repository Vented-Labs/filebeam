package io.filebeam.android.ui.feature.send

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.common.formatRetention
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbSwitch
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.design.code.CodeEditor
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.shell.InstanceProfile
import io.filebeam.android.ui.shell.Staggered

/**
 * The note surface shared by standalone and attached notes: a title, a language and wrap
 * bar, the editor, and a byte counter that turns red past [maximumBytes].
 */
@Composable
internal fun NoteFields(note: NoteDraft, maximumBytes: Long?, onChange: (NoteDraft) -> Unit, onPickLanguage: () -> Unit, modifier: Modifier = Modifier) {
    val palette = Fb.palette
    Column(modifier.fillMaxWidth().clip(Shapes.card).background(palette.surface).border(1.dp, palette.border, Shapes.card)) {
        Row(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.md), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            FbIcon(R.drawable.ic_approved_note, size = 20.dp)
            BasicTextField(
                note.title,
                { value -> if (value.codePointCount(0, value.length) <= NOTE_TITLE_MAX) onChange(note.copy(title = value.replace("\n", " "))) },
                Modifier.weight(1f),
                textStyle = Fb.type.bodyStrong.copy(color = palette.text),
                singleLine = true,
                cursorBrush = SolidColor(palette.accentText),
                decorationBox = { inner ->
                    if (note.title.isEmpty()) FbText(stringResource(R.string.note_title_placeholder), style = Fb.type.bodyStrong, color = palette.textSubtle, maxLines = 1)
                    inner()
                },
            )
        }
        Hairline()
        Row(Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(
                Modifier
                    .height(36.dp)
                    .clip(Shapes.control)
                    .border(1.dp, palette.controlBorder, Shapes.control)
                    .clickable(role = Role.DropdownList, onClickLabel = stringResource(R.string.note_language), onClick = onPickLanguage)
                    .padding(horizontal = Space.md),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Space.sm),
            ) {
                FbText(note.language.label, style = Fb.type.smallStrong, maxLines = 1)
                FbIcon(R.drawable.ic_approved_chevron_down, size = 14.dp)
            }
            FbText("untitled.${note.language.extension}", Modifier.weight(1f), style = Fb.type.codeSmall, color = palette.textMuted, maxLines = 1)
            FbText(stringResource(R.string.note_wrap), style = Fb.type.caption, color = palette.textMuted)
            FbSwitch(note.wrap, { onChange(note.copy(wrap = it)) })
        }
        CodeEditor(
            note.body, note.language.key,
            Modifier.padding(horizontal = Space.sm),
            onValueChange = { onChange(note.copy(body = it)) },
            wrap = note.wrap,
            placeholder = stringResource(R.string.note_placeholder),
        )
        val over = maximumBytes != null && note.bytes > maximumBytes
        val counter by animateColorAsState(if (over) palette.danger.ink else palette.textMuted, motionOr(Motion.fade()), label = "counter")
        FbText(
            if (maximumBytes != null) stringResource(R.string.note_counter, formatBytes(note.bytes.toLong()), formatBytes(maximumBytes)) else formatBytes(note.bytes.toLong()),
            Modifier.fillMaxWidth().padding(horizontal = Space.lg, vertical = Space.sm),
            style = Fb.type.numeric, color = counter, textAlign = TextAlign.End,
        )
    }
}

/** "Attach note" on the files composer: a switch that unfolds a note sharing the files' encryption and expiry. */
@Composable
internal fun AttachedNoteSection(note: NoteDraft?, onChange: (NoteDraft?) -> Unit) {
    val palette = Fb.palette
    var picking by rememberSaveable { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
        Panel(Modifier.fillMaxWidth()) {
            SwitchRow(
                stringResource(R.string.attach_note), note != null, { enabled -> onChange(if (enabled) NoteDraft() else null) },
                Modifier.padding(horizontal = Space.lg),
                description = stringResource(R.string.attach_note_description),
                icon = R.drawable.ic_approved_note,
            )
        }
        AnimatedVisibility(note != null, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.enter()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                NoteFields(note ?: NoteDraft(), ATTACHED_NOTE_MAX_BYTES.toLong(), onChange, { picking = true })
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FbIcon(R.drawable.ic_approved_lock, size = 14.dp)
                    FbText(stringResource(R.string.attach_note_footer), style = Fb.type.caption, color = palette.textMuted)
                }
            }
        }
    }
    if (picking && note != null) LanguageSheet(note.language, { picking = false }) { onChange(note.copy(language = it)) }
}

internal fun LazyListScope.notesComposer(
    state: SendUiState,
    profile: InstanceProfile,
    blocker: SendBlocker?,
    model: SendViewModel,
    onOpen: (OpenSheetRequest) -> Unit,
    onSend: () -> Unit,
) {
    val note = state.note
    val driver = note.driver ?: profile.defaultDriver
    item(key = "note-transport") {
        Staggered(1) {
            TransportRail(profile, driver, { selected -> model.updateNote { it.copy(driver = selected) } }, showFileCount = false, liveCaption = stringResource(R.string.transport_webrtc_caption))
        }
    }
    item(key = "note-editor") {
        Staggered(2) {
            NoteFields(note, profile.limits(driver).maximumNoteBytes, { updated -> model.updateNote { updated } }, { onOpen(OpenSheetRequest.Language) })
        }
    }
    item(key = "note-options") {
        Staggered(3) {
            val context = LocalContext.current
            Panel(Modifier.fillMaxWidth()) {
                OptionRow(
                    R.drawable.ic_approved_key, stringResource(R.string.option_password),
                    stringResource(if (note.password != null) R.string.option_password_set else R.string.option_password_none),
                    highlighted = note.password != null,
                ) { onOpen(OpenSheetRequest.Password) }
                Hairline()
                OptionRow(
                    R.drawable.ic_approved_clock,
                    stringResource(if (driver == "webrtc") R.string.option_link_lifetime else R.string.option_retention),
                    formatRetention(context, note.retentionHours ?: profile.noteRetention.defaultHours),
                ) { onOpen(OpenSheetRequest.Retention) }
                Hairline()
                SwitchRow(
                    stringResource(R.string.option_include_key), note.includeKey, { include -> model.updateNote { it.copy(includeKey = include) } },
                    Modifier.padding(horizontal = Space.lg),
                    description = if (note.includeKey) null else stringResource(R.string.option_include_key_off),
                    icon = R.drawable.ic_approved_link,
                )
                Hairline()
                SwitchRow(
                    stringResource(R.string.option_burn), note.burnOnRead, { burn -> model.updateNote { it.copy(burnOnRead = burn) } },
                    Modifier.padding(horizontal = Space.lg),
                    description = stringResource(if (driver == "webrtc") R.string.option_burn_live else R.string.option_burn_stored),
                    icon = R.drawable.ic_approved_trash,
                )
            }
        }
    }
    blockerMessage(blocker)?.let { (text, _) ->
        item(key = "note-blocker") { Notice(text(), Modifier.animateItem(), Tone.Danger) }
    }
    item(key = "note-actions") {
        Staggered(4) {
            Column(verticalArrangement = Arrangement.spacedBy(Space.md)) {
                FbButton(stringResource(R.string.send_note), onSend, Modifier.fillMaxWidth(), icon = R.drawable.ic_approved_lock, enabled = blocker == null, loading = state.noteBusy)
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) {
                    FbIcon(R.drawable.ic_approved_shield, size = 16.dp)
                    FbText(stringResource(R.string.send_footer), style = Fb.type.caption, color = Fb.palette.textMuted)
                }
            }
        }
    }
}

/** Which sheet the notes composer wants opened. */
internal enum class OpenSheetRequest { Password, Retention, Language }
