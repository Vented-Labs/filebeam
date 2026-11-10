package io.filebeam.android.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.filebeam.android.R
import io.filebeam.android.ui.design.EmptyState

/** Temporary stand-ins while a destination is being built. */
@Composable
internal fun PlaceholderTab(tab: Tab) {
    TabScreen { item { EmptyState(tab.icon, stringResource(tab.label)) } }
}

@Composable
internal fun Placeholder(route: Route) {
    SubScreen(route.key) { item { EmptyState(R.drawable.ic_approved_info, route.key) } }
}
