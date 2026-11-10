package io.filebeam.android.ui.feature.settings

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.design.Appearance
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.ColorSource
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbSwitch
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Segment
import io.filebeam.android.ui.design.SegmentedRail
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.SmoothProgress
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.ThemeMode
import io.filebeam.android.ui.design.color.ColorMath
import io.filebeam.android.ui.design.color.PaletteEngine
import io.filebeam.android.ui.design.color.ThemePreset
import io.filebeam.android.ui.design.color.opaque
import io.filebeam.android.ui.design.materialYouAvailable
import io.filebeam.android.ui.design.pressScale
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.motion.rememberHaptics
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlin.math.roundToInt

/**
 * Theme and colour. Material You is offered where the system provides wallpaper colours;
 * the Filebeam presets and a custom colour are always available, and are the only choices
 * on systems without dynamic colour. Changes spread from the control that made them.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AppearanceScreen() {
    val app = LocalAppModel.current
    val appearance by app.appearance.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    val palette = Fb.palette
    var lastTouch by remember { mutableStateOf(Offset.Zero) }
    val source = appearance.source
    val custom = source as? ColorSource.Custom
    var customOpen by remember { mutableStateOf(custom != null) }
    val change: (Appearance) -> Unit = { app.setAppearance(it, lastTouch) }

    SubScreen(
        stringResource(R.string.appearance_title),
        // Remember where the finger went down so the reveal can start there.
        modifier = Modifier.pointerInput(Unit) {
            awaitPointerEventScope {
                while (true) {
                    val event = awaitPointerEvent(androidx.compose.ui.input.pointer.PointerEventPass.Initial)
                    event.changes.firstOrNull { it.pressed }?.let { lastTouch = it.position }
                }
            }
        },
    ) {
        item { PreviewCard() }
        item { SectionLabel(stringResource(R.string.appearance_theme)) }
        item {
            SegmentedRail(
                listOf(
                    Segment(ThemeMode.System, stringResource(R.string.appearance_system), R.drawable.ic_approved_monitor),
                    Segment(ThemeMode.Light, stringResource(R.string.appearance_light), R.drawable.ic_approved_sun),
                    Segment(ThemeMode.Dark, stringResource(R.string.appearance_dark), R.drawable.ic_approved_moon),
                ),
                appearance.mode, { change(appearance.copy(mode = it)) },
            )
        }
        item { SectionLabel(stringResource(R.string.appearance_color)) }
        if (materialYouAvailable) {
            item {
                SourceRow(
                    stringResource(R.string.appearance_material_you), stringResource(R.string.appearance_material_you_description),
                    selected = source == ColorSource.MaterialYou, icon = R.drawable.ic_approved_brush,
                ) { change(appearance.copy(source = ColorSource.MaterialYou)) }
            }
        } else {
            item { FbText(stringResource(R.string.appearance_no_material_you), style = Fb.type.small, color = palette.textMuted) }
        }
        item {
            Panel(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                    FbText(stringResource(R.string.appearance_filebeam_colors), style = Fb.type.bodyStrong)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.md), verticalArrangement = Arrangement.spacedBy(Space.md)) {
                        val instanceSeed = profile.primaryColor ?: PaletteEngine.DEFAULT_PRIMARY
                        Swatch(stringResource(R.string.appearance_instance), listOf(opaque(instanceSeed), opaque(instanceSeed)), source == ColorSource.Instance, R.drawable.ic_approved_rotate_left) {
                            change(appearance.copy(source = ColorSource.Instance))
                        }
                        ThemePreset.entries.forEach { preset ->
                            Swatch(preset.key.replaceFirstChar(Char::uppercase), listOf(opaque(preset.seed), opaque(preset.endpoint)), source == ColorSource.Preset(preset)) {
                                change(appearance.copy(source = ColorSource.Preset(preset)))
                            }
                        }
                        Swatch(
                            stringResource(R.string.appearance_custom),
                            listOf(Color(0xFFFF5F6D), Color(0xFFFFC371), Color(0xFF4BC0C8), Color(0xFF8B35FF)),
                            custom != null, R.drawable.ic_approved_color,
                        ) {
                            customOpen = true
                            if (custom == null) change(appearance.copy(source = ColorSource.Custom(profile.primaryColor ?: PaletteEngine.DEFAULT_PRIMARY)))
                        }
                    }
                    AnimatedVisibility(customOpen && custom != null, enter = expandVertically(Motion.spatial()) + fadeIn(Motion.enter()), exit = shrinkVertically(Motion.spatial()) + fadeOut(Motion.exit())) {
                        // Dragging applies instantly without a reveal, so the picker feels direct.
                        CustomPicker(custom?.seed ?: PaletteEngine.DEFAULT_PRIMARY) { seed -> app.setAppearance(appearance.copy(source = ColorSource.Custom(seed))) }
                    }
                }
            }
        }
        item {
            FbButton(stringResource(R.string.appearance_reset), { change(Appearance()) }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet, enabled = appearance != Appearance())
        }
        item { FbText(stringResource(R.string.appearance_saved_on_device), style = Fb.type.caption, color = palette.textSubtle) }
    }
}

/** A small specimen of the current palette so a change can be judged at a glance. */
@Composable
private fun PreviewCard() {
    val palette = Fb.palette
    var on by remember { mutableStateOf(true) }
    Panel(Modifier.fillMaxWidth(), elevation = 8.dp) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                io.filebeam.android.ui.design.FilebeamMark(height = 30.dp)
                Column(Modifier.weight(1f)) {
                    FbText(stringResource(R.string.appearance_preview_title), style = Fb.type.title)
                    FbText(stringResource(R.string.appearance_preview_body), style = Fb.type.small, color = palette.textMuted)
                }
                FbSwitch(on, { on = it })
            }
            SmoothProgress(if (on) 0.68f else 0.24f)
            Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                FbButton(stringResource(R.string.send_encrypted), {}, Modifier.weight(1f), icon = R.drawable.ic_approved_lock, height = 44.dp)
                FbButton(stringResource(R.string.copy), {}, style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_copy, height = 44.dp)
            }
        }
    }
}

