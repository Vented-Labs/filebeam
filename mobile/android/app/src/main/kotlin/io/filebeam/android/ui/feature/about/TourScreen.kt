package io.filebeam.android.ui.feature.about

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Emblem
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.motion.LocalReduceMotion
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr
import io.filebeam.android.ui.motion.rememberHaptics
import io.filebeam.android.ui.shell.ContentMaxWidth
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlinx.coroutines.launch
import kotlin.math.absoluteValue

private data class Chapter(val eyebrow: Int, val title: Int, val body: Int, val art: Int)

/**
 * A short, animated introduction to what Filebeam does, mirroring the chapters of the web
 * Features page. Shown once on first run and available again from About.
 */
@Composable
fun TourScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val profile by app.instance.collectAsStateCompat()
    val haptics = rememberHaptics()
    val scope = rememberCoroutineScope()
    val palette = Fb.palette
    val chapters = listOf(
        Chapter(R.string.tour_hero_eyebrow, R.string.tour_hero_title, R.string.tour_hero_body, 0),
        Chapter(R.string.tour_sharing_eyebrow, R.string.tour_sharing_title, R.string.tour_sharing_body, 1),
        Chapter(R.string.tour_notes_eyebrow, R.string.tour_notes_title, R.string.tour_notes_body, 2),
        Chapter(R.string.tour_friends_eyebrow, R.string.tour_friends_title, R.string.tour_friends_body, 3),
        Chapter(R.string.tour_privacy_eyebrow, R.string.tour_privacy_title, R.string.tour_privacy_body, 4),
        Chapter(R.string.tour_devices_eyebrow, R.string.tour_devices_title, if (profile.deviceRendezvousEnabled) R.string.tour_devices_body else R.string.tour_devices_body_disabled, 5),
        Chapter(R.string.tour_open_eyebrow, R.string.tour_open_title, R.string.tour_open_body, 6),
    )
    val pager = rememberPagerState { chapters.size }
    val last = pager.currentPage == chapters.lastIndex
    val finish = {
        scope.launch { app.app.appearance.markTourSeen() }
        navigator.back()
        Unit
    }
    LaunchedEffect(pager.currentPage) { haptics.tick() }

    Column(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding(), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(Modifier.widthIn(max = ContentMaxWidth).fillMaxWidth().padding(horizontal = Space.gutter, vertical = Space.sm), verticalAlignment = Alignment.CenterVertically) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                chapters.indices.forEach { index ->
                    val active by animateFloatAsState(if (index == pager.currentPage) 1f else 0f, motionOr(Motion.spatial()), label = "dot")
                    Box(Modifier.height(6.dp).width((8 + 18 * active).dp).clip(Shapes.pill).background(if (index <= pager.currentPage) palette.accentText else palette.controlBorder.copy(alpha = 0.5f)))
                }
            }
            FbButton(stringResource(R.string.tour_skip), finish, style = ButtonStyle.Quiet, height = 40.dp)
        }
        HorizontalPager(pager, Modifier.weight(1f).fillMaxWidth()) { page ->
            val chapter = chapters[page]
            // Pages ease in as they approach the centre, so art and copy arrive separately.
            val offset = { ((pager.currentPage - page) + pager.currentPageOffsetFraction).absoluteValue.coerceIn(0f, 1f) }
            Column(
                Modifier.fillMaxSize().padding(horizontal = Space.xxl),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(Space.xl, Alignment.CenterVertically),
            ) {
                Box(
                    Modifier.fillMaxWidth().height(260.dp).graphicsLayer {
                        alpha = 1f - offset()
                        val scale = 1f - 0.12f * offset()
                        scaleX = scale
                        scaleY = scale
                    },
                    contentAlignment = Alignment.Center,
                ) { Art(chapter.art) }
                Column(
                    Modifier.widthIn(max = 420.dp).graphicsLayer {
                        alpha = (1f - offset() * 1.4f).coerceAtLeast(0f)
                        translationY = offset() * 28.dp.toPx()
                    },
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(Space.md),
                ) {
                    FbText(stringResource(chapter.eyebrow).uppercase(), style = Fb.type.eyebrow, color = palette.accentText)
                    FbText(stringResource(chapter.title), Modifier.semantics { heading() }, style = Fb.type.display, textAlign = TextAlign.Center)
                    FbText(stringResource(chapter.body), style = Fb.type.body, color = palette.textMuted, textAlign = TextAlign.Center)
                }
            }
        }
        FbButton(
            stringResource(if (last) R.string.tour_start else R.string.tour_next),
            { if (last) finish() else scope.launch { pager.animateScrollToPage(pager.currentPage + 1) } },
            Modifier.widthIn(max = ContentMaxWidth).fillMaxWidth().padding(horizontal = Space.gutter, vertical = Space.lg),
            trailingIcon = R.drawable.ic_approved_arrow_right,
        )
    }
}

