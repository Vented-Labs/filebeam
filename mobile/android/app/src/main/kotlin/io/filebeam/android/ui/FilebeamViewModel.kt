package io.filebeam.android.ui

import android.app.Application
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.filebeam.android.FilebeamApplication
import io.filebeam.android.R
import io.filebeam.android.platform.AppSettings
import io.filebeam.android.platform.UploadRequest
import io.filebeam.rust.Transport
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

enum class Destination(val label: Int) {
    Send(R.string.send), Receive(R.string.receive), Transfers(R.string.transfers),
    Notes(R.string.notes), Inbox(R.string.inbox), Turbo(R.string.turbo),
    Account(R.string.account), Settings(R.string.settings);
    companion object { val primary = listOf(Send, Receive, Transfers, Settings) }
}

class FilebeamViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as FilebeamApplication
    val coordinator = app.transfers
    val accounts = app.accounts
    val notes = app.notes
    val turbo = app.turbo
    val inbox = app.inbox
    val transfers = coordinator.state
    val settings = app.settings.values.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AppSettings())
    var destination by mutableStateOf(Destination.Send)
    var selectedFiles by mutableStateOf(emptyList<Uri>())
        private set
    var link by mutableStateOf("")
    var transport by mutableStateOf(Transport.HTTP)
    var archive by mutableStateOf(false)
    var passwordProtected by mutableStateOf(false)
    var retentionHours by mutableStateOf("")
    var exportSource: String? = null

    fun selectFiles(uris: List<Uri>) { selectedFiles = uris; destination = Destination.Send }
    fun receiveLink(value: String) { link = value; destination = Destination.Receive }
    fun navigate(value: Destination) { destination = value }
    fun upload() {
        coordinator.upload(UploadRequest(
            uris = selectedFiles,
            transport = transport,
            archive = archive,
            passwordProtected = passwordProtected,
            retentionHours = retentionHours.toULongOrNull(),
        ))
        destination = Destination.Transfers
    }
    fun download() { coordinator.download(link.trim()); destination = Destination.Transfers }
    fun saveSettings(value: AppSettings) { viewModelScope.launch { app.settings.save(value) } }
}
