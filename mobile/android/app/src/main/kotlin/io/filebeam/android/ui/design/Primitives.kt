package io.filebeam.android.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr

/** A two-tone icon from the approved set, tinted with a semantic colour. */
@Composable
fun FbIcon(
    @DrawableRes id: Int,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
    tint: Color = Fb.palette.textMuted,
    size: Dp = 22.dp,
) {
    Icon(painterResource(id), contentDescription, modifier.size(size), tint)
}

/** Text in a Filebeam style. Colour defaults to primary text. */
@Composable
fun FbText(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = Fb.type.body,
    color: Color = Fb.palette.text,
    textAlign: TextAlign? = null,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Ellipsis,
) {
    Text(text, modifier, color = color, style = style, textAlign = textAlign, maxLines = maxLines, overflow = overflow)
}

/** Shrinks slightly while pressed and springs back on release. */
@Composable
fun Modifier.pressScale(interactionSource: InteractionSource, pressed: Float = 0.97f): Modifier {
    val isPressed by interactionSource.collectIsPressedAsState()
    val scale by animateFloatAsState(if (isPressed) pressed else 1f, motionOr(Motion.spatialFast()), label = "press")
    return graphicsLayer { scaleX = scale; scaleY = scale }
}

/** The standard raised container: surface fill, hairline border, soft shadow. */
@Composable
fun Panel(
    modifier: Modifier = Modifier,
    shape: Shape = Shapes.panel,
    color: Color = Fb.palette.surface,
    border: Color = Fb.palette.border,
    elevation: Dp = 0.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val palette = Fb.palette
    Column(
        modifier
            .then(if (elevation > 0.dp) Modifier.shadow(elevation, shape, ambientColor = palette.shadow, spotColor = palette.shadow) else Modifier)
            .clip(shape)
            .background(color)
            .border(BorderStroke(1.dp, border), shape),
        content = content,
    )
}

@Composable
fun PanelBox(
    modifier: Modifier = Modifier,
    shape: Shape = Shapes.card,
    color: Color = Fb.palette.surface,
    border: Color = Fb.palette.border,
    content: @Composable BoxScope.() -> Unit,
) {
    Box(modifier.clip(shape).background(color).border(BorderStroke(1.dp, border), shape), content = content)
}

/** A one-pixel separator in the palette's soft line colour. */
@Composable
fun Hairline(modifier: Modifier = Modifier, color: Color = Fb.palette.border) {
    Box(modifier.fillMaxWidth().height(1.dp).background(color))
}
