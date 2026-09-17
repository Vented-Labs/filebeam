package io.filebeam.android.ui

import android.view.WindowManager
import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.filebeam.android.R
import io.filebeam.android.platform.services.ServiceState
import io.filebeam.android.platform.services.AccountKeySituation
import io.filebeam.android.platform.services.KeyCustody
import io.filebeam.android.ui.design.FilebeamSpace
import io.filebeam.android.ui.design.ProductionGroupCard
import kotlinx.coroutines.launch

private enum class AccountPane { SIGN_IN, REGISTER, VERIFY, RECOVERY, RESET }

@Composable
fun AccountDestination(
    model: FilebeamViewModel,
    instance: String,
    exportKey: () -> Unit,
    importKey: () -> Unit,
) {
    val state = model.accounts.state.collectAsStateWithLifecycle().value
    val scope = rememberCoroutineScope()
    val activity = LocalActivity.current
    var pane by remember { mutableStateOf(AccountPane.SIGN_IN) }
    var username by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var token by remember { mutableStateOf("") }
    var verifyLink by remember { mutableStateOf("") }
    var custodyPassword by remember { mutableStateOf("") }
    var custody by remember { mutableStateOf(KeyCustody.SELF) }
    var acknowledgement by remember { mutableStateOf(false) }
    var situation by remember { mutableStateOf<AccountKeySituation?>(null) }
    var feedback by remember { mutableStateOf<String?>(null) }
    var confirmExport by remember { mutableStateOf(false) }
    var confirmImport by remember { mutableStateOf(false) }

    val sensitive = state !is ServiceState.Ready || custody == KeyCustody.PASSWORD
    DisposableEffect(sensitive) {
        if (sensitive) activity?.window?.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { if (sensitive) activity?.window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }
    LaunchedEffect(instance) { runCatching { model.accounts.resume(instance) }.onFailure { feedback = it.message } }
    LaunchedEffect(state is ServiceState.Ready) { if (state is ServiceState.Ready) runCatching { model.accounts.keySituation() }.onSuccess { situation = it }.onFailure { feedback = it.message } }

    Column(Modifier.verticalScroll(rememberScrollState()).imePadding().padding(horizontal = FilebeamSpace.Gutter, vertical = FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.Medium)) {
        Text(stringResource(R.string.account), style = MaterialTheme.typography.headlineSmall)
        when (state) {
            is ServiceState.Ready -> SignedInAccount(
                username = state.value.username,
                profileUrl = state.value.profileUrl,
                inbox = state.value.inboxEnabled,
                channel = state.value.notificationChannel,
                situation = situation,
                custody = custody,
                password = custodyPassword,
                replacementAcknowledged = acknowledgement,
                onInbox = { enabled -> scope.launch { runCatching { model.accounts.setInboxEnabled(enabled) }.onFailure { feedback = it.message } } },
                onNotification = { channel -> scope.launch { runCatching { model.accounts.setNotificationChannel(channel) }.onFailure { feedback = it.message } } },
                onCustody = { custody = it }, onPassword = { custodyPassword = it }, onAcknowledged = { acknowledgement = it }, onPrepare = { scope.launch { runCatching { model.accounts.setupReceivingKey(custody, custodyPassword.takeIf { custody == KeyCustody.PASSWORD }, acknowledgement) }.onSuccess { situation = it; custodyPassword = "" }.onFailure { feedback = it.message } } }, onExport = { confirmExport = true }, onImport = { confirmImport = true },
                onSignOut = { scope.launch { runCatching { model.accounts.signOut() }.onFailure { feedback = it.message } } })
            is ServiceState.Loading -> AccountStatusCard(stringResource(R.string.account_not_signed_in))
            is ServiceState.Failed -> AccountStatusCard(state.message, true)
            is ServiceState.Unavailable -> AccountStatusCard(state.reason, true)
        }
        if (state !is ServiceState.Ready) ProductionGroupCard(Modifier.fillMaxWidth()) {
            Column(verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
                AccountPane.entries.forEach { option -> TextButton(onClick = { pane = option }) { Text(stringResource(option.label)) } }
            }
            when (pane) {
                AccountPane.SIGN_IN -> {
                    CredentialFields(username, { username = it }, password, { password = it })
                    Button(enabled = username.isNotBlank() && password.isNotBlank(), onClick = { scope.launch { runCatching { model.accounts.signIn(instance, username, password) }.onSuccess { password = "" }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.sign_in)) }
                }
                AccountPane.REGISTER -> {
                    OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.account_name)) }, modifier = Modifier.fillMaxWidth())
                    CredentialFields(username, { username = it }, password, { password = it })
                    OutlinedTextField(email, { email = it }, label = { Text(stringResource(R.string.account_email)) }, modifier = Modifier.fillMaxWidth())
                    Button(enabled = username.isNotBlank() && email.isNotBlank() && password.isNotBlank(), onClick = { scope.launch { runCatching { model.accounts.signUp(instance, username, name, email, password) }.onSuccess { password = "" }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.sign_up)) }
                }
                AccountPane.VERIFY -> { OutlinedTextField(verifyLink, { verifyLink = it }, label = { Text(stringResource(R.string.verification_link)) }, modifier = Modifier.fillMaxWidth()); Button(enabled = verifyLink.startsWith("http"), onClick = { scope.launch { runCatching { model.accounts.verifyEmail(verifyLink) }.onSuccess { verifyLink = "" }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.verify_email)) }; TextButton(onClick = { scope.launch { runCatching { model.accounts.resendVerification() }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.resend_verification)) } }
                AccountPane.RECOVERY -> { OutlinedTextField(email, { email = it }, label = { Text(stringResource(R.string.account_email)) }, modifier = Modifier.fillMaxWidth()); Button(enabled = email.isNotBlank(), onClick = { scope.launch { runCatching { model.accounts.requestPasswordReset(instance, email) }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.request_reset)) } }
                AccountPane.RESET -> { OutlinedTextField(email, { email = it }, label = { Text(stringResource(R.string.account_email)) }, modifier = Modifier.fillMaxWidth()); OutlinedTextField(token, { token = it }, label = { Text(stringResource(R.string.reset_token)) }, modifier = Modifier.fillMaxWidth()); OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.new_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth()); Button(enabled = email.isNotBlank() && token.isNotBlank() && password.isNotBlank(), onClick = { scope.launch { runCatching { model.accounts.resetPassword(instance, email, token, password) }.onSuccess { password = "" }.onFailure { feedback = it.message } } }) { Text(stringResource(R.string.reset_password)) } }
            }
        }
        feedback?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
    if (confirmExport) AlertDialog(onDismissRequest = { confirmExport = false }, title = { Text(stringResource(R.string.export_key)) }, text = { Text(stringResource(R.string.key_export_review)) }, confirmButton = { Button(onClick = { confirmExport = false; exportKey() }) { Text(stringResource(R.string.continue_action)) } }, dismissButton = { TextButton(onClick = { confirmExport = false }) { Text(stringResource(R.string.cancel)) } })
    if (confirmImport) AlertDialog(onDismissRequest = { confirmImport = false }, title = { Text(stringResource(R.string.import_key)) }, text = { Text(stringResource(R.string.key_import_review)) }, confirmButton = { Button(onClick = { confirmImport = false; importKey() }) { Text(stringResource(R.string.continue_action)) } }, dismissButton = { TextButton(onClick = { confirmImport = false }) { Text(stringResource(R.string.cancel)) } })
}

