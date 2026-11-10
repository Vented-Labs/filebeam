package io.filebeam.android.ui.shell

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FilebeamMark
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.pressScale
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.motion.motionOr

/** Content never grows wider than this on tablets and unfolded devices. */
val ContentMaxWidth: Dp = 640.dp

/** The brand row at the top of tab screens: mark, wordmark and the account entry point. */
@Composable
fun BrandBar(modifier: Modifier = Modifier, actions: @Composable RowScope.() -> Unit = {}) {
    val palette = Fb.palette
    val model = LocalAppModel.current
    val navigator = LocalNavigator.current
    val account by model.account.collectAsStateCompat()
    Row(
        modifier.fillMaxWidth().height(56.dp).padding(horizontal = Space.gutter),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FilebeamMark(height = 26.dp)
        FbText(stringResource(R.string.app_name), Modifier.padding(start = Space.sm).weight(1f), style = Fb.type.title.copy(fontWeight = androidx.compose.ui.text.font.FontWeight(800)))
        actions()
        val interaction = remember { MutableInteractionSource() }
        val label = account?.let { stringResource(R.string.account_signed_in_as, it.username) } ?: stringResource(R.string.account_open)
        Box(
            Modifier
                .padding(start = Space.sm)
                .pressScale(interaction, 0.92f)
                .size(40.dp)
                .clip(Shapes.pill)
                .background(if (account != null) Brush.linearGradient(palette.brandGradient.takeLast(3)) else Brush.linearGradient(listOf(palette.surface, palette.surface)))
                .border(1.dp, if (account != null) palette.actionBorder else palette.controlBorder, Shapes.pill)
                .clickable(interaction, indication = null, role = Role.Button) { navigator.push(if (account != null) Route.Account else Route.Auth()) }
                .semantics { contentDescription = label },
            contentAlignment = Alignment.Center,
        ) {
            val name = account?.username
            if (name != null) FbText(name.take(1).uppercase(), style = Fb.type.label, color = Color.White)
            else io.filebeam.android.ui.design.FbIcon(R.drawable.ic_approved_user, tint = palette.textMuted, size = 20.dp)
        }
    }
}

/**
 * A scrolling tab screen. Items rise in on first appearance; the bottom is padded so the
 * last item clears the floating navigation bar.
 */
@Composable
fun TabScreen(
    modifier: Modifier = Modifier,
    state: LazyListState = rememberLazyListState(),
    actions: @Composable RowScope.() -> Unit = {},
    content: LazyListScope.() -> Unit,
) {
    Column(modifier.fillMaxSize().statusBarsPadding(), horizontalAlignment = Alignment.CenterHorizontally) {
        BrandBar(Modifier.widthIn(max = ContentMaxWidth), actions)
        LazyColumn(
            Modifier.widthIn(max = ContentMaxWidth).fillMaxSize().imePadding(),
            state = state,
            contentPadding = PaddingValues(start = Space.gutter, end = Space.gutter, top = Space.sm, bottom = NavBarClearance + Space.xxl),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
            content = content,
        )
    }
}

/** A pushed screen with a back affordance and a title that gains a divider once content scrolls under it. */
@Composable
fun SubScreen(
    title: String,
    modifier: Modifier = Modifier,
    state: LazyListState = rememberLazyListState(),
    actions: @Composable RowScope.() -> Unit = {},
    bottomBar: (@Composable ColumnScope.() -> Unit)? = null,
    content: LazyListScope.() -> Unit,
) {
    val palette = Fb.palette
    val navigator = LocalNavigator.current
    val scrolled by remember(state) { derivedStateOf { state.firstVisibleItemIndex > 0 || state.firstVisibleItemScrollOffset > 4 } }
    val divider by animateFloatAsState(if (scrolled) 1f else 0f, motionOr(Motion.fade()), label = "divider")
    Column(modifier.fillMaxSize().statusBarsPadding().imePadding(), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(
            Modifier.widthIn(max = ContentMaxWidth).fillMaxWidth().height(56.dp).padding(horizontal = Space.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconAction(R.drawable.ic_approved_arrow_left, stringResource(R.string.back), { navigator.back() }, tint = palette.text)
            FbText(title, Modifier.weight(1f).padding(horizontal = Space.xs).semantics { heading() }, style = Fb.type.title, maxLines = 1)
            actions()
        }
        Box(Modifier.fillMaxWidth().height(1.dp).graphicsLayer { alpha = divider }.background(palette.border))
        LazyColumn(
            Modifier.widthIn(max = ContentMaxWidth).fillMaxWidth().weight(1f),
            state = state,
            contentPadding = PaddingValues(start = Space.gutter, end = Space.gutter, top = Space.lg, bottom = Space.xxxl),
            verticalArrangement = Arrangement.spacedBy(Space.lg),
            content = content,
        )
        if (bottomBar != null) {
            Column(Modifier.widthIn(max = ContentMaxWidth).fillMaxWidth().navigationBarsPadding().padding(horizontal = Space.gutter, vertical = Space.md), content = bottomBar)
        } else {
            Box(Modifier.navigationBarsPadding())
        }
    }
}

/** A screen heading with an optional supporting line. */
@Composable
fun ScreenHeader(title: String, modifier: Modifier = Modifier, subtitle: String? = null) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
        FbText(title, Modifier.semantics { heading() }, style = Fb.type.headline)
        if (subtitle != null) FbText(subtitle, style = Fb.type.body, color = Fb.palette.textMuted)
    }
}

/** A small uppercase label that introduces a group of rows. */
@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    FbText(text.uppercase(), modifier.padding(top = Space.sm).semantics { heading() }, style = Fb.type.eyebrow, color = Fb.palette.textMuted)
}

/** Fades and lifts its content in once, [index] steps after its siblings. */
@Composable
fun Staggered(index: Int, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        kotlinx.coroutines.delay((index * Motion.STAGGER).toLong())
        shown = true
    }
    val progress by animateFloatAsState(if (shown) 1f else 0f, motionOr(Motion.enter()), label = "stagger")
    Box(
        modifier.graphicsLayer {
            alpha = progress
            translationY = (1f - progress) * 16.dp.toPx()
            val scale = 0.985f + 0.015f * progress
            scaleX = scale
            scaleY = scale
        },
    ) { content() }
}
