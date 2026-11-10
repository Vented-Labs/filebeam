package io.filebeam.android.ui.feature.send

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.platform.services.Contact
import io.filebeam.android.ui.common.formatRetention
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbSheet
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.motion.rememberHaptics
import io.filebeam.android.ui.shell.LocalAppModel

/** Sets, generates or removes the optional transfer password. The screen is kept out of screenshots. */
@Composable
internal fun PasswordSheet(current: String?, onDismiss: () -> Unit, onChange: (String?) -> Unit) {
    SecureWindowEffect()
    var value by remember { mutableStateOf(current.orEmpty()) }
    val tooShort = value.isNotEmpty() && value.length < PASSWORD_MIN
    FbSheet(onDismiss, title = stringResource(R.string.password_title), subtitle = stringResource(R.string.password_subtitle)) {
        PasswordField(
            value, { if (it.length <= PASSWORD_MAX) value = it },
            placeholder = stringResource(R.string.password_placeholder),
            error = if (tooShort) stringResource(R.string.password_length_rule) else null,
            showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = {
                if (!tooShort) {
                    onChange(value)
                    onDismiss()
                }
            }),
        )
        FbButton(stringResource(R.string.password_generate), { value = SendViewModel.generatePassword() }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_bolt)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.md)) {
            if (current != null) {
                FbButton(stringResource(R.string.password_remove), {
                    onChange(null)
                    onDismiss()
                }, Modifier.weight(1f), style = ButtonStyle.Danger)
            }
            FbButton(stringResource(R.string.done), {
                onChange(value)
                onDismiss()
            }, Modifier.weight(1f), enabled = !tooShort)
        }
    }
}

/** A single-choice list row with a radio-style marker. */
@Composable
internal fun ChoiceRow(title: String, selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier, description: String? = null, icon: Int? = null) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    Row(
        modifier
            .fillMaxWidth()
            .clip(Shapes.control)
            .background(if (selected) palette.selectedSurface else Color.Transparent)
            .border(1.dp, if (selected) palette.choiceBorder else palette.border, Shapes.control)
            .clickable(role = Role.RadioButton) {
                haptics.tick()
                onClick()
            }
            .padding(horizontal = Space.lg, vertical = Space.md),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        if (icon != null) FbIcon(icon, tint = if (selected) palette.accentText else palette.textMuted)
        Column(Modifier.weight(1f)) {
            FbText(title, style = Fb.type.bodyStrong)
            if (description != null) FbText(description, style = Fb.type.small, color = palette.textMuted)
        }
        Box(Modifier.size(22.dp).clip(Shapes.pill).border(1.5.dp, if (selected) palette.accentText else palette.controlBorder, Shapes.pill), contentAlignment = Alignment.Center) {
            if (selected) Box(Modifier.size(11.dp).clip(Shapes.pill).background(palette.accentText))
        }
    }
}

@Composable
internal fun RetentionSheet(options: List<Long>, selected: Long, live: Boolean, onDismiss: () -> Unit, onSelect: (Long) -> Unit) {
    val context = LocalContext.current
    FbSheet(
        onDismiss,
        title = stringResource(if (live) R.string.option_link_lifetime else R.string.option_retention),
        subtitle = stringResource(if (live) R.string.retention_live_hint else R.string.retention_hint),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            (options + selected).distinct().sorted().forEach { hours ->
                ChoiceRow(formatRetention(context, hours), hours == selected, {
                    onSelect(hours)
                    onDismiss()
                })
            }
        }
    }
}

@Composable
internal fun LanguageSheet(selected: NoteLanguage, onDismiss: () -> Unit, onSelect: (NoteLanguage) -> Unit) {
    FbSheet(onDismiss, title = stringResource(R.string.note_language)) {
        Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
            NoteLanguage.entries.forEach { language ->
                ChoiceRow(language.label, language == selected, {
                    onSelect(language)
                    onDismiss()
                }, description = "untitled.${language.extension}")
            }
        }
    }
}

