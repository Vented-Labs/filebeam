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
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
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
        Text(
            stringResource(if (notesSelected) R.string.send_note_title else R.string.send_title),
            style = MaterialTheme.typography.headlineMedium,
        )
        Text(
            stringResource(if (notesSelected) R.string.send_note_description else R.string.send_description),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
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
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = FilebeamSpace.Gutter), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Large)) {
            header()
            FileComposer(model, busy, policy, pick, pickTree)
        }
        FileSendDock(model, busy, send, Modifier.padding(horizontal = FilebeamSpace.Gutter))
    }
}
