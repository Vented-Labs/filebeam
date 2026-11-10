package io.filebeam.android.ui.feature.account

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.filebeam.android.R
import io.filebeam.android.ui.common.formatAbsolute
import io.filebeam.android.ui.common.parseInstant
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.EmptyState
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbDialog
import io.filebeam.android.ui.design.FbIcon
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Shapes
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.Spinner
import io.filebeam.android.ui.design.StatusPill
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import io.filebeam.rust.DeviceRegistry
import io.filebeam.rust.RegisteredDevice
import io.filebeam.rust.deviceKeyFingerprint
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.Instant

private sealed interface DeviceDialog {
    data class Rename(val device: RegisteredDevice) : DeviceDialog
    data class Revoke(val device: RegisteredDevice) : DeviceDialog
    data object Reset : DeviceDialog
}

/** Registered devices for this account: review, rename, revoke one, or reset them all. */
@Composable
fun DevicesScreen() {
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val account by app.account.collectAsStateCompat()
    var registry by remember { mutableStateOf<DeviceRegistry?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var revision by remember { mutableIntStateOf(0) }
    var dialog by remember { mutableStateOf<DeviceDialog?>(null) }
    val current = account
    val generic = stringResource(R.string.unknown_error)

    LaunchedEffect(current == null) { if (current == null) navigator.replace(Route.Auth()) }
    if (current == null) return
    val service = remember(current.instance) { app.app.accountSessions.service(current.instance) }

    LaunchedEffect(revision) {
        error = null
        runCatching { withContext(Dispatchers.IO) { service.accountDevices() } }
            .onSuccess { registry = it }
            .onFailure { error = it.message ?: generic }
    }

    SubScreen(stringResource(R.string.devices_title)) {
        val loaded = registry
        when {
            error != null && loaded == null -> item {
                EmptyState(R.drawable.ic_approved_alert, stringResource(R.string.devices_error_title), body = error, tone = Tone.Danger) {
                    FbButton(stringResource(R.string.retry), { revision++ }, style = ButtonStyle.Secondary)
                }
            }
            loaded == null -> item { Box(Modifier.fillMaxWidth().padding(Space.xxxl), contentAlignment = Alignment.Center) { Spinner() } }
            else -> {
                val active = loaded.devices.count { it.status == "active" }
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                        FbText(pluralStringResource(R.plurals.devices_active, active, active), style = Fb.type.headline)
                        FbText(stringResource(R.string.devices_revision, loaded.revision.toLong()), style = Fb.type.caption, color = Fb.palette.textSubtle)
                        FbText(stringResource(R.string.devices_explainer), style = Fb.type.small, color = Fb.palette.textMuted)
                    }
                }
                error?.let { message -> item { Notice(message, tone = Tone.Danger) } }
                if (loaded.devices.isEmpty()) {
                    item { EmptyState(R.drawable.ic_approved_laptop, stringResource(R.string.devices_empty_title), body = stringResource(R.string.devices_empty_body)) }
                }
                items(loaded.devices, key = { it.id }) { device ->
                    DeviceCard(device, Modifier.animateItem(placementSpec = Motion.spatial()), onRename = { dialog = DeviceDialog.Rename(device) }, onRevoke = { dialog = DeviceDialog.Revoke(device) })
                }
                if (loaded.devices.any { it.status != "revoked" }) {
                    item { FbButton(stringResource(R.string.devices_reset_all), { dialog = DeviceDialog.Reset }, Modifier.fillMaxWidth(), style = ButtonStyle.Danger) }
                }
            }
        }
    }

    when (val open = dialog) {
        is DeviceDialog.Rename -> {
            var name by remember(open) { mutableStateOf(open.device.displayName) }
            var busy by remember { mutableStateOf(false) }
            FbDialog(
                { dialog = null }, stringResource(R.string.devices_rename_title), stringResource(R.string.devices_rename),
                {
                    busy = true
                    scope.launch {
                        runCatching { withContext(Dispatchers.IO) { service.accountDeviceRename(open.device.id, name.trim()) } }
                            .onSuccess { app.toast(context.getString(R.string.devices_renamed), Tone.Success) }
                            .onFailure { error = it.message ?: generic }
                        busy = false
                        dialog = null
                        revision++
                    }
                },
                dismissLabel = stringResource(R.string.cancel), confirmEnabled = name.isNotBlank() && name.trim() != open.device.displayName, confirmLoading = busy,
            ) {
                FbTextField(name, { if (it.length <= 80) name = it }, label = stringResource(R.string.devices_name))
            }
        }
        is DeviceDialog.Revoke, DeviceDialog.Reset -> {
            SecureWindowEffect()
            val reset = open is DeviceDialog.Reset
            var password by remember(open) { mutableStateOf("") }
            var busy by remember { mutableStateOf(false) }
            var failure by remember(open) { mutableStateOf<String?>(null) }
            FbDialog(
                { if (!busy) dialog = null },
                if (reset) stringResource(R.string.devices_reset_title) else stringResource(R.string.devices_revoke_title, (open as DeviceDialog.Revoke).device.displayName),
                stringResource(if (reset) R.string.devices_reset_all else R.string.devices_revoke),
                {
                    busy = true
                    failure = null
                    scope.launch {
                        runCatching {
                            withContext(Dispatchers.IO) {
                                if (reset) service.accountDevicesReset(password) else service.accountDeviceRevoke((open as DeviceDialog.Revoke).device.id, password)
                            }
                        }.onSuccess {
                            app.toast(context.getString(if (reset) R.string.devices_reset_done else R.string.devices_revoked), Tone.Success)
                            dialog = null
                            revision++
                        }.onFailure { failure = it.message ?: generic }
                        busy = false
                    }
                },
                body = stringResource(if (reset) R.string.devices_reset_body else R.string.devices_revoke_body),
                dismissLabel = stringResource(R.string.cancel), destructive = true, confirmEnabled = password.isNotEmpty(), confirmLoading = busy,
            ) {
                PasswordField(
                    password, { password = it }, label = stringResource(R.string.account_current_password), error = failure,
                    showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
                )
            }
        }
        null -> Unit
    }
}