@Composable
private fun SourceRow(title: String, description: String, selected: Boolean, icon: Int, onClick: () -> Unit) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    Row(
        Modifier
            .fillMaxWidth()
            .clip(Shapes.card)
            .background(if (selected) palette.selectedSurface else palette.surface)
            .border(1.dp, if (selected) palette.choiceBorder else palette.border, Shapes.card)
            .selectable(selected, role = Role.RadioButton) {
                haptics.tick()
                onClick()
            }
            .padding(Space.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.md),
    ) {
        Box(Modifier.size(44.dp).clip(Shapes.pill).background(Brush.sweepGradient(palette.brandGradient + palette.brandGradient.first())), contentAlignment = Alignment.Center) {
            FbIcon(icon, tint = Color.White, size = 22.dp)
        }
        Column(Modifier.weight(1f)) {
            FbText(title, style = Fb.type.bodyStrong)
            FbText(description, style = Fb.type.small, color = palette.textMuted)
        }
        if (selected) FbIcon(R.drawable.ic_approved_check, tint = palette.accentText)
    }
}

/** A round colour choice. The selected swatch grows a ring and a check mark. */
@Composable
private fun Swatch(label: String, colors: List<Color>, selected: Boolean, icon: Int? = null, onClick: () -> Unit) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    val interaction = remember { MutableInteractionSource() }
    val ring by animateDpAsState(if (selected) 3.dp else 0.dp, motionOr(Motion.spatial()), label = "ring")
    val scale by animateFloatAsState(if (selected) 1f else 0.9f, motionOr(Motion.bouncy()), label = "scale")
    Box(
        Modifier
            .pressScale(interaction, 0.9f)
            .size(56.dp)
            .clip(Shapes.pill)
            .border(ring, palette.accentText, Shapes.pill)
            .selectable(selected, interaction, indication = null, role = Role.RadioButton) {
                haptics.tick()
                onClick()
            }
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                }
                .size(44.dp)
                .clip(Shapes.pill)
                .background(Brush.linearGradient(if (colors.size == 1) colors + colors else colors))
                .border(1.dp, palette.lineStrong, Shapes.pill),
            contentAlignment = Alignment.Center,
        ) {
            when {
                selected -> FbIcon(R.drawable.ic_approved_check, tint = Color.White, size = 20.dp)
                icon != null -> FbIcon(icon, tint = Color.White, size = 18.dp)
            }
        }
    }
}