/** Picks an accepted friend, or resolves an exact username, as the inbox recipient. */
@Composable
internal fun RecipientSheet(state: SendUiState, onDismiss: () -> Unit, model: SendViewModel) {
    val app = LocalAppModel.current.app
    val palette = Fb.palette
    var username by remember { mutableStateOf("") }
    var friends by remember { mutableStateOf<List<Contact>?>(null) }
    val selected = state.files.recipient
    LaunchedEffect(Unit) {
        friends = runCatching { app.accounts.contacts().contacts.filter { it.status == "accepted" } }.getOrDefault(emptyList())
    }
    // Close once a lookup lands on a recipient chosen from this sheet.
    var awaiting by remember { mutableStateOf(false) }
    LaunchedEffect(selected, state.recipientLookup) {
        if (awaiting && selected != null && state.recipientLookup == RecipientLookup.Idle) onDismiss()
    }
    FbSheet(onDismiss, title = stringResource(R.string.friend_title), subtitle = stringResource(R.string.friend_subtitle)) {
        if (selected != null) {
            Notice(stringResource(R.string.friend_selected, selected.username), tone = Tone.Accent, icon = R.drawable.ic_approved_users) {
                FbButton(stringResource(R.string.friend_clear), { model.clearRecipient() }, style = ButtonStyle.Quiet, height = 40.dp)
            }
        }
        FbTextField(
            username, { username = it.lowercase().filter { char -> char.isLetterOrDigit() || char == '_' || char == '@' }.take(25) },
            label = stringResource(R.string.friend_username), placeholder = stringResource(R.string.friend_username_placeholder),
            leadingIcon = R.drawable.ic_approved_user,
            error = (state.recipientLookup as? RecipientLookup.Failed)?.let { it.message.ifBlank { stringResource(R.string.friend_not_found) } },
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = {
                awaiting = true
                model.chooseRecipient(username)
            }),
            trailing = { if (state.recipientLookup == RecipientLookup.Searching) Spinner(size = 20.dp) },
        )
        FbButton(stringResource(R.string.friend_use_username), {
            awaiting = true
            model.chooseRecipient(username)
        }, Modifier.fillMaxWidth(), enabled = username.trim('@').length >= 3 && state.recipientLookup != RecipientLookup.Searching)
        val list = friends
        when {
            list == null -> Box(Modifier.fillMaxWidth().padding(Space.lg), contentAlignment = Alignment.Center) { Spinner() }
            list.isEmpty() -> FbText(stringResource(R.string.friend_none), style = Fb.type.small, color = palette.textMuted)
            else -> Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                FbText(stringResource(R.string.friend_list).uppercase(), style = Fb.type.eyebrow, color = palette.textMuted)
                list.forEach { friend ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clip(Shapes.control)
                            .background(if (selected?.username == friend.username) palette.selectedSurface else Color.Transparent)
                            .border(1.dp, if (selected?.username == friend.username) palette.choiceBorder else palette.border, Shapes.control)
                            .clickable(role = Role.Button) {
                                awaiting = true
                                model.chooseRecipient(friend.username)
                            }
                            .padding(Space.md),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(Space.md),
                    ) {
                        Avatar(friend.username)
                        FbText("@${friend.username}", Modifier.weight(1f), style = Fb.type.bodyStrong, maxLines = 1)
                        FbIcon(R.drawable.ic_approved_arrow_right, size = 16.dp)
                    }
                }
            }
        }
    }
}

/** An initial on the brand gradient, used wherever a person is shown. */
@Composable
fun Avatar(name: String, modifier: Modifier = Modifier) {
    val palette = Fb.palette
    Box(
        modifier.size(40.dp).clip(Shapes.pill).background(Brush.linearGradient(palette.brandGradient.takeLast(3))),
        contentAlignment = Alignment.Center,
    ) {
        FbText(name.take(1).uppercase(), style = Fb.type.label, color = Color.White)
    }
}
