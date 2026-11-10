package io.filebeam.android.ui.design

import androidx.compose.material3.Typography
import androidx.compose.runtime.Immutable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import io.filebeam.android.R

private val weights = listOf(400, 500, 600, 700, 800)

/** Inter's optical-size axis runs 14 (text) to 32 (display); each family pins one end. */
private fun inter(opticalSize: Float) = FontFamily(
    weights.map { weight ->
        Font(
            R.font.inter_variable,
            weight = FontWeight(weight),
            variationSettings = FontVariation.Settings(FontVariation.weight(weight), FontVariation.Setting("opsz", opticalSize)),
        )
    },
)

val InterText = inter(14f)
val InterDisplay = inter(32f)
val JetBrainsMono = FontFamily(
    listOf(400, 500, 700).map { weight ->
        Font(R.font.jetbrains_mono_variable, weight = FontWeight(weight), variationSettings = FontVariation.Settings(FontVariation.weight(weight)))
    },
)

private val centered = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None)

/** The type scale. Sizes follow the web tokens, tightened for a phone-width measure. */
@Immutable
class FilebeamType {
    private fun text(size: Int, line: Int, weight: Int, tracking: Double = 0.0) = TextStyle(
        fontFamily = InterText, fontSize = size.sp, lineHeight = line.sp, fontWeight = FontWeight(weight),
        letterSpacing = tracking.em, lineHeightStyle = centered,
    )

    private fun display(size: Int, line: Int, weight: Int, tracking: Double) = TextStyle(
        fontFamily = InterDisplay, fontSize = size.sp, lineHeight = line.sp, fontWeight = FontWeight(weight),
        letterSpacing = tracking.em, lineHeightStyle = centered,
    )

    /** Hero statements such as "Drop your files here". */
    val display = display(34, 38, 700, -0.035)
    val headline = display(26, 32, 700, -0.028)
    val title = display(20, 26, 600, -0.02)
    val subtitle = text(17, 24, 600, -0.012)
    val body = text(16, 24, 400, -0.006)
    val bodyStrong = text(16, 24, 600, -0.008)
    val small = text(14, 20, 400)
    val smallStrong = text(14, 20, 600, -0.004)
    val caption = text(12, 16, 500, 0.004)
    val label = text(15, 20, 600, -0.006)

    /** Small caps style section markers, e.g. "TURBO TRANSFER". */
    val eyebrow = text(11, 16, 700, 0.14)
    val code = TextStyle(fontFamily = JetBrainsMono, fontSize = 14.sp, lineHeight = 23.sp, fontWeight = FontWeight(400), lineHeightStyle = centered)
    val codeSmall = TextStyle(fontFamily = JetBrainsMono, fontSize = 12.sp, lineHeight = 18.sp, fontWeight = FontWeight(400), lineHeightStyle = centered)

    /** Tabular figures so counters do not jitter as digits change. */
    val numeric = small.copy(fontFeatureSettings = "tnum", fontWeight = FontWeight(500))

    val material = Typography(
        displayLarge = display, displayMedium = display, displaySmall = headline,
        headlineLarge = headline, headlineMedium = headline, headlineSmall = title,
        titleLarge = title, titleMedium = subtitle, titleSmall = smallStrong,
        bodyLarge = body, bodyMedium = small, bodySmall = caption,
        labelLarge = label, labelMedium = smallStrong, labelSmall = caption,
    )
}
