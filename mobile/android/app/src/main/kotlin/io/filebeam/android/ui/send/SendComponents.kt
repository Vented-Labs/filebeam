package io.filebeam.android.ui.send

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selectableGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.unit.dp
import io.filebeam.android.ui.FilebeamViewModel
import io.filebeam.android.R
import io.filebeam.android.ui.RecipientStatus
import io.filebeam.android.ui.SelectedSource
import io.filebeam.android.ui.SendDraft
import io.filebeam.android.ui.SendDiscoveryState
import io.filebeam.android.ui.SendInstancePolicy
import io.filebeam.android.ui.SendLimitStatus
import io.filebeam.android.ui.recipientConflict
import io.filebeam.android.ui.transportForDriver
import io.filebeam.android.ui.design.ActionDock
import io.filebeam.android.ui.design.ApprovedIcon
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.OptionRow
import io.filebeam.android.ui.design.ProductionGroupCard
import io.filebeam.rust.Transport
import kotlin.math.ln
import kotlin.math.pow

@Composable
fun SendModeTabs(notesSelected: Boolean, enabled: Boolean, onSelect: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().semantics { selectableGroup() }, horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
        ModeTab(Modifier.weight(1f), stringResource(R.string.files_tab), !notesSelected, enabled, ApprovedIcon.Folder) { onSelect(false) }
        ModeTab(Modifier.weight(1f), stringResource(R.string.notes_tab), notesSelected, enabled, ApprovedIcon.File) { onSelect(true) }
    }
}

@Composable
private fun ModeTab(modifier: Modifier, label: String, selected: Boolean, enabled: Boolean, icon: ApprovedIcon, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Surface(
        modifier = modifier.heightIn(min = FilebeamSpace.MinimumTouchTarget)
            .clickable(enabled = enabled, role = Role.Tab, onClick = onClick)
            .semantics { this.selected = selected; stateDescription = if (selected) "Selected" else "Not selected" },
        shape = MaterialTheme.shapes.large,
        color = if (selected) colors.surfaceContainerLowest else colors.surfaceContainer,
    ) {
        Row(Modifier.padding(FilebeamSpace.Small), horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall), verticalAlignment = Alignment.CenterVertically) {
            ApprovedIcon(icon, null)
            Text(label, style = MaterialTheme.typography.titleMedium)
        }
    }
}

@Composable
fun TransportCards(selected: Transport, enabled: (Transport) -> Boolean, onSelect: (Transport) -> Unit) {
    Row(Modifier.fillMaxWidth().semantics { selectableGroup() }, horizontalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
        TransportCard(Modifier.weight(1f), Transport.HTTP, "HTTP", stringResource(R.string.http_download_later), selected == Transport.HTTP, enabled(Transport.HTTP), onSelect)
        TransportCard(Modifier.weight(1f), Transport.WEB_RTC, "WebRTC", stringResource(R.string.webrtc_live_transfer), selected == Transport.WEB_RTC, enabled(Transport.WEB_RTC), onSelect)
    }
}

@Composable
private fun TransportCard(modifier: Modifier, transport: Transport, title: String, detail: String, selected: Boolean, enabled: Boolean, onSelect: (Transport) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Surface(
        modifier = modifier.heightIn(min = 72.dp).clickable(enabled = enabled, role = Role.RadioButton) { onSelect(transport) }
            .semantics { this.selected = selected; stateDescription = if (selected) "Selected" else "Not selected" },
        shape = MaterialTheme.shapes.large,
        color = if (selected) colors.primaryContainer else colors.surfaceContainerLow,
        border = BorderStroke(1.dp, if (selected) colors.primary else colors.outlineVariant),
    ) {
        Column(Modifier.padding(FilebeamSpace.Small), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                ApprovedIcon(if (transport == Transport.HTTP) ApprovedIcon.Storage else ApprovedIcon.Upload, null)
                Text(title, Modifier.padding(start = FilebeamSpace.XSmall).weight(1f), style = MaterialTheme.typography.titleMedium)
                if (selected) Text("✓", style = MaterialTheme.typography.titleMedium)
            }
            Text(detail, style = MaterialTheme.typography.bodySmall, color = if (selected) colors.onPrimaryContainer else colors.onSurfaceVariant)
        }
    }
}

