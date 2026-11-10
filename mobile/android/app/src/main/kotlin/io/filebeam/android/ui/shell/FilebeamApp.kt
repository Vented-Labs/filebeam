package io.filebeam.android.ui.shell

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.ContentTransform
import kotlinx.coroutines.flow.first
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ClipOp
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import io.filebeam.android.ui.motion.LocalReduceMotion
import kotlin.math.hypot
import kotlin.math.max
import androidx.compose.animation.ExperimentalSharedTransitionApi
import androidx.compose.animation.SharedTransitionLayout
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import androidx.navigation3.runtime.NavEntry
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import io.filebeam.android.ui.design.AmbientBackground
import io.filebeam.android.ui.design.FilebeamTheme
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.ToastHost
import io.filebeam.android.ui.feature.send.SendScreen
import io.filebeam.android.ui.motion.Motion

/** Forward navigation: the new screen glides in from the trailing edge as the old one recedes. */
private val forward: ContentTransform
    get() = (slideInHorizontally(Motion.enter()) { it / 7 } + fadeIn(Motion.enter(Motion.DIALOG_IN, 40)) + scaleIn(Motion.enter(), initialScale = 0.985f)) togetherWith
        (fadeOut(Motion.exit()) + scaleOut(Motion.exit(), targetScale = 0.985f) + slideOutHorizontally(Motion.exit()) { -it / 14 })

/** Back navigation mirrors [forward]; the predictive gesture scrubs this same transform. */
private val backward: ContentTransform
    get() = (slideInHorizontally(Motion.enter()) { -it / 14 } + fadeIn(Motion.enter(Motion.DIALOG_IN)) + scaleIn(Motion.enter(), initialScale = 0.985f)) togetherWith
        (slideOutHorizontally(Motion.exit(Motion.DIALOG_OUT)) { it / 7 } + fadeOut(Motion.exit(Motion.DIALOG_OUT)) + scaleOut(Motion.exit(Motion.DIALOG_OUT), targetScale = 0.97f))

@OptIn(ExperimentalSharedTransitionApi::class)
@Composable
fun FilebeamApp(model: AppModel, host: HostActions) {
    val appearance by model.appearance.collectAsStateCompat()
    val instance by model.instance.collectAsStateCompat()
    val toast by model.toast.collectAsStateCompat()
    FilebeamTheme(appearance, instance.primaryColor) {
        val navigator = rememberSaveable(saver = Navigator.Saver) { Navigator() }
        val request by model.navigation.collectAsStateCompat()
        // The tour introduces the app once; afterwards it lives under About.
        LaunchedEffect(Unit) {
            if (!model.app.appearance.hasSeenTour.first()) navigator.push(Route.Tour)
        }
        LaunchedEffect(request) {
            val pending = request ?: return@LaunchedEffect
            pending.tab?.let(navigator::select)
            pending.route?.let(navigator::push)
            model.navigationHandled(pending)
        }
        // The whole UI is drawn through a layer so a theme change can freeze the old look and
        // wipe it away in a growing circle from where the user tapped.
        val layer = rememberGraphicsLayer()
        var frozen by remember { mutableStateOf<ImageBitmap?>(null) }
        var origin by remember { mutableStateOf(Offset.Zero) }
        val reveal = remember { Animatable(0f) }
        val reduceMotion = LocalReduceMotion.current
        LaunchedEffect(Unit) {
            model.themeChange.collect { change ->
                if (reduceMotion) {
                    model.applyAppearance(change.appearance)
                    return@collect
                }
                origin = change.origin
                frozen = runCatching { layer.toImageBitmap() }.getOrNull()
                reveal.snapTo(0f)
                model.applyAppearance(change.appearance)
                reveal.animateTo(1f, tween(620, easing = Motion.ease))
                frozen = null
            }
        }
        CompositionLocalProvider(LocalNavigator provides navigator, LocalHost provides host, LocalAppModel provides model) {
            Box(
                Modifier.fillMaxSize().drawWithContent {
                    layer.record { this@drawWithContent.drawContent() }
                    drawLayer(layer)
                },
            ) {
                AmbientBackground {
                    SharedTransitionLayout {
                        NavDisplay(
                            backStack = navigator.backStack,
                            onBack = { navigator.back() },
                            entryDecorators = listOf(rememberSaveableStateHolderNavEntryDecorator(), rememberViewModelStoreNavEntryDecorator()),
                            sharedTransitionScope = this,
                            transitionSpec = { forward },
                            popTransitionSpec = { backward },
                            predictivePopTransitionSpec = { backward },
                            entryProvider = { route -> NavEntry(route) { RouteContent(route) } },
                        )
                    }
                    ToastHost(toast, model::toastDismissed, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = if (navigator.atRoot) NavBarClearance else Space.xxl))
                }
            }
            frozen?.let { image ->
                Canvas(Modifier.fillMaxSize()) {
                    val radius = hypot(max(origin.x, size.width - origin.x), max(origin.y, size.height - origin.y)) * reveal.value
                    clipPath(Path().apply { addOval(Rect(origin, radius)) }, ClipOp.Difference) { drawImage(image) }
                }
            }
        }
    }
}

