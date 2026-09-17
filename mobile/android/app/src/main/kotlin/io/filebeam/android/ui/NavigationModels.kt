package io.filebeam.android.ui

enum class DestinationRoute(val route: String) {
    Send("send"), Receive("receive"), Transfers("transfers"), Inbox("inbox"),
    Settings("settings"), Account("account"), TransferDetail("transfer"), NoteViewer("note");

    companion object {
        val primary = listOf(Send, Receive, Transfers, Inbox)
        fun fromLegacy(route: String?): DestinationRoute = when (route?.substringBefore('/')?.lowercase()) {
            "notes", "turbo", "send" -> Send
            "account" -> Account
            "settings" -> Settings
            "receive" -> Receive
            "transfers" -> Transfers
            "inbox" -> Inbox
            else -> Send
        }
    }
}

data class NavigationState(val current: DestinationRoute = DestinationRoute.Send, val backStack: List<DestinationRoute> = emptyList()) {
    fun push(destination: DestinationRoute): NavigationState =
        if (destination == current) this else copy(current = destination, backStack = backStack + current)
    fun back(): NavigationState = backStack.lastOrNull()?.let { copy(current = it, backStack = backStack.dropLast(1)) } ?: this
    fun selectPrimary(destination: DestinationRoute): NavigationState =
        if (destination == current && backStack.isEmpty()) this else NavigationState(destination)
}

/** Saved route state may contain only opaque IDs, never URLs, secrets, draft text, or keys. */
data class RouteSavedState(val route: DestinationRoute, val transferId: String? = null, val noteId: String? = null)