@Composable
fun FileComposer(model: FilebeamViewModel, busy: Boolean, policy: SendInstancePolicy?, pick: () -> Unit, pickTree: () -> Unit) {
    var optionsOpen by remember { mutableStateOf(false) }
    val draft = model.sendDraft
    val conflict = draft.recipientConflict()
    Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
        if (draft.sources.isEmpty()) EmptyFiles(busy, pick, pickTree) else SelectedFiles(draft.sources, busy, model::removeFile, pick, pickTree)
        TransferOptionsCard(draft, !busy && policy != null) { optionsOpen = true }
        RecipientCard(model, draft, busy, conflict)
        if (conflict != null) Text(conflict, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
    }
    if (optionsOpen && policy != null) TransferOptionsSheet(draft, policy, { transform -> model.updateSendFromUi(transform) }) { optionsOpen = false }
}

@Composable
fun FileSendDock(model: FilebeamViewModel, busy: Boolean, send: () -> Unit, modifier: Modifier = Modifier) {
    val draft = model.sendDraft
    val enabled = !busy && draft.sources.isNotEmpty() && draft.recipientConflict() == null && validRetention(draft.retentionHours) && recipientIsReady(draft) && model.canSubmitFiles()
    Column(modifier.fillMaxWidth().padding(top = FilebeamSpace.Small)) {
        Text(stringResource(R.string.encrypted_on_device), Modifier.align(Alignment.CenterHorizontally), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
        ActionDock(stringResource(R.string.encrypt_and_share), enabled, send, Modifier.padding(vertical = FilebeamSpace.Small))
    }
}

@Composable
fun SendPolicyStatus(discovery: SendDiscoveryState?, limit: SendLimitStatus?) {
    val message = when (discovery) {
        null, SendDiscoveryState.Loading -> stringResource(R.string.policy_loading)
        is SendDiscoveryState.Failed -> discovery.message
        is SendDiscoveryState.Ready -> when (limit) {
            is SendLimitStatus.Exceeds -> limit.reason
            SendLimitStatus.UnknownSize -> stringResource(R.string.policy_size_pending)
            else -> stringResource(R.string.policy_ready)
        }
    }
    Text(message, color = if (discovery is SendDiscoveryState.Failed || limit is SendLimitStatus.Exceeds) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
}

@Composable
private fun EmptyFiles(busy: Boolean, pick: () -> Unit, pickTree: () -> Unit) = ProductionGroupCard {
    Column(Modifier.padding(FilebeamSpace.Large), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Small)) {
        ApprovedIcon(ApprovedIcon.Folder, null)
        Text(stringResource(R.string.choose_files_to_send), style = MaterialTheme.typography.titleLarge)
        Text(stringResource(R.string.original_quality_encrypted), color = MaterialTheme.colorScheme.onSurfaceVariant)
        ActionDock(stringResource(R.string.choose_files), !busy, pick, icon = ApprovedIcon.Add)
        OutlinedButton(enabled = !busy, onClick = pickTree) { ApprovedIcon(ApprovedIcon.Folder, null); Text(stringResource(R.string.choose_folder), Modifier.padding(start = FilebeamSpace.XSmall)) }
    }
}

@Composable
private fun SelectedFiles(sources: List<SelectedSource>, busy: Boolean, remove: (android.net.Uri) -> Unit, pick: () -> Unit, pickTree: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(stringResource(R.string.your_files, sources.size), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        OutlinedButton(enabled = !busy, onClick = pick) { Text(stringResource(R.string.add_more)) }
    }
    ProductionGroupCard {
        sources.forEach { source -> SelectedFileRow(source, !busy) { remove(source.uri) } }
    }
    OutlinedButton(enabled = !busy, onClick = pickTree, modifier = Modifier.fillMaxWidth().heightIn(min = FilebeamSpace.MinimumTouchTarget)) { ApprovedIcon(ApprovedIcon.Folder, null); Text(stringResource(R.string.choose_folder), Modifier.padding(start = FilebeamSpace.XSmall)) }
}

@Composable
private fun SelectedFileRow(source: SelectedSource, enabled: Boolean, remove: () -> Unit) {
    val removeLabel = stringResource(R.string.remove_file, source.displayName)
    Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small), verticalAlignment = Alignment.CenterVertically) {
        ApprovedIcon(ApprovedIcon.File, null)
        Column(Modifier.padding(start = FilebeamSpace.Small).weight(1f)) {
            Text(source.displayName, style = MaterialTheme.typography.bodyLarge)
            Text(source.error ?: source.sizeBytes?.let(::formatFileSize) ?: stringResource(R.string.size_unavailable), color = if (source.error == null) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            source.relativePath?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodySmall) }
        }
        IconButton(enabled = enabled, onClick = remove, modifier = Modifier.semantics { contentDescription = removeLabel }) { Text("×", style = MaterialTheme.typography.headlineSmall) }
    }
}