@Composable
private fun Art(kind: Int) {
    val palette = Fb.palette
    when (kind) {
        0 -> Emblem(engaged = true, size = 170.dp)
        1 -> Beam(R.drawable.ic_custom_http_server, R.drawable.ic_custom_webrtc_p2p)
        2 -> Tile(R.drawable.ic_approved_note) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf(0.9f to palette.editor.keyword, 0.6f to palette.editor.string, 0.75f to palette.editor.property, 0.4f to palette.editor.comment).forEach { (width, color) ->
                    Box(Modifier.fillMaxWidth(width).height(8.dp).clip(Shapes.pill).background(color.copy(alpha = 0.8f)))
                }
            }
        }
        3 -> Beam(R.drawable.ic_approved_user, R.drawable.ic_approved_users)
        4 -> Tile(R.drawable.ic_approved_shield) {
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                StatusPill(stringResource(R.string.tour_privacy_server_sees), tone = Tone.Neutral)
                StatusPill(stringResource(R.string.tour_privacy_server_never), tone = Tone.Success, icon = R.drawable.ic_approved_lock)
            }
        }
        5 -> Beam(R.drawable.ic_approved_laptop, R.drawable.ic_approved_monitor)
        else -> Tile(R.drawable.ic_approved_heart) { FbText("MIT", style = Fb.type.display, color = palette.accentText) }
    }
}

/** A framed illustration: an icon badge above a small piece of content. */
@Composable
private fun Tile(icon: Int, content: @Composable () -> Unit) {
    val palette = Fb.palette
    Column(
        Modifier.width(240.dp).clip(Shapes.panel).background(Brush.linearGradient(listOf(palette.cardFront, palette.cardFrontEnd))).border(1.dp, palette.controlBorder.copy(alpha = 0.6f), Shapes.panel).padding(Space.xl),
        verticalArrangement = Arrangement.spacedBy(Space.lg),
    ) {
        Box(Modifier.size(52.dp).clip(Shapes.card).background(palette.accentSurface).border(1.dp, palette.choiceBorder.copy(alpha = 0.5f), Shapes.card), contentAlignment = Alignment.Center) {
            FbIcon(icon, tint = palette.accentText, size = 26.dp)
        }
        content()
    }
}

/** Two endpoints joined by the beam: a gradient line with an encrypted packet travelling along it. */
@Composable
private fun Beam(from: Int, to: Int) {
    val palette = Fb.palette
    val travel = if (LocalReduceMotion.current) 0.5f else {
        val transition = rememberInfiniteTransition(label = "beam")
        val value by transition.animateFloat(0f, 1f, infiniteRepeatable(tween(1300, easing = LinearEasing), RepeatMode.Restart), label = "packet")
        value
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Endpoint(from)
        Canvas(Modifier.width(120.dp).height(40.dp)) {
            val y = size.height / 2
            drawLine(Brush.horizontalGradient(palette.brandGradient), Offset(0f, y), Offset(size.width, y), 3.dp.toPx(), StrokeCap.Round)
            val x = size.width * travel
            drawCircle(palette.brandGradient.last().copy(alpha = 0.25f), 12.dp.toPx(), Offset(x, y))
            drawCircle(Color.White, 5.dp.toPx(), Offset(x, y))
        }
        Endpoint(to)
    }
}

@Composable
private fun Endpoint(icon: Int) {
    val palette = Fb.palette
    Box(
        Modifier.size(84.dp).clip(Shapes.panel).background(Brush.linearGradient(listOf(palette.cardFront, palette.cardFrontEnd))).border(1.dp, palette.controlBorder.copy(alpha = 0.6f), Shapes.panel),
        contentAlignment = Alignment.Center,
    ) { FbIcon(icon, tint = palette.accentText, size = 36.dp) }
}
