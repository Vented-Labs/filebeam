package io.filebeam.android.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R

@Composable
fun FilebeamScreen(
    model: FilebeamViewModel,
    start: (() -> Unit) -> Unit,
    pickFiles: () -> Unit,
    pickTree: () -> Unit,
    saveFile: (String) -> Unit,
) {
    val config = model.settings.collectAsStateWithLifecycle().value
    val state = model.transfers.collectAsStateWithLifecycle().value
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(state.message) {
        state.message?.let { snackbar.showSnackbar(it); model.coordinator.message(null) }
    }
    BackHandler(model.destination != Destination.Send) { model.destination = Destination.Send }
    FilebeamTheme(config.dynamicColor) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val wide = maxWidth >= 600.dp
            Row(Modifier.fillMaxSize()) {
                if (wide) DestinationNavigation(model.destination, model::navigate, true)
                Scaffold(
                    modifier = Modifier.weight(1f),
                    snackbarHost = { SnackbarHost(snackbar) },
                    bottomBar = { if (!wide) DestinationNavigation(model.destination, model::navigate, false) },
                ) { padding ->
                    Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 20.dp)) {
                        when (model.destination) {
                            Destination.Send -> SendScreen(model, state.busy, pickFiles, pickTree, { start(model::upload) })
                            Destination.Receive -> ReceiveScreen(model, state.busy) { start(model::download) }
                            Destination.Transfers -> TransfersScreen(model, state, start, saveFile)
                            Destination.Settings -> SettingsScreen(config, model::saveSettings, model.coordinator::checkInstance, model::navigate)
                            Destination.Notes -> NotesScreen(model, config.instance)
                            Destination.Inbox -> InboxScreen(model, config.instance)
                            Destination.Turbo -> TurboScreen(model, config.instance)
                            Destination.Account -> AccountScreen(model, config.instance)
                        }
                    }
                }
            }
        }
        state.snapshot?.prompt?.let { PromptDialog(it, model.coordinator::respond, model.coordinator::pause) }
    }
}

@Composable
private fun DestinationNavigation(selected: Destination, navigate: (Destination) -> Unit, rail: Boolean) {
    if (rail) NavigationRail {
        Spacer(Modifier.windowInsetsTopHeight(WindowInsets.statusBars))
        DestinationItems(selected, navigate, true)
    } else NavigationBar {
        DestinationItems(selected, navigate, false)
    }
}

@Composable
private fun DestinationItems(selected: Destination, navigate: (Destination) -> Unit, rail: Boolean) {
    Destination.primary.forEach { destination ->
        val item: @Composable () -> Unit = { Icon(destinationIcon(destination), contentDescription = null) }
        if (rail) NavigationRailItem(
                selected = selected == destination,
                onClick = { navigate(destination) },
                icon = item,
                label = { Text(destinationLabel(destination)) },
        ) else NavigationRailItem(
                selected = selected == destination,
                onClick = { navigate(destination) },
                icon = item,
                label = { Text(destinationLabel(destination)) },
        )
    }
}

@Composable private fun destinationLabel(destination: Destination) = stringResource(destination.label)

private fun destinationIcon(destination: Destination) = when (destination) {
    Destination.Send -> Icons.Default.Add
    Destination.Receive -> Icons.AutoMirrored.Filled.ArrowBack
    Destination.Transfers -> Icons.AutoMirrored.Filled.List
    else -> Icons.Default.Settings
}