/** Hue and vividness sliders with a hex field. Colours are generated in OKLCH so every hue reads evenly. */
@Composable
private fun CustomPicker(seed: Int, onChange: (Int) -> Unit) {
    val initial = remember { ColorMath.oklch(seed) }
    var hue by remember { mutableFloatStateOf(initial.h.toFloat()) }
    var chroma by remember { mutableFloatStateOf((initial.c / 0.3).toFloat().coerceIn(0f, 1f)) }
    var hex by remember { mutableStateOf(ColorMath.format(seed)) }
    fun emit() {
        val color = ColorMath.rgb(0.62, chroma * 0.3, hue.toDouble())
        hex = ColorMath.format(color)
        onChange(color)
    }
    Column(verticalArrangement = Arrangement.spacedBy(Space.lg)) {
        GradientSlider(
            stringResource(R.string.appearance_hue), hue / 360f,
            (0..12).map { opaque(ColorMath.rgb(0.68, 0.17, it * 30.0)) },
        ) {
            hue = it * 360f
            emit()
        }
        GradientSlider(
            stringResource(R.string.appearance_vividness), chroma,
            listOf(opaque(ColorMath.rgb(0.62, 0.0, hue.toDouble())), opaque(ColorMath.rgb(0.62, 0.3, hue.toDouble()))),
        ) {
            chroma = it
            emit()
        }
        FbTextField(
            hex, { text ->
                hex = text.take(7)
                Regex("^#?([0-9a-fA-F]{6})$").matchEntire(text.trim())?.let { match ->
                    val color = match.groupValues[1].toInt(16)
                    val parsed = ColorMath.oklch(color)
                    hue = parsed.h.toFloat()
                    chroma = (parsed.c / 0.3).toFloat().coerceIn(0f, 1f)
                    onChange(color)
                }
            },
            label = stringResource(R.string.appearance_hex), placeholder = "#8b35ff", textStyle = Fb.type.code,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
        )
    }
}

/** A slider drawn on its own gradient. It ticks as it moves and is adjustable by accessibility services. */
@Composable
private fun GradientSlider(label: String, value: Float, colors: List<Color>, onChange: (Float) -> Unit) {
    val palette = Fb.palette
    val haptics = rememberHaptics()
    var width by remember { mutableFloatStateOf(1f) }
    var lastStep by remember { mutableStateOf(-1) }
    fun update(x: Float) {
        val next = (x / width).coerceIn(0f, 1f)
        val step = (next * 24).roundToInt()
        if (step != lastStep) {
            lastStep = step
            haptics.tick()
        }
        onChange(next)
    }
    Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
        FbText(label, style = Fb.type.smallStrong)
        Canvas(
            Modifier
                .fillMaxWidth()
                .height(36.dp)
                .onGloballyPositioned { width = it.size.width.toFloat().coerceAtLeast(1f); it.positionInRoot() }
                .pointerInput(Unit) { detectTapGestures { update(it.x) } }
                .pointerInput(Unit) { detectDragGestures { change, _ -> update(change.position.x) } }
                .semantics {
                    contentDescription = label
                    progressBarRangeInfo = ProgressBarRangeInfo(value, 0f..1f)
                    setProgress { target ->
                        onChange(target.coerceIn(0f, 1f))
                        true
                    }
                },
        ) {
            val trackHeight = 14.dp.toPx()
            val top = (size.height - trackHeight) / 2
            drawRoundRect(Brush.horizontalGradient(colors), Offset(0f, top), androidx.compose.ui.geometry.Size(size.width, trackHeight), CornerRadius(trackHeight / 2))
            val x = (size.width * value).coerceIn(size.height / 2, size.width - size.height / 2)
            drawCircle(palette.shadow, size.height / 2 - 1.dp.toPx(), Offset(x, size.height / 2 + 1.dp.toPx()))
            drawCircle(Color.White, size.height / 2 - 3.dp.toPx(), Offset(x, size.height / 2))
            drawCircle(palette.border, size.height / 2 - 3.dp.toPx(), Offset(x, size.height / 2), style = androidx.compose.ui.graphics.drawscope.Stroke(1.dp.toPx()))
        }
    }
}