@Composable
private fun RouteContent(route: Route) {
    when (route) {
        Route.Home -> Home()
        Route.Settings -> io.filebeam.android.ui.feature.settings.SettingsScreen()
        is Route.Auth -> io.filebeam.android.ui.feature.account.AuthScreen(route.mode)
        Route.Account -> io.filebeam.android.ui.feature.account.AccountScreen()
        Route.SecureInbox -> io.filebeam.android.ui.feature.account.SecureInboxScreen()
        Route.Devices -> io.filebeam.android.ui.feature.account.DevicesScreen()
        Route.About -> io.filebeam.android.ui.feature.about.AboutScreen()
        Route.Tour -> io.filebeam.android.ui.feature.about.TourScreen()
        Route.Changelog -> io.filebeam.android.ui.feature.about.ChangelogScreen()
        is Route.Release -> io.filebeam.android.ui.feature.about.ReleaseScreen(route.tag)
        Route.Appearance -> io.filebeam.android.ui.feature.settings.AppearanceScreen()
        Route.Contacts -> io.filebeam.android.ui.feature.contacts.ContactsScreen()
        is Route.Recipient -> io.filebeam.android.ui.feature.contacts.RecipientScreen(route.username)
        Route.History -> io.filebeam.android.ui.feature.transfers.HistoryScreen()
        Route.Storage -> io.filebeam.android.ui.feature.settings.StorageScreen()
        else -> Placeholder(route)
    }
}

/** The root entry: the selected tab cross-fades through, and each tab keeps its own state. */
@Composable
private fun Home() {
    val navigator = LocalNavigator.current
    val model = LocalAppModel.current
    val unread by model.inboxUnread.collectAsStateCompat()
    val holder = rememberSaveableStateHolder()
    Box(Modifier.fillMaxSize()) {
        AnimatedContent(
            navigator.tab,
            transitionSpec = {
                (fadeIn(Motion.enter(Motion.MENU_IN, 60)) + scaleIn(Motion.enter(Motion.MENU_IN, 60), initialScale = 0.98f)) togetherWith fadeOut(Motion.exit(120))
            },
            label = "tab",
        ) { tab ->
            holder.SaveableStateProvider(tab.key) {
                when (tab) {
                    Tab.Send -> SendScreen()
                    Tab.Receive -> io.filebeam.android.ui.feature.receive.ReceiveScreen()
                    Tab.Inbox -> io.filebeam.android.ui.feature.inbox.InboxScreen()
                    Tab.Transfers -> io.filebeam.android.ui.feature.transfers.TransfersScreen()
                }
            }
        }
        FloatingNavBar(
            navigator.tab, navigator::select, unread,
            Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(horizontal = Space.gutter).padding(bottom = 12.dp),
        )
    }
}
