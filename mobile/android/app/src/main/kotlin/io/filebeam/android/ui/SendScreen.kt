package io.filebeam.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import android.os.Build
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.send.FileComposer
import io.filebeam.android.ui.send.FileSendDock
import io.filebeam.android.ui.send.SendModeTabs
import io.filebeam.android.ui.send.TransportCards
import io.filebeam.android.ui.send.SendPolicyStatus

/** The Send destination owns only visual composition; drafts and transfer lifecycle remain model-owned. */
@Composable
fun SendScreen(model: FilebeamViewModel, busy: Boolean, pick: () -> Unit, pickTree: () -> Unit, send: () -> Unit) {
    val notesSelected = model.sendContent == SendContent.NOTES
    val instance = model.settings.collectAsStateWithLifecycle().value.instance
    val discovery = model.activeSendDiscovery
    val policy = (discovery as? SendDiscoveryState.Ready)?.policy
    val header: @Composable () -> Unit = {
        Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
            Text(stringResource(if (notesSelected) R.string.send_note_title else R.string.send_title), style = MaterialTheme.typography.headlineMedium)
            Text(stringResource(if (notesSelected) R.string.send_note_description else R.string.send_description), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        // Composer navigation is safe while work runs; changing it does not control the job.
        SendModeTabs(notesSelected, enabled = true) { model.showSend(if (it) SendContent.NOTES else SendContent.FILES) }
        TransportCards(
            selected = if (notesSelected && model.noteDraft.live) io.filebeam.rust.Transport.WEB_RTC else model.transport,
            enabled = { transport -> !busy && policy?.enabledTransports?.contains(transport) == true },
            onSelect = { transport ->
                if (notesSelected) model.updateNote { it.copy(live = transport == io.filebeam.rust.Transport.WEB_RTC) }
                else model.transport = transport
            },
        )
        SendPolicyStatus(discovery, if (notesSelected) policy?.let { model.noteDraft.limitStatus(it) } else policy?.let { model.sendDraft.limitStatus(it) })
    }
    if (notesSelected) {
        NotesComposer(model, instance, busy, header)
    } else Column(Modifier.fillMaxSize().imePadding()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
            header()
            FileComposer(model, busy, policy, pick, pickTree)
            DeviceRecipientSection(model)
            androidx.compose.material3.OutlinedButton(enabled = !busy, onClick = { model.updateAttachedNote(if (model.sendDraft.attachedNote == null) NoteDraft() else null) }) {
                Text(stringResource(if (model.sendDraft.attachedNote == null) R.string.attach_note else R.string.remove_attached_note))
            }
            model.sendDraft.attachedNote?.let { draft ->
                Text(stringResource(R.string.attached_note_lifetime), style = MaterialTheme.typography.bodySmall)
                io.filebeam.android.ui.send.NoteEditor(draft.title, draft.body, draft.language, !busy,
                    { model.updateAttachedNote(draft.copy(title = it)) }, { model.updateAttachedNote(draft.copy(body = it)) }, { model.updateAttachedNote(draft.copy(language = it)) }, attached = true)
                Text(stringResource(R.string.attached_note_size, draft.body.toByteArray(Charsets.UTF_8).size), style = MaterialTheme.typography.bodySmall)
                draft.attachmentError()?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        }
        FileSendDock(model, busy, send, Modifier.padding(horizontal = FilebeamSpace.Gutter))
    }
}

@Composable
private fun DeviceRecipientSection(model: FilebeamViewModel) {
    val directory = model.deviceDirectory
    val clipboard = LocalClipboardManager.current
    var name by remember { mutableStateOf(Build.MODEL.ifBlank { "Android device" }) }
    var pairingPayload by remember { mutableStateOf("") }
    var pairingFingerprint by remember { mutableStateOf("") }
    var approverFingerprint by remember { mutableStateOf("") }
    Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
        Text("My devices", style = MaterialTheme.typography.titleMedium)
        Text("Registered devices receive through Filebeam when their local runtime is available.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (directory == null) {
            Text(model.deviceDirectoryError ?: "Checking registered devices...", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            directory.devices.forEach { device ->
                androidx.compose.material3.OutlinedButton(enabled = device.trust == io.filebeam.android.platform.devices.DeviceTrust.VERIFIED && device.canReceive && device.id != directory.currentDeviceId, onClick = { model.selectDeviceTarget(if (model.deviceTargetId == device.id) null else device.id) }) {
                    Text("${if (model.deviceTargetId == device.id) "Selected: " else ""}${device.name} · ${device.availability.name.lowercase().replace('_', ' ')}")
                }
            }
            androidx.compose.material3.Button(enabled = model.deviceTargetId != null && model.sendDraft.sources.size == 1, onClick = model::sendToDevice) { Text("Send selected document to device") }
            if (directory.stale) Text(directory.detail ?: "Device status may be stale.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
        }
        androidx.compose.material3.OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text("This device name") }, singleLine = true)
        androidx.compose.material3.OutlinedButton(onClick = { model.enrollThisDevice(name, "Android") }) { Text("Register this device") }
        androidx.compose.material3.OutlinedButton(onClick = { model.pairingPayload { clipboard.setText(AnnotatedString(it)) } }) { Text("Copy pairing payload") }
        androidx.compose.material3.OutlinedTextField(value = pairingPayload, onValueChange = { pairingPayload = it }, label = { Text("Paste pairing payload") }, minLines = 3)
        androidx.compose.material3.OutlinedTextField(value = pairingFingerprint, onValueChange = { pairingFingerprint = it }, label = { Text("Confirm candidate key fingerprint") }, singleLine = true)
        androidx.compose.material3.OutlinedButton(enabled = pairingPayload.isNotBlank() && pairingFingerprint.isNotBlank(), onClick = { model.approvePairing(pairingPayload, pairingFingerprint) }) { Text("Approve and pin device") }
        androidx.compose.material3.OutlinedTextField(value = approverFingerprint, onValueChange = { approverFingerprint = it }, label = { Text("Confirm approving device fingerprint") }, singleLine = true)
        androidx.compose.material3.OutlinedButton(enabled = approverFingerprint.isNotBlank(), onClick = { model.confirmApprover(approverFingerprint) }) { Text("Confirm approver and pin") }
        Text("Compare the candidate fingerprint out of band before approving. Approval pins only after authenticated registry sync.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        model.deviceOperationError?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
        androidx.compose.material3.OutlinedButton(onClick = model::refreshDevices) { Text("Refresh devices") }
    }
}
