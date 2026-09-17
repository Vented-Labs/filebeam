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
import androidx.compose.foundation.layout.safeDrawing
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
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
                        contentWindowInsets = WindowInsets.safeDrawing,
                        snackbarHost = { SnackbarHost(snackbar) },
                        topBar = { FilebeamTopBar(model) },
                        bottomBar = { if (!wide) DestinationNavigation(model.navigation.current, model::navigate, rail = false) },
                    ) { padding ->
                        // Destinations own their scroll and IME behavior; this root applies system insets once.
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
private fun FilebeamTopBar(model: FilebeamViewModel) {
    val account = model.accounts.state.collectAsStateWithLifecycle().value
    Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        if (model.navigation.backStack.isNotEmpty()) {
            IconButton(onClick = model::back) { Text("Back") }
        } else {
            FilebeamMark(stringResource(R.string.app_name))
            Text(stringResource(R.string.app_name), style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(start = 8.dp))
        }
        Box(Modifier.weight(1f))
        IconButton(onClick = { model.navigate(Destination.Settings) }) {
            ApprovedIcon(ApprovedIcon.Settings, stringResource(R.string.settings))
        }
        IconButton(onClick = { model.navigate(Destination.Account) }) {
            val identity = (account as? ServiceState.Ready)?.value?.username
            Text(identity?.take(1)?.uppercase() ?: "Sign in")
        }
    }
}

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
    DestinationRoute.Account -> AccountDestination(model, instance, exportAccountKey, importAccountKey)
} }

@Composable
private fun DestinationNavigation(selected: DestinationRoute, navigate: (Destination) -> Unit, rail: Boolean) {
    if (rail) NavigationRail { Destination.primary.forEach { DestinationRailItem(it, selected, navigate) } }
    else NavigationBar { Destination.primary.forEach { DestinationBarItem(it, selected, navigate) } }
}

@Composable
private fun ColumnScope.DestinationRailItem(destination: Destination, selected: DestinationRoute, navigate: (Destination) -> Unit) {
    val icon: @Composable () -> Unit = { ApprovedIcon(destination.icon(), null) }
    val label: @Composable () -> Unit = { Text(stringResource(destination.label)) }
    NavigationRailItem(selected = destination.routeDestination() == selected, onClick = { navigate(destination) }, icon = icon, label = label)
}

@Composable
private fun RowScope.DestinationBarItem(destination: Destination, selected: DestinationRoute, navigate: (Destination) -> Unit) {
    val icon: @Composable () -> Unit = { ApprovedIcon(destination.icon(), null) }
    val label: @Composable () -> Unit = { Text(stringResource(destination.label)) }
    NavigationBarItem(selected = destination.routeDestination() == selected, onClick = { navigate(destination) }, icon = icon, label = label)
}

private fun Destination.icon() = when (this) {
    Destination.Send -> ApprovedIcon.Upload
    Destination.Receive -> ApprovedIcon.Download
    Destination.Transfers -> ApprovedIcon.Transfers
    Destination.Inbox -> ApprovedIcon.Folder
    else -> ApprovedIcon.Settings
}

private fun Destination.routeDestination() = when (this) {
    Destination.Send, Destination.Notes, Destination.Turbo -> DestinationRoute.Send
    Destination.Receive -> DestinationRoute.Receive
    Destination.Transfers -> DestinationRoute.Transfers
    Destination.Inbox -> DestinationRoute.Inbox
    Destination.Settings -> DestinationRoute.Settings
    Destination.Account -> DestinationRoute.Account
}
