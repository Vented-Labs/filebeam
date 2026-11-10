package io.filebeam.android.ui.design

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import io.filebeam.android.ui.design.color.ColorMath
import io.filebeam.android.ui.design.color.FilebeamPalette
import io.filebeam.android.ui.design.color.PaletteEngine
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** The fixture is the web theme generator's output; Android must derive identical colours. */
class PaletteEngineTest {
    private val golden = JSONObject(File("src/test/fixtures/palette-golden.json").readText())
    private val hex = Regex("#[0-9a-f]{6}")

    private fun Color.hex() = ColorMath.format(toArgb())

    private fun roles(palette: FilebeamPalette) = mapOf(
        "--fb-bg" to palette.background, "--fb-surface" to palette.surface, "--fb-surface-raised" to palette.raised,
        "--fb-surface-sunken" to palette.sunken, "--fb-border" to palette.border, "--fb-control-border" to palette.controlBorder,
        "--fb-control-border-hover" to palette.controlBorderHover, "--fb-surface-hover" to palette.hover,
        "--fb-surface-active" to palette.active, "--fb-accent-text" to palette.accentText,
        "--fb-accent-surface" to palette.accentSurface, "--fb-selected-surface" to palette.selectedSurface,
        "--fb-action" to palette.action, "--fb-action-active" to palette.actionPressed, "--fb-on-action" to palette.onAction,
        "--fb-action-border" to palette.actionBorder, "--fb-progress-fill" to palette.progressFill,
        "--fb-progress-track" to palette.progressTrack, "--fb-editor-bg" to palette.editor.background,
        "--fb-editor-keyword" to palette.editor.keyword, "--fb-editor-type" to palette.editor.type,
        "--fb-editor-heading" to palette.editor.heading, "--fb-editor-selection" to palette.editor.selection,
    )

    @Test fun matchesWebThemeGenerator() {
        val mismatches = mutableListOf<String>()
        for (seed in golden.keys()) for (mode in listOf("dark", "light")) {
            val expected = golden.getJSONObject(seed).getJSONObject(mode)
            val palette = PaletteEngine.seeded(ColorMath.parse(seed), mode == "dark")
            roles(palette).forEach { (name, color) ->
                if (color.hex() != expected.getString(name)) mismatches += "$seed $mode $name: ${color.hex()} != ${expected.getString(name)}"
            }
            val gradient = hex.findAll(expected.getString("--fb-brand-gradient")).map { it.value }.toList()
            if (palette.brandGradient.map { it.hex() } != gradient) mismatches += "$seed $mode gradient: ${palette.brandGradient.map { it.hex() }} != $gradient"
            val back = hex.findAll(expected.getString("--fb-dropzone-mark")).map { it.value }.toList()
            if (listOf(palette.cardBack.hex(), palette.cardBackEnd.hex()) != back) mismatches += "$seed $mode card back: ${palette.cardBack.hex()} ${palette.cardBackEnd.hex()} != $back"
            val front = hex.findAll(expected.getString("--fb-dropzone-mark-hover")).map { it.value }.toList()
            if (listOf(palette.cardFront.hex(), palette.cardFrontEnd.hex()) != front) mismatches += "$seed $mode card front: ${palette.cardFront.hex()} ${palette.cardFrontEnd.hex()} != $front"
        }
        assertTrue(mismatches.joinToString("\n"), mismatches.isEmpty())
    }

    @Test fun textAndActionsMeetTheContrastContract() {
        for (seed in golden.keys()) for (dark in listOf(true, false)) {
            val palette = PaletteEngine.seeded(ColorMath.parse(seed), dark)
            val surfaces = listOf(palette.background, palette.surface, palette.raised, palette.sunken).map { it.toArgb() and 0xFFFFFF }
            assertTrue("$seed text", ColorMath.minimumContrast(palette.text.toArgb() and 0xFFFFFF, surfaces) >= 4.5)
            assertTrue("$seed accent", ColorMath.minimumContrast(palette.accentText.toArgb() and 0xFFFFFF, surfaces) >= 4.5)
            assertTrue("$seed action", ColorMath.contrast(palette.onAction.toArgb() and 0xFFFFFF, palette.action.toArgb() and 0xFFFFFF) >= 4.8)
            assertTrue("$seed control", ColorMath.minimumContrast(palette.controlBorder.toArgb() and 0xFFFFFF, surfaces) >= 3.0)
        }
    }

    @Test fun roundTripsThroughOklch() {
        for (rgb in listOf(0x8b35ff, 0x000000, 0xffffff, 0x808080, 0x12ab9f)) {
            assertEquals(rgb, ColorMath.rgb(ColorMath.oklch(rgb)))
        }
    }
}
