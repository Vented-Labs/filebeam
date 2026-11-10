package io.filebeam.android.ui.shell

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.snapshots.SnapshotStateList

/**
 * Every screen the app can show. Routes carry only opaque identifiers; links, keys and
 * draft text never enter navigation state.
 */
sealed interface Route {
    /** Stable text form used to restore the back stack after process death. */
    val key: String

    /** The root entry, which hosts the selected tab. */
    data object Home : Route { override val key = "home" }

    data object Account : Route { override val key = "account" }
    data class Auth(val mode: String = "login") : Route { override val key = "auth/$mode" }
    data object SecureInbox : Route { override val key = "secure-inbox" }
    data object Contacts : Route { override val key = "contacts" }
    data object Devices : Route { override val key = "devices" }
    data object History : Route { override val key = "history" }
    data object Appearance : Route { override val key = "appearance" }
    data object Settings : Route { override val key = "settings" }
    data object Storage : Route { override val key = "storage" }
    data object About : Route { override val key = "about" }
    data object Tour : Route { override val key = "tour" }
    data object Changelog : Route { override val key = "changelog" }
    data class Release(val tag: String) : Route { override val key = "release/$tag" }
    data object StagedInbox : Route { override val key = "staged" }
    data class InboxItem(val id: String) : Route { override val key = "inbox-item/$id" }
    data class Recipient(val username: String) : Route { override val key = "recipient/$username" }
    data object Gallery : Route { override val key = "gallery" }

    companion object {
        fun parse(key: String): Route? {
            val head = key.substringBefore('/')
            val tail = key.substringAfter('/', "")
            return when (head) {
                "home" -> Home
                "account" -> Account
                "auth" -> Auth(tail.ifEmpty { "login" })
                "secure-inbox" -> SecureInbox
                "contacts" -> Contacts
                "devices" -> Devices
                "history" -> History
                "appearance" -> Appearance
                "settings" -> Settings
                "storage" -> Storage
                "about" -> About
                "tour" -> Tour
                "changelog" -> Changelog
                "release" -> tail.takeIf(String::isNotEmpty)?.let(::Release)
                "staged" -> StagedInbox
                "inbox-item" -> tail.takeIf(String::isNotEmpty)?.let(::InboxItem)
                "recipient" -> tail.takeIf(String::isNotEmpty)?.let(::Recipient)
                "gallery" -> Gallery
                else -> null
            }
        }
    }
}

/** The four primary destinations shown in the navigation bar. They live inside the root entry. */
enum class Tab(val key: String) { Send("send"), Receive("receive"), Transfers("transfers"), Inbox("inbox") }

/**
 * Owns navigation state. The root of the stack is always [Route.Home], which hosts the
 * selected [tab]; every other route stacks above it.
 */
@Stable
class Navigator(initialTab: Tab = Tab.Send, initial: List<Route> = emptyList()) {
    var tab by mutableStateOf(initialTab)
        private set
    val backStack: SnapshotStateList<Route> = mutableStateListOf<Route>().apply {
        add(Route.Home)
        addAll(initial.filter { it != Route.Home })
    }

    val current: Route get() = backStack.last()
    val atRoot: Boolean get() = backStack.size == 1

    /** Switches tabs, dropping anything stacked above the root. */
    fun select(target: Tab) {
        tab = target
        while (backStack.size > 1) backStack.removeAt(backStack.lastIndex)
    }

    fun push(route: Route) {
        if (route == Route.Home || current == route) return
        backStack.remove(route)
        backStack.add(route)
    }

    /** Replaces the top route, e.g. sign-in giving way to the account screen. */
    fun replace(route: Route) {
        if (backStack.size > 1) backStack.removeAt(backStack.lastIndex)
        push(route)
    }

    /** Returns false at the root so the system can handle back. */
    fun back(): Boolean {
        if (backStack.size <= 1) return false
        backStack.removeAt(backStack.lastIndex)
        return true
    }

    companion object {
        val Saver: Saver<Navigator, Any> = listSaver(
            save = { navigator -> listOf(navigator.tab.key) + navigator.backStack.drop(1).map(Route::key) },
            restore = { keys ->
                Navigator(Tab.entries.firstOrNull { it.key == keys.firstOrNull() } ?: Tab.Send, keys.drop(1).mapNotNull(Route::parse))
            },
        )
    }
}
