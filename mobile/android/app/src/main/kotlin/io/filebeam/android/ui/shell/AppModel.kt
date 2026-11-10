package io.filebeam.android.ui.shell

import android.app.Application
import android.net.Uri
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.filebeam.android.FilebeamApplication
import io.filebeam.android.platform.AppSettings
import io.filebeam.android.platform.services.AccountSummary
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.ui.design.Appearance
import io.filebeam.android.ui.design.ToastMessage
import io.filebeam.android.ui.design.Tone
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

/** Something that arrived from outside the app and still needs a screen to claim it. */
/** A navigation asked for from outside composition, such as an intent. */
data class NavRequest(val tab: Tab? = null, val route: Route? = null, val id: Long = System.nanoTime())

sealed interface Incoming {
    data class Files(val uris: List<Uri>) : Incoming
    data class Text(val text: String) : Incoming
    data class TransferLink(val link: String) : Incoming
    data class Invitation(val token: String) : Incoming
    data class VerifyEmail(val link: String) : Incoming
    data class ResetPassword(val token: String, val email: String?) : Incoming
}

/**
 * App-wide state shared by every screen: appearance, the connected instance, the signed-in
 * account, transient messages and work handed over by intents.
 */
class AppModel(application: Application) : AndroidViewModel(application) {
    val app = application as FilebeamApplication

    // Read once before the first frame so the window never flashes the wrong theme.
    val appearance: StateFlow<Appearance> = app.appearance.appearance
        .stateIn(viewModelScope, SharingStarted.Eagerly, runBlocking { app.appearance.appearance.first() })
    val settings: StateFlow<AppSettings> = app.settings.values
        .stateIn(viewModelScope, SharingStarted.Eagerly, runBlocking { app.settings.values.first() })
    val account: StateFlow<AccountSummary?> = app.accounts.state
        .map { (it as? ServiceState.Ready)?.value }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)
    val instance: StateFlow<InstanceProfile> = app.instances.profile

    private val toastState = MutableStateFlow<ToastMessage?>(null)
    val toast = toastState.asStateFlow()
    private val incomingState = MutableStateFlow<Incoming?>(null)
    val incoming = incomingState.asStateFlow()
    private val navigationState = MutableStateFlow<NavRequest?>(null)
    val navigation = navigationState.asStateFlow()
    private val unreadState = MutableStateFlow(0)
    val inboxUnread = unreadState.asStateFlow()

    init {
        viewModelScope.launch {
            settings.collect { app.instances.refresh(it.instance) }
        }
        // Keeps the session, automatic receiving and the unread badge fresh while the app is open.
        viewModelScope.launch(Dispatchers.IO) {
            while (true) {
                runCatching {
                    app.accounts.resume(settings.value.instance)
                    app.accounts.syncAutomaticReceiving()
                }
                refreshUnread()
                delay(60_000)
            }
        }
        viewModelScope.launch { account.collect { refreshUnread() } }
        viewModelScope.launch {
            app.transfers.state.collect { state ->
                state.message?.let {
                    toast(it, Tone.Neutral)
                    app.transfers.message(null)
                }
            }
        }
    }

    fun toast(text: String, tone: Tone = Tone.Neutral) {
        toastState.value = ToastMessage(text, tone)
    }

    fun toastDismissed(message: ToastMessage) {
        toastState.update { if (it?.id == message.id) null else it }
    }

    fun setAppearance(value: Appearance) {
        viewModelScope.launch { app.appearance.save(value) }
    }

    fun deliver(value: Incoming) {
        incomingState.value = value
    }

    fun navigate(tab: Tab? = null, route: Route? = null) {
        navigationState.value = NavRequest(tab, route)
    }

    fun navigationHandled(request: NavRequest) {
        navigationState.update { if (it?.id == request.id) null else it }
    }

    /** Hands the pending item to the caller exactly once. */
    fun <T : Incoming> claim(type: Class<T>): T? {
        val value = incomingState.value ?: return null
        if (!type.isInstance(value)) return null
        incomingState.value = null
        return type.cast(value)
    }

    fun refreshUnread() {
        viewModelScope.launch(Dispatchers.IO) {
            val current = account.value
            unreadState.value = if (current == null) 0 else runCatching {
                app.accountSessions.service(current.instance).accountInboxUnreadCount().toInt()
            }.getOrDefault(unreadState.value)
        }
    }
}

/** Platform services only an Activity can provide, such as document pickers. */
interface HostActions {
    fun pickFiles(onResult: (List<Uri>) -> Unit)
    fun pickFolder(onResult: (Uri?) -> Unit)
    fun createDocument(suggestedName: String, mimeType: String, onResult: (Uri?) -> Unit)
    fun openDocument(mimeTypes: Array<String>, onResult: (Uri?) -> Unit)

    /** Asks for notification permission if needed, then runs [then] either way. */
    fun withNotifications(then: () -> Unit)
}

val LocalHost = staticCompositionLocalOf<HostActions> { error("No host actions") }
val LocalNavigator = staticCompositionLocalOf<Navigator> { error("No navigator") }
val LocalAppModel = staticCompositionLocalOf<AppModel> { error("No app model") }
