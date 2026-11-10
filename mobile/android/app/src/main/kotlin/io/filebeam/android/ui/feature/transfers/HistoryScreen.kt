package io.filebeam.android.ui.feature.transfers

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatAbsolute
import io.filebeam.android.ui.common.formatBytes
import io.filebeam.android.ui.common.formatRetention
import io.filebeam.android.ui.common.parseInstant
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbDialog
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.FilterChip
import io.filebeam.android.ui.design.IconAction
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.feature.send.copySensitive
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.Tab
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.HistoryEntry
import io.filebeam.rust.HistoryFilter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.Instant

private val statuses = listOf(
    "pending" to R.string.history_status_pending, "available" to R.string.history_status_available, "live" to R.string.history_status_live,
    "ended" to R.string.history_status_ended, "deleting" to R.string.history_status_deleting, "expired" to R.string.history_status_expired,
    "deleted" to R.string.history_status_deleted, "abandoned" to R.string.history_status_abandoned, "burned" to R.string.history_status_burned,
    "removed" to R.string.history_status_removed,
)

private fun statusTone(status: String) = when (status) {
    "available", "live" -> Tone.Success
    "pending" -> Tone.Accent
    "deleting", "abandoned" -> Tone.Warning
    "expired", "deleted", "burned", "removed" -> Tone.Danger
    else -> Tone.Neutral
}

/** Sent transfers recorded for this account: filter, extend retention, or delete for everyone. */
@Composable
fun HistoryScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val account by app.account.collectAsStateCompat()
    var status by rememberSaveable { mutableStateOf<String?>(null) }
    var kind by rememberSaveable { mutableStateOf<String?>(null) }
    var driver by rememberSaveable { mutableStateOf<String?>(null) }
    var entries by remember { mutableStateOf<List<HistoryEntry>>(emptyList()) }
    var cursor by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(true) }
    var loadingMore by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var revision by remember { mutableIntStateOf(0) }
    var extending by remember { mutableStateOf<HistoryEntry?>(null) }
    var deleting by remember { mutableStateOf<HistoryEntry?>(null) }
    val current = account
    val generic = stringResource(R.string.unknown_error)

    LaunchedEffect(current == null) { if (current == null) navigator.replace(Route.Auth()) }
    if (current == null) return
    val service = remember(current.instance) { app.app.accountSessions.service(current.instance) }

    LaunchedEffect(status, kind, driver, revision) {
        loading = true
        error = null
        runCatching { withContext(Dispatchers.IO) { service.accountHistory(HistoryFilter(status, kind, driver), null, 20u) } }
            .onSuccess {
                entries = it.entries
                cursor = it.nextCursor
            }
            .onFailure { error = it.message ?: generic }
        loading = false
    }

    SubScreen(
        stringResource(R.string.history_title),
        actions = { IconAction(R.drawable.ic_approved_rotate_right, stringResource(R.string.refresh), { revision++ }) },
    ) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(Space.sm)) {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FilterChip(stringResource(R.string.history_all_types), kind == null, { kind = null })
                    FilterChip(stringResource(R.string.send_mode_files), kind == "files", { kind = "files" }, icon = R.drawable.ic_approved_folder)
                    FilterChip(stringResource(R.string.send_mode_notes), kind == "note", { kind = "note" }, icon = R.drawable.ic_approved_note)
                    FilterChip(stringResource(R.string.history_stored), driver == "http", { driver = if (driver == "http") null else "http" })
                    FilterChip(stringResource(R.string.history_live), driver == "webrtc", { driver = if (driver == "webrtc") null else "webrtc" })
                }
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FilterChip(stringResource(R.string.history_any_status), status == null, { status = null })
                    statuses.forEach { (key, label) -> FilterChip(stringResource(label), status == key, { status = key }) }
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    FbIcon(R.drawable.ic_approved_shield, size = 16.dp)
                    FbText(stringResource(R.string.history_privacy), style = Fb.type.caption, color = Fb.palette.textMuted)
                }
            }
        }
        error?.let { message -> item { Notice(message, tone = Tone.Danger) { FbButton(stringResource(R.string.retry), { revision++ }, style = ButtonStyle.Secondary, height = 40.dp) } } }
        when {
            loading && entries.isEmpty() -> item { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
            entries.isEmpty() && error == null -> item {
                EmptyState(R.drawable.ic_approved_clock, stringResource(R.string.history_empty_title), body = stringResource(R.string.history_empty_body)) {
                    FbButton(stringResource(R.string.history_send), { navigator.select(Tab.Send) }, icon = R.drawable.ic_approved_upload)
                }
            }
            else -> {
                items(entries, key = { it.id }) { entry ->
                    HistoryCard(
                        entry, Modifier.animateItem(fadeInSpec = Motion.enter(), placementSpec = Motion.spatial(), fadeOutSpec = Motion.exit()),
                        onCopy = {
                            copySensitive(context, resources.getString(R.string.history_transfer_id), entry.id)
                            app.toast(resources.getString(R.string.copied), Tone.Success)
                        },
                        onExtend = { extending = entry }, onDelete = { deleting = entry },
                    )
                }
                if (cursor != null) {
                    item {
                        FbButton(stringResource(R.string.history_older), {
                            loadingMore = true
                            scope.launch {
                                runCatching { withContext(Dispatchers.IO) { service.accountHistory(HistoryFilter(status, kind, driver), cursor, 20u) } }
                                    .onSuccess {
                                        entries = entries + it.entries.filter { next -> entries.none { have -> have.id == next.id } }
                                        cursor = it.nextCursor
                                    }
                                    .onFailure { error = it.message ?: generic }
                                loadingMore = false
                            }
                        }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary, loading = loadingMore)
                    }
                }
                item { FbText(stringResource(R.string.history_footer), style = Fb.type.caption, color = Fb.palette.textSubtle) }
            }
        }
    }

    extending?.let { entry ->
        var hours by remember(entry.id) { mutableStateOf((entry.retentionHours + 24uL).coerceAtMost(entry.maximumRetentionHours).toString()) }
        var busy by remember { mutableStateOf(false) }
        var failure by remember(entry.id) { mutableStateOf<String?>(null) }
        val value = hours.toULongOrNull()
        val valid = value != null && value > entry.retentionHours && value <= entry.maximumRetentionHours
        val created = parseInstant(entry.createdAt)
        FbDialog(
            { if (!busy) extending = null }, stringResource(R.string.history_extend_title), stringResource(R.string.history_extend),
            {
                busy = true
                failure = null
                scope.launch {
                    runCatching { withContext(Dispatchers.IO) { service.accountHistoryExtend(entry.id, value!!) } }
                        .onSuccess {
                            app.toast(resources.getString(R.string.history_extended), Tone.Success)
                            extending = null
                            revision++
                        }
                        .onFailure { failure = it.message ?: generic }
                    busy = false
                }
            },
            body = stringResource(R.string.history_extend_body, formatRetention(context, entry.retentionHours.toLong()), formatRetention(context, entry.maximumRetentionHours.toLong())),
            dismissLabel = stringResource(R.string.cancel), confirmEnabled = valid, confirmLoading = busy,
        ) {
            FbTextField(
                hours, { hours = it.filter(Char::isDigit).take(6) }, label = stringResource(R.string.history_extend_hours),
                error = failure, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                supporting = if (valid && created != null) stringResource(R.string.history_extend_new_expiry, formatAbsolute(created.plusSeconds(value!!.toLong() * 3600))) else null,
            )
            if (entry.driver == "webrtc") FbText(stringResource(R.string.history_extend_live_note), style = Fb.type.caption, color = Fb.palette.textMuted)
        }
    }
    deleting?.let { entry ->
        var busy by remember { mutableStateOf(false) }
        FbDialog(
            { if (!busy) deleting = null }, stringResource(R.string.share_delete_title), stringResource(R.string.history_delete),
            {
                busy = true
                scope.launch {
                    runCatching { withContext(Dispatchers.IO) { service.accountHistoryDelete(entry.id) } }
                        .onSuccess { app.toast(resources.getString(R.string.share_deleted), Tone.Success) }
                        .onFailure { error = it.message ?: generic }
                    busy = false
                    deleting = null
                    revision++
                }
            },
            body = stringResource(R.string.share_delete_body), dismissLabel = stringResource(R.string.cancel), destructive = true, confirmLoading = busy,
        )
    }
}

