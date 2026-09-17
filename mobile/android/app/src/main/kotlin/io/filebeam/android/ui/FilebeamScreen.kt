package io.filebeam.android.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AccountCircle
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.FilebeamMark

/** Window information supplied to Transfers and Inbox when they opt into a list/detail pane. */
data class FilebeamAdaptivePane(val rail: Boolean, val listDetail: Boolean)
val LocalFilebeamAdaptivePane = compositionLocalOf { FilebeamAdaptivePane(false, false) }

@Composable
fun FilebeamScreen(
    model: FilebeamViewModel,
    start: (() -> Unit) -> Unit,
    pickFiles: () -> Unit,
    pickTree: () -> Unit,
    saveFile: (String) -> Unit,
    exportAccountKey: () -> Unit,
    importAccountKey: () -> Unit,
) {
    val config = model.settings.collectAsStateWithLifecycle().value
    val state = model.transfers.collectAsStateWithLifecycle().value
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(state.message, model.draftRestoreError) {
        val message = state.message ?: model.draftRestoreError
        message?.let {
            snackbar.showSnackbar(it)
            if (state.message != null) model.coordinator.message(null)
        }
    }
    BackHandler(enabled = model.navigation.backStack.isNotEmpty()) { model.back() }
    FilebeamTheme(config.dynamicColor) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val wide = maxWidth >= 600.dp
            CompositionLocalProvider(LocalFilebeamAdaptivePane provides FilebeamAdaptivePane(rail = wide, listDetail = maxWidth >= 840.dp)) {
                Row(Modifier.fillMaxSize()) {
                    if (wide) DestinationNavigation(model.navigation.current, model::navigate, rail = true)
                    Scaffold(
                        modifier = Modifier.weight(1f),
                        contentWindowInsets = WindowInsets(0, 0, 0, 0),
                        snackbarHost = { SnackbarHost(snackbar) },
                        topBar = { FilebeamTopBar(model) },
                        bottomBar = { if (!wide) DestinationNavigation(model.navigation.current, model::navigate, rail = false) },
                    ) { padding ->
                        // Destinations own their scroll and IME behavior; bars already consumed system insets.
                        Box(Modifier.fillMaxSize().padding(padding)) {
                            DestinationContent(model, state, config.instance, start, pickFiles, pickTree, saveFile, exportAccountKey, importAccountKey)
                        }
                    }
                }
            }
        }
        state.snapshot?.prompt?.let { PromptDialog(it, model.coordinator::respond, model.coordinator::respondDirectory, model.coordinator::respondConsent, model.coordinator::pause) }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun FilebeamTopBar(model: FilebeamViewModel) {
    val account = model.accounts.state.collectAsStateWithLifecycle().value
    val nested = model.navigation.backStack.isNotEmpty()
    TopAppBar(
        title = {
            if (nested) Text(routeTitle(model.navigation.current), maxLines = 1)
            else Row(verticalAlignment = Alignment.CenterVertically) {
                FilebeamMark(null, Modifier.padding(end = 8.dp))
                Text(stringResource(R.string.app_name), style = MaterialTheme.typography.titleLarge, maxLines = 1)
            }
        },
        navigationIcon = {
            if (nested) IconButton(onClick = model::back) { Icon(Icons.Filled.ArrowBack, stringResource(R.string.back)) }
        },
        actions = {
            if (!nested) {
                IconButton(onClick = { model.navigate(Destination.Settings) }) { ApprovedIcon(ApprovedIcon.Settings, stringResource(R.string.settings)) }
                IconButton(onClick = { model.navigate(Destination.Account) }) { AccountAvatar((account as? ServiceState.Ready)?.value?.username, stringResource(R.string.account)) }
            }
        },
        windowInsets = TopAppBarDefaults.windowInsets,
    )
}