@Composable
private fun DeviceCard(device: RegisteredDevice, modifier: Modifier, onRename: () -> Unit, onRevoke: () -> Unit) {
    val palette = Fb.palette
    var expanded by remember { mutableStateOf(false) }
    val fingerprint = remember(device.publicKey) { runCatching { deviceKeyFingerprint(device.publicKey) }.getOrDefault(device.publicKey) }
    val online = parseInstant(device.presenceExpiresAt)?.isAfter(Instant.now()) == true
    val revoked = device.status == "revoked"
    Panel(modifier.fillMaxWidth().animateContentSize(Motion.spatial())) {
        Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.md)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                Box(Modifier.size(44.dp).clip(Shapes.control), contentAlignment = Alignment.Center) {
                    FbIcon(if (device.platform.contains("android", true) || device.platform.contains("ios", true)) R.drawable.ic_approved_mobile else R.drawable.ic_approved_laptop, tint = palette.accentText, size = 26.dp)
                }
                Column(Modifier.weight(1f)) {
                    FbText(device.displayName, style = Fb.type.bodyStrong, maxLines = 1)
                    FbText(stringResource(R.string.devices_platform_key, device.platform, device.keyVersion.toInt()), style = Fb.type.small, color = palette.textMuted, maxLines = 1)
                }
                StatusPill(
                    when {
                        revoked -> stringResource(R.string.devices_state_revoked)
                        device.status == "pending" -> stringResource(R.string.devices_state_pending)
                        online -> stringResource(R.string.devices_state_ready)
                        else -> stringResource(R.string.devices_state_offline)
                    },
                    tone = when {
                        revoked -> Tone.Danger
                        device.status == "pending" -> Tone.Warning
                        online -> Tone.Success
                        else -> Tone.Neutral
                    },
                )
            }
            FbText(
                if (expanded) fingerprint.chunked(4).joinToString(" ") else fingerprint.take(16).chunked(4).joinToString(" ") + "…",
                Modifier.fillMaxWidth().clip(Shapes.sm).clickable(onClickLabel = stringResource(R.string.devices_show_fingerprint)) { expanded = !expanded },
                style = Fb.type.codeSmall, color = palette.textMuted,
            )
            parseInstant(device.revokedAt)?.let { FbText(stringResource(R.string.devices_revoked_on, formatAbsolute(it)), style = Fb.type.caption, color = palette.danger.ink) }
            if (!revoked) {
                Row(horizontalArrangement = Arrangement.spacedBy(Space.md)) {
                    FbButton(stringResource(R.string.devices_rename), onRename, Modifier.weight(1f), style = ButtonStyle.Secondary, height = 44.dp)
                    FbButton(stringResource(R.string.devices_revoke), onRevoke, Modifier.weight(1f), style = ButtonStyle.Danger, height = 44.dp)
                }
            }
        }
    }
}
