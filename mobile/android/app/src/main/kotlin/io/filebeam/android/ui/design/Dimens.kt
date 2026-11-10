package io.filebeam.android.ui.design

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp

/** Spacing on the web's 4 px grid. */
object Space {
    val xxs = 2.dp
    val xs = 4.dp
    val sm = 8.dp
    val md = 12.dp
    val lg = 16.dp
    val xl = 20.dp
    val xxl = 24.dp
    val xxxl = 32.dp

    /** Horizontal page margin. */
    val gutter = 20.dp

    /** Minimum interactive size. */
    val touch = 48.dp
    val control = 52.dp
}

/** Corner radii from the web tokens, with a touch-sized control radius. */
object Radius {
    val sm = 8.dp
    val control = 14.dp
    val card = 18.dp
    val panel = 24.dp
    val sheet = 28.dp
}

object Shapes {
    val sm = RoundedCornerShape(Radius.sm)
    val control = RoundedCornerShape(Radius.control)
    val card = RoundedCornerShape(Radius.card)
    val panel = RoundedCornerShape(Radius.panel)
    val sheet = RoundedCornerShape(topStart = Radius.sheet, topEnd = Radius.sheet)
    val pill = RoundedCornerShape(percent = 50)
}
