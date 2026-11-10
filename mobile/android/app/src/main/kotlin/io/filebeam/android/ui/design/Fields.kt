package io.filebeam.android.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr

/**
 * A labelled single- or multi-line input. The border warms to the accent on focus and to the
 * danger colour while [error] is set; the error message slides in beneath.
 */
@Composable
fun FbTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
    placeholder: String? = null,
    supporting: String? = null,
    error: String? = null,
    @DrawableRes leadingIcon: Int? = null,
    trailing: (@Composable () -> Unit)? = null,
    enabled: Boolean = true,
    readOnly: Boolean = false,
    singleLine: Boolean = true,
    minLines: Int = 1,
    maxLines: Int = if (singleLine) 1 else 6,
    textStyle: TextStyle = Fb.type.body,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
) {
    val palette = Fb.palette
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val border by animateColorAsState(
        when {
            error != null -> palette.danger.border
            focused -> palette.accentText
            !enabled -> palette.disabledBorder
            else -> palette.controlBorder
        },
        motionOr(Motion.fade()), label = "border",
    )
    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        if (label != null) FbText(label, style = Fb.type.smallStrong, color = if (enabled) palette.text else palette.disabledText)
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.fillMaxWidth().semantics { if (error != null) error(error) },
            enabled = enabled,
            readOnly = readOnly,
            textStyle = textStyle.copy(color = if (enabled) palette.text else palette.disabledText),
            singleLine = singleLine,
            minLines = minLines,
            maxLines = maxLines,
            keyboardOptions = keyboardOptions,
            keyboardActions = keyboardActions,
            visualTransformation = visualTransformation,
            interactionSource = interaction,
            cursorBrush = SolidColor(palette.accentText),
            decorationBox = { inner ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .defaultMinSize(minHeight = Space.control)
                        .clip(Shapes.control)
                        .background(if (enabled) palette.sunken else palette.disabledSurface)
                        .border(if (focused || error != null) 1.5.dp else 1.dp, border, Shapes.control)
                        .padding(horizontal = Space.lg, vertical = Space.md),
                    verticalAlignment = if (singleLine) Alignment.CenterVertically else Alignment.Top,
                    horizontalArrangement = Arrangement.spacedBy(Space.md),
                ) {
                    if (leadingIcon != null) FbIcon(leadingIcon, tint = if (focused) palette.accentText else palette.textMuted, size = 20.dp)
                    Box(Modifier.weight(1f), contentAlignment = if (singleLine) Alignment.CenterStart else Alignment.TopStart) {
                        if (value.isEmpty() && placeholder != null) FbText(placeholder, style = textStyle, color = palette.textSubtle, maxLines = if (singleLine) 1 else maxLines)
                        inner()
                    }
                    trailing?.invoke()
                }
            },
        )
        AnimatedVisibility(error != null || supporting != null, enter = fadeIn(Motion.fade()) + expandVertically(Motion.spatialFast()), exit = fadeOut(Motion.exit()) + shrinkVertically(Motion.spatialFast())) {
            FbText(error ?: supporting.orEmpty(), style = Fb.type.small, color = if (error != null) palette.danger.ink else palette.textMuted)
        }
    }
}

/** A password input with a show/hide control. The value is never logged or persisted by this component. */
@Composable
fun PasswordField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    label: String? = null,
    placeholder: String? = null,
    supporting: String? = null,
    error: String? = null,
    enabled: Boolean = true,
    keyboardOptions: KeyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    showLabel: String,
    hideLabel: String,
) {
    var visible by remember { mutableStateOf(false) }
    FbTextField(
        value, onValueChange, modifier, label, placeholder, supporting, error,
        leadingIcon = R.drawable.ic_approved_key,
        trailing = {
            IconAction(
                if (visible) R.drawable.ic_approved_eye_off else R.drawable.ic_approved_eye,
                if (visible) hideLabel else showLabel,
                { visible = !visible },
                size = 36.dp,
            )
        },
        enabled = enabled,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        visualTransformation = if (visible) VisualTransformation.None else PasswordVisualTransformation(),
    )
}