@Composable
private fun TransferOptionsCard(draft: SendDraft, enabled: Boolean, onClick: () -> Unit) = ProductionGroupCard {
    val summary = listOf(
        draft.retentionHours.takeIf(String::isNotBlank)?.let { "$it hours" } ?: stringResource(R.string.default_retention),
        stringResource(if (draft.passwordProtected) R.string.password_on else R.string.password_off),
        stringResource(if (draft.archive) R.string.zip_on else R.string.zip_off),
    ).joinToString(" · ")
    OptionRow(stringResource(R.string.transfer_options), summary, ApprovedIcon.Settings, enabled = enabled, onClick = onClick)
}

@Composable
private fun RecipientCard(model: FilebeamViewModel, draft: SendDraft, busy: Boolean, conflict: String?) = ProductionGroupCard {
    var expanded by remember { mutableStateOf(draft.recipient.username.isNotBlank()) }
    OptionRow(stringResource(R.string.deliver_to_username), if ((conflict ?: draft.recipient.error) != null) conflict ?: draft.recipient.error else stringResource(R.string.recipient_optional), ApprovedIcon.Shield, enabled = !busy, onClick = { expanded = !expanded })
    if (expanded) {
        androidx.compose.material3.OutlinedTextField(
            draft.recipient.username,
            { model.recipientUsername = it },
            label = { Text(stringResource(R.string.username)) },
            supportingText = { Text(recipientSupport(draft.recipient.status)) },
            isError = conflict != null || draft.recipient.status == RecipientStatus.INVALID,
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small),
        )
        OutlinedButton(enabled = !busy && draft.recipient.username.isNotBlank() && conflict == null, onClick = model::validateRecipient, modifier = Modifier.padding(horizontal = FilebeamSpace.Medium, vertical = FilebeamSpace.Small)) { Text(stringResource(R.string.validate_recipient)) }
    }
}

internal fun FilebeamViewModel.updateSendFromUi(transform: (SendDraft) -> SendDraft) {
    val edited = transform(sendDraft)
    val selected = edited.driver
    transport = selected?.let(::transportForDriver) ?: edited.transport
    archive = edited.archive
    turboTransfer = edited.turbo
    passwordProtected = edited.passwordProtected
    retentionHours = edited.retentionHours
    setFileIncludeKey(edited.includeKeyInLink)
    setFileDriver(selected)
}

internal fun transferOptionsSummary(draft: SendDraft): String = listOf(
    draft.retentionHours.takeIf(String::isNotBlank)?.let { "$it hours" } ?: "Default retention",
    if (draft.passwordProtected) "Password on" else "Password off",
    if (draft.archive) "ZIP on" else "ZIP off",
).joinToString(" · ")

internal fun validRetention(value: String): Boolean = io.filebeam.android.ui.retentionHours(value).isSuccess
internal fun recipientIsReady(draft: SendDraft): Boolean = draft.recipient.username.isBlank() || draft.recipient.status == RecipientStatus.VALIDATED
@Composable
internal fun recipientSupport(status: RecipientStatus): String = stringResource(when (status) {
    RecipientStatus.NONE, RecipientStatus.UNVALIDATED -> R.string.recipient_validate
    RecipientStatus.VALIDATING -> R.string.recipient_validating
    RecipientStatus.VALIDATED -> R.string.recipient_validated
    RecipientStatus.INVALID -> R.string.recipient_invalid
    RecipientStatus.CONFLICT -> R.string.recipient_conflict
})
internal fun formatFileSize(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val unit = (ln(bytes.toDouble()) / ln(1024.0)).toInt().coerceAtMost(4)
    return "%.1f %s".format(bytes / 1024.0.pow(unit), arrayOf("B", "KB", "MB", "GB", "TB")[unit])
}
