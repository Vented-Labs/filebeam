package io.filebeam.android.ui.design

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/**
 * A bottom sheet in the Filebeam surface style with an optional title row.
 * Content scrolls when it outgrows the sheet and stays above the keyboard.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FbSheet(
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    title: String? = null,
    subtitle: String? = null,
    expanded: Boolean = true,
    content: @Composable ColumnScope.() -> Unit,
) {
    val palette = Fb.palette
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        modifier = modifier,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = expanded),
        shape = Shapes.sheet,
        containerColor = palette.surface,
        contentColor = palette.text,
        scrimColor = palette.scrim,
        tonalElevation = 0.dp,
        dragHandle = {
            Box(Modifier.padding(top = Space.md, bottom = Space.sm).size(width = 36.dp, height = 4.dp).clip(Shapes.pill).background(palette.controlBorder.copy(alpha = 0.6f)))
        },
        contentWindowInsets = { WindowInsets(0, 0, 0, 0) },
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .navigationBarsPadding()
                .padding(horizontal = Space.gutter)
                .padding(bottom = Space.xxl),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            if (title != null) {
                Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                    FbText(title, style = Fb.type.title)
                    if (subtitle != null) FbText(subtitle, style = Fb.type.small, color = palette.textMuted)
                }
            }
            content()
        }
    }
}

/** A centred confirmation dialog with a statement, optional body and up to two actions. */
@Composable
fun FbDialog(
    onDismiss: () -> Unit,
    title: String,
    confirmLabel: String,
    onConfirm: () -> Unit,
    modifier: Modifier = Modifier,
    body: String? = null,
    dismissLabel: String? = null,
    destructive: Boolean = false,
    confirmEnabled: Boolean = true,
    confirmLoading: Boolean = false,
    content: (@Composable ColumnScope.() -> Unit)? = null,
) {
    val palette = Fb.palette
    Dialog(onDismiss, DialogProperties(usePlatformDefaultWidth = false)) {
        Column(
            modifier
                .padding(horizontal = Space.xxl)
                .widthIn(max = 420.dp)
                .fillMaxWidth()
                .shadow(24.dp, Shapes.panel, ambientColor = palette.shadow, spotColor = palette.shadow)
                .clip(Shapes.panel)
                .background(palette.raised)
                .padding(Space.xxl),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
        ) {
            FbText(title, style = Fb.type.title)
            if (body != null) FbText(body, style = Fb.type.small, color = palette.textMuted)
            content?.invoke(this)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Space.sm, Alignment.End)) {
                if (dismissLabel != null) FbButton(dismissLabel, onDismiss, style = ButtonStyle.Quiet, height = 44.dp)
                FbButton(
                    confirmLabel, onConfirm,
                    style = if (destructive) ButtonStyle.Danger else ButtonStyle.Primary,
                    enabled = confirmEnabled, loading = confirmLoading, height = 44.dp,
                )
            }
        }
    }
}