@Composable
private fun HistoryCard(entry: HistoryEntry, modifier: Modifier, onCopy: () -> Unit, onExtend: () -> Unit, onDelete: () -> Unit) {
    val palette = Fb.palette
    val note = entry.kind.startsWith("note")
    Panel(modifier.fillMaxWidth()) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                FbIcon(if (note) R.drawable.ic_approved_note else R.drawable.ic_approved_folder, tint = palette.accentText)
                FbText(
                    if (note) stringResource(R.string.history_note) else pluralStringResource(R.plurals.history_files, entry.itemCount.toInt(), entry.itemCount.toInt()),
                    Modifier.weight(1f), style = Fb.type.bodyStrong, maxLines = 1,
                )
                StatusPill(stringResource(statuses.firstOrNull { it.first == entry.status }?.second ?: R.string.history_status_ended), tone = statusTone(entry.status))
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                FbText(entry.id, Modifier.weight(1f), style = Fb.type.codeSmall, color = palette.textMuted, maxLines = 1)
                IconAction(R.drawable.ic_approved_copy, stringResource(R.string.history_copy_id), onCopy, size = 36.dp)
            }
            val delivery = listOfNotNull(
                stringResource(if (entry.driver == "webrtc") R.string.history_live_webrtc else R.string.history_stored_http),
                stringResource(if (entry.delivery == "inbox") R.string.history_delivery_inbox else R.string.history_delivery_link),
                if (entry.burnOnRead) stringResource(R.string.option_burn) else null,
            ).joinToString(" · ")
            FbText(delivery + " · " + formatBytes(entry.ciphertextBytes.toLong()), style = Fb.type.small, color = palette.textMuted)
            val created = parseInstant(entry.createdAt)
            val removed = parseInstant(entry.removedAt)
            val expires = parseInstant(entry.expiresAt)
            Column {
                if (created != null) FbText(stringResource(R.string.history_created, formatAbsolute(created)), style = Fb.type.caption, color = palette.textSubtle)
                when {
                    removed != null -> FbText(stringResource(R.string.history_removed, formatAbsolute(removed)), style = Fb.type.caption, color = palette.textSubtle)
                    expires != null -> FbText(
                        stringResource(if (expires.isAfter(Instant.now())) R.string.history_expires else R.string.history_expired, formatAbsolute(expires)),
                        style = Fb.type.caption, color = palette.textSubtle,
                    )
                }
            }
            if (entry.canExtend || entry.canDelete) {
                Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                    if (entry.canExtend) FbButton(stringResource(R.string.history_extend), onExtend, Modifier.weight(1f), style = ButtonStyle.Secondary, icon = R.drawable.ic_approved_clock, height = 44.dp)
                    if (entry.canDelete) FbButton(stringResource(R.string.history_delete), onDelete, Modifier.weight(1f), style = ButtonStyle.Danger, icon = R.drawable.ic_approved_trash, height = 44.dp)
                }
            }
        }
    }
}