@Composable
private fun AccountAvatar(username: String?, description: String) {
    androidx.compose.material3.Surface(shape = MaterialTheme.shapes.extraLarge, color = MaterialTheme.colorScheme.secondaryContainer) {
        if (username == null) Icon(Icons.Filled.AccountCircle, description, Modifier.padding(8.dp), MaterialTheme.colorScheme.onSecondaryContainer)
        else Text(username.first().uppercase(), modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp), color = MaterialTheme.colorScheme.onSecondaryContainer, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable
private fun routeTitle(route: DestinationRoute): String = stringResource(when (route) {
    DestinationRoute.Send -> R.string.send
    DestinationRoute.Receive, DestinationRoute.NoteViewer -> R.string.receive
    DestinationRoute.Transfers, DestinationRoute.TransferDetail -> R.string.transfers
    DestinationRoute.Inbox -> R.string.inbox
    DestinationRoute.Settings -> R.string.settings
    DestinationRoute.StorageUsage -> R.string.storage_usage
    DestinationRoute.ReviewTransfers -> R.string.review_transfers
    DestinationRoute.Account -> R.string.account
})

@Composable
private fun DestinationContent(
    model: FilebeamViewModel,
    state: io.filebeam.android.platform.TransferUiState,
    instance: String,
    start: (() -> Unit) -> Unit,
    pickFiles: () -> Unit,
    pickTree: () -> Unit,
    saveFile: (String) -> Unit,
    exportAccountKey: () -> Unit,
    importAccountKey: () -> Unit,
) = Column(Modifier.fillMaxSize()) { when (model.navigation.current) {
    DestinationRoute.Send -> SendScreen(model, state.busy, pickFiles, pickTree, { start(model::upload) })
    DestinationRoute.Receive -> ReceiveScreen(model, state.busy) { start(model::startReceivedDownload) }
    DestinationRoute.Transfers, DestinationRoute.TransferDetail -> TransfersScreen(model, state, start, saveFile) { id ->
        model.retrySavedExport(id) { result -> result.onSuccess(saveFile).onFailure { model.coordinator.message(it.message) } }
    }
    DestinationRoute.Inbox -> InboxDestination(model, instance)
    DestinationRoute.NoteViewer -> ReceiveScreen(model, state.busy) { start(model::startReceivedDownload) }
    DestinationRoute.Settings -> SettingsScreen(model, model::navigate)
    DestinationRoute.StorageUsage -> StorageUsageScreen { model.navigate(Destination.ReviewTransfers) }
    DestinationRoute.ReviewTransfers -> ReviewTransfersScreen(model, start)
    DestinationRoute.Account -> AccountDestination(model, instance, exportAccountKey, importAccountKey)
} }

@Composable
private fun DestinationNavigation(selected: DestinationRoute, navigate: (Destination) -> Unit, rail: Boolean) {
    val fontScale = LocalDensity.current.fontScale
    if (rail) NavigationRail { Destination.primary.forEach { DestinationRailItem(it, selected, navigate) } }
    else BoxWithConstraints {
        val twoByTwo = fontScale > 1.3f && maxWidth < 480.dp
        NavigationBar(Modifier.heightIn(min = compactNavigationHeight(fontScale, twoByTwo))) {
            if (twoByTwo) Column {
                Row(Modifier.weight(1f)) { Destination.primary.take(2).forEach { DestinationBarItem(it, selected, navigate) } }
                Row(Modifier.weight(1f)) { Destination.primary.drop(2).forEach { DestinationBarItem(it, selected, navigate) } }
            } else Destination.primary.forEach { DestinationBarItem(it, selected, navigate) }
        }
    }
}

@Composable
private fun ColumnScope.DestinationRailItem(destination: Destination, selected: DestinationRoute, navigate: (Destination) -> Unit) {
    val icon: @Composable () -> Unit = { ApprovedIcon(destination.icon(), null) }
    val label: @Composable () -> Unit = { Text(stringResource(destination.label), style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center, maxLines = 2) }
    NavigationRailItem(selected = destination.routeDestination() == selected, onClick = { navigate(destination) }, icon = icon, label = label)
}

@Composable
private fun RowScope.DestinationBarItem(destination: Destination, selected: DestinationRoute, navigate: (Destination) -> Unit) {
    val icon: @Composable () -> Unit = { ApprovedIcon(destination.icon(), null) }
    val label: @Composable () -> Unit = { Text(stringResource(destination.label), style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center, maxLines = 2) }
    NavigationBarItem(selected = destination.routeDestination() == selected, onClick = { navigate(destination) }, icon = icon, label = label, alwaysShowLabel = true)
}

private fun Destination.icon() = when (this) {
    Destination.Send -> ApprovedIcon.Upload
    Destination.Receive -> ApprovedIcon.Download
    Destination.Transfers -> ApprovedIcon.Transfers
    Destination.Inbox -> ApprovedIcon.Archive
    else -> ApprovedIcon.Settings
}

/** Labels remain visible at accessibility sizes; the bar grows to accommodate their second line. */
internal fun compactNavigationHeight(fontScale: Float, twoByTwo: Boolean = false) = when {
    twoByTwo -> 144.dp
    fontScale > 1.3f -> 112.dp
    else -> 80.dp
}

private fun Destination.routeDestination() = when (this) {
    Destination.Send, Destination.Notes, Destination.Turbo -> DestinationRoute.Send
    Destination.Receive -> DestinationRoute.Receive
    Destination.Transfers -> DestinationRoute.Transfers
    Destination.Inbox -> DestinationRoute.Inbox
    Destination.Settings -> DestinationRoute.Settings
    Destination.StorageUsage -> DestinationRoute.StorageUsage
    Destination.ReviewTransfers -> DestinationRoute.ReviewTransfers
    Destination.Account -> DestinationRoute.Account
}