@Composable private fun AccountStatusCard(message: String, error: Boolean = false) = ProductionGroupCard(Modifier.fillMaxWidth()) {
    Text(message, Modifier.padding(FilebeamSpace.Medium), color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable private fun CredentialFields(username: String, setUsername: (String) -> Unit, password: String, setPassword: (String) -> Unit) {
    OutlinedTextField(username, setUsername, label = { Text(stringResource(R.string.account_username)) }, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(password, setPassword, label = { Text(stringResource(R.string.account_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
}

@Composable private fun SignedInAccount(username: String, profileUrl: String?, inbox: Boolean, channel: String, situation: AccountKeySituation?, custody: KeyCustody, password: String, replacementAcknowledged: Boolean, onInbox: (Boolean) -> Unit, onNotification: (String) -> Unit, onCustody: (KeyCustody) -> Unit, onPassword: (String) -> Unit, onAcknowledged: (Boolean) -> Unit, onPrepare: () -> Unit, onExport: () -> Unit, onImport: () -> Unit, onSignOut: () -> Unit) {
    ProductionGroupCard(Modifier.fillMaxWidth()) { Column(Modifier.padding(FilebeamSpace.Medium), verticalArrangement = Arrangement.spacedBy(FilebeamSpace.XSmall)) {
    Text(stringResource(R.string.signed_in_as, username), style = MaterialTheme.typography.titleMedium)
    profileUrl?.let { Text(stringResource(R.string.receiving_profile, it)) }
    Text(stringResource(R.string.inbox_receiving), style = MaterialTheme.typography.titleMedium)
    Text(if (inbox) stringResource(R.string.inbox_enabled) else stringResource(R.string.inbox_disabled))
    Column {
        TextButton(onClick = { onInbox(!inbox) }) { Text(if (inbox) stringResource(R.string.disable_inbox) else stringResource(R.string.enable_inbox)) }
        TextButton(onClick = { onNotification(if (channel == "mail") "database" else "mail") }) { Text(stringResource(R.string.notification_preference, channel)) }
    }
    Text(stringResource(R.string.custody), style = MaterialTheme.typography.titleMedium)
    Text(stringResource(R.string.key_situation, situation?.activeBundleId?.toString() ?: stringResource(R.string.no_active_key)), style = MaterialTheme.typography.bodySmall)
    Column {
        TextButton(onClick = { onCustody(KeyCustody.SELF) }) { Text(stringResource(R.string.self_custody)) }
        TextButton(onClick = { onCustody(KeyCustody.PASSWORD) }) { Text(stringResource(R.string.password_custody)) }
    }
    if (custody == KeyCustody.PASSWORD) OutlinedTextField(password, onPassword, label = { Text(stringResource(R.string.account_password)) }, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth())
    if (situation?.replacementAcknowledgementRequired == true) TextButton(onClick = { onAcknowledged(!replacementAcknowledged) }) { Text(stringResource(if (replacementAcknowledged) R.string.replacement_acknowledged else R.string.acknowledge_replacement)) }
    Text(stringResource(R.string.key_generate_review), style = MaterialTheme.typography.bodySmall)
    Button(enabled = custody != KeyCustody.PASSWORD || password.isNotBlank(), onClick = onPrepare) { Text(stringResource(R.string.prepare_receiving_key)) }
    Column {
        TextButton(onClick = onExport) { Text(stringResource(R.string.export_key)) }
        TextButton(onClick = onImport) { Text(stringResource(R.string.import_key)) }
    }
    Text(stringResource(R.string.historical_key_notice), style = MaterialTheme.typography.bodySmall)
    TextButton(onClick = onSignOut) { Text(stringResource(R.string.sign_out)) }
    } }
}

private val AccountPane.label get() = when (this) { AccountPane.SIGN_IN -> R.string.sign_in; AccountPane.REGISTER -> R.string.sign_up; AccountPane.VERIFY -> R.string.verify; AccountPane.RECOVERY -> R.string.recovery; AccountPane.RESET -> R.string.reset }
