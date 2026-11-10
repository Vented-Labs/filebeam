package io.filebeam.android.ui.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import io.filebeam.android.R
import io.filebeam.android.platform.AppSettings
import io.filebeam.android.platform.services.AccountSessionRegistry
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.Hairline
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.SectionLabel
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private sealed interface Check {
    data object Idle : Check
    data object Running : Check
    data class Failed(val message: String?) : Check
}

@Composable
fun SettingsScreen() {
    val app = LocalAppModel.current
    val settings by app.settings.collectAsStateCompat()
    val transfer by app.app.transfers.state.collectAsStateCompat()
    val scope = rememberCoroutineScope()
    var instance by rememberSaveable(settings.instance) { mutableStateOf(settings.instance) }
    var relayOnly by rememberSaveable(settings.relayOnly) { mutableStateOf(settings.relayOnly) }
    var check by remember { mutableStateOf<Check>(Check.Idle) }
    val dirty = instance.trim().trimEnd('/') != settings.instance || relayOnly != settings.relayOnly
    val invalid = stringResource(R.string.settings_instance_invalid)
    val connected = stringResource(R.string.settings_instance_saved)

    // An instance is only saved after it answers, so a typo can never strand the app.
    val save: () -> Unit = {
        scope.launch {
            val origin = runCatching { AccountSessionRegistry.normalizeOrigin(instance) }.getOrElse {
                check = Check.Failed(invalid)
                return@launch
            }
            check = Check.Running
            runCatching { withContext(Dispatchers.IO) { app.app.accountSessions.service(origin).accountPolicyJson() } }
                .onSuccess {
                    app.app.settings.save(AppSettings(origin, relayOnly))
                    instance = origin
                    check = Check.Idle
                    app.toast(connected, Tone.Success)
                }
                .onFailure { check = Check.Failed(it.message) }
        }
    }

    val navigator = io.filebeam.android.ui.shell.LocalNavigator.current
    SubScreen(stringResource(R.string.settings_title)) {
        item {
            Panel(Modifier.fillMaxWidth()) {
                io.filebeam.android.ui.feature.account.LinkRow(R.drawable.ic_approved_brush, stringResource(R.string.appearance_title), { navigator.push(io.filebeam.android.ui.shell.Route.Appearance) })
                Hairline()
                io.filebeam.android.ui.feature.account.LinkRow(R.drawable.ic_approved_storage, stringResource(R.string.storage_title), { navigator.push(io.filebeam.android.ui.shell.Route.Storage) })
                Hairline()
                io.filebeam.android.ui.feature.account.LinkRow(R.drawable.ic_approved_info, stringResource(R.string.about_title), { navigator.push(io.filebeam.android.ui.shell.Route.About) })
            }
        }
        item { SectionLabel(stringResource(R.string.settings_connection)) }
        item {
            Panel(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(Space.lg), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                    FbTextField(
                        instance, {
                            instance = it
                            check = Check.Idle
                        },
                        label = stringResource(R.string.settings_instance),
                        placeholder = "https://filebeam.io",
                        supporting = stringResource(R.string.settings_instance_hint),
                        error = (check as? Check.Failed)?.let { it.message ?: stringResource(R.string.settings_instance_unreachable) },
                        leadingIcon = R.drawable.ic_approved_globe,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { if (dirty && !transfer.busy) save() }),
                    )
                    Hairline()
                    SwitchRow(
                        stringResource(R.string.settings_relay_only), relayOnly, { relayOnly = it },
                        description = stringResource(R.string.settings_relay_only_description), icon = R.drawable.ic_approved_shield,
                    )
                    if (transfer.busy && dirty) Notice(stringResource(R.string.settings_busy), tone = Tone.Warning)
                    FbButton(
                        stringResource(R.string.settings_check_and_save), save, Modifier.fillMaxWidth(),
                        enabled = dirty && !transfer.busy, loading = check == Check.Running,
                    )
                }
            }
        }
    }
}
