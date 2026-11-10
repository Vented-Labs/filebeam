package io.filebeam.android.ui.feature.account

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import io.filebeam.android.R
import io.filebeam.android.ui.design.ButtonStyle
import io.filebeam.android.ui.design.Fb
import io.filebeam.android.ui.design.FbButton
import io.filebeam.android.ui.design.FbText
import io.filebeam.android.ui.design.FbTextField
import io.filebeam.android.ui.design.FilebeamMark
import io.filebeam.android.ui.design.Notice
import io.filebeam.android.ui.design.Panel
import io.filebeam.android.ui.design.PasswordField
import io.filebeam.android.ui.design.SecureWindowEffect
import io.filebeam.android.ui.design.Space
import io.filebeam.android.ui.design.SwitchRow
import io.filebeam.android.ui.design.Tone
import io.filebeam.android.ui.motion.Motion
import io.filebeam.android.ui.shell.Incoming
import io.filebeam.android.ui.shell.LocalAppModel
import io.filebeam.android.ui.shell.LocalNavigator
import io.filebeam.android.ui.shell.Route
import io.filebeam.android.ui.shell.SubScreen
import io.filebeam.android.ui.shell.collectAsStateCompat
import kotlinx.coroutines.launch

private val usernameRule = Regex("^[a-z0-9_]{3,24}$")
private val modeOrder = listOf("login", "register", "forgot", "reset", "verify", "invitation")

/**
 * Sign-in and account bootstrap: sign in, register, password recovery and reset, email
 * verification, and accepting an invitation. Passwords never leave this screen's memory.
 */
@Composable
fun AuthScreen(initialMode: String) {
    SecureWindowEffect()
    val app = LocalAppModel.current
    val navigator = LocalNavigator.current
    val context = LocalContext.current
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val settings by app.settings.collectAsStateCompat()
    val profile by app.instance.collectAsStateCompat()
    val account by app.account.collectAsStateCompat()
    val accounts = app.app.accounts
    var mode by rememberSaveable { mutableStateOf(initialMode) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }
    var identity by rememberSaveable { mutableStateOf("") }
    var username by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    var email by rememberSaveable { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    var remember by rememberSaveable { mutableStateOf(true) }
    var token by remember { mutableStateOf("") }
    var emailLocked by remember { mutableStateOf(false) }
    val incoming by app.incoming.collectAsStateCompat()
    val generic = stringResource(R.string.unknown_error)

    fun run(after: () -> Unit = {}, block: suspend () -> Unit) {
        error = null
        notice = null
        busy = true
        scope.launch {
            runCatching { block() }.onSuccess { after() }.onFailure { error = it.message ?: generic }
            busy = false
        }
    }
    fun switch(target: String) {
        mode = target
        error = null
        notice = null
        password = ""
        confirm = ""
    }

    // Tokens from links are handed over once and kept only for this screen.
    LaunchedEffect(incoming) {
        app.claim(Incoming.Invitation::class.java)?.let { invitation ->
            token = invitation.token
            mode = "invitation"
            runCatching { accounts.inspectInvitation(settings.instance, invitation.token) }
                .onSuccess {
                    it.email?.let { bound ->
                        email = bound
                        emailLocked = true
                    }
                }
                .onFailure { error = it.message ?: generic }
        }
        app.claim(Incoming.ResetPassword::class.java)?.let { reset ->
            token = reset.token
            reset.email?.let { email = it }
            mode = "reset"
        }
        app.claim(Incoming.VerifyEmail::class.java)?.let { verify ->
            mode = "verify"
            if (account != null) run({ app.toast(resources.getString(R.string.auth_verified), Tone.Success); navigator.replace(Route.Account) }) {
                accounts.verifyEmail(verify.link)
                accounts.refresh()
            } else notice = resources.getString(R.string.auth_verify_sign_in_first)
        }
    }

    val passwordsMatch = password == confirm
    val signedInDone = {
        app.toast(resources.getString(R.string.auth_signed_in), Tone.Success)
        navigator.replace(Route.Account)
    }

    SubScreen(
        stringResource(
            when (mode) {
                "register" -> R.string.auth_register_title
                "forgot" -> R.string.auth_forgot_title
                "reset" -> R.string.auth_reset_title
                "verify" -> R.string.auth_verify_title
                "invitation" -> R.string.auth_invitation_title
                else -> R.string.auth_login_title
            },
        ),
    ) {
        item {
            Panel(Modifier.fillMaxWidth()) {
                AnimatedContent(
                    mode,
                    transitionSpec = {
                        val forward = modeOrder.indexOf(targetState) >= modeOrder.indexOf(initialState)
                        (fadeIn(Motion.enter()) + slideInHorizontally(Motion.spatial()) { if (forward) it / 8 else -it / 8 }) togetherWith
                            (fadeOut(Motion.exit()) + slideOutHorizontally(Motion.exit()) { if (forward) -it / 10 else it / 10 })
                    },
                    label = "auth-mode",
                ) { current ->
                    Column(Modifier.fillMaxWidth().padding(Space.xl), verticalArrangement = Arrangement.spacedBy(Space.lg)) {
                        FilebeamMark(height = 34.dp)
                        FbText(
                            stringResource(
                                when (current) {
                                    "register" -> R.string.auth_register_lead
                                    "forgot" -> R.string.auth_forgot_lead
                                    "reset" -> R.string.auth_reset_lead
                                    "verify" -> R.string.auth_verify_lead
                                    "invitation" -> R.string.auth_invitation_lead
                                    else -> R.string.auth_login_lead
                                },
                            ),
                            style = Fb.type.body, color = Fb.palette.textMuted,
                        )
                        FbText(profile.name + " · " + settings.instance.removePrefix("https://").removePrefix("http://"), style = Fb.type.caption, color = Fb.palette.textSubtle)
                        notice?.let { Notice(it, tone = Tone.Success) }
                        error?.let { Notice(it, tone = Tone.Danger) }
                        when (current) {
                            "login" -> {
                                val submit = { run(signedInDone) { accounts.signIn(settings.instance, identity.trim(), password, remember) } }
                                FbTextField(
                                    identity, { identity = it }, label = stringResource(R.string.auth_identity), leadingIcon = R.drawable.ic_approved_user,
                                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
                                )
                                PasswordField(
                                    password, { password = it }, label = stringResource(R.string.option_password),
                                    showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
                                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
                                    keyboardActions = KeyboardActions(onGo = { if (identity.isNotBlank() && password.isNotEmpty()) submit() }),
                                )
                                SwitchRow(stringResource(R.string.auth_remember), remember, { remember = it })
                                FbButton(stringResource(R.string.auth_sign_in), submit, Modifier.fillMaxWidth(), enabled = identity.isNotBlank() && password.isNotEmpty(), loading = busy)
                                FbButton(stringResource(R.string.auth_forgot_link), { switch("forgot") }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
                                if (profile.registrationEnabled) {
                                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                                        FbText(stringResource(R.string.auth_new_here), style = Fb.type.small, color = Fb.palette.textMuted)
                                        FbButton(stringResource(R.string.auth_create_account), { switch("register") }, style = ButtonStyle.Quiet, height = 40.dp)
                                    }
                                }
                            }
                            "register", "invitation" -> {
                                val invited = current == "invitation"
                                val valid = usernameRule.matches(username) && email.contains('@') && password.length >= 8 && passwordsMatch
                                val submit = {
                                    run(signedInDone) {
                                        if (invited) accounts.acceptInvitation(settings.instance, token, username, name, email.trim(), password)
                                        else accounts.signUp(settings.instance, username, null, email.trim(), password)
                                    }
                                }
                                FbTextField(
                                    username, { username = it.lowercase().filter { char -> char.isLetterOrDigit() || char == '_' }.take(24) },
                                    label = stringResource(R.string.auth_username), supporting = stringResource(R.string.auth_username_rule), leadingIcon = R.drawable.ic_approved_user,
                                    error = if (username.isNotEmpty() && !usernameRule.matches(username)) stringResource(R.string.auth_username_rule) else null,
                                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Next),
                                )
                                if (invited) FbTextField(name, { name = it }, label = stringResource(R.string.auth_name_optional), keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next))
                                FbTextField(
                                    email, { email = it }, label = stringResource(R.string.auth_email), enabled = !(invited && emailLocked),
                                    supporting = if (invited && emailLocked) stringResource(R.string.auth_invitation_email_bound) else null,
                                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
                                )
                                PasswordField(
                                    password, { password = it }, label = stringResource(R.string.option_password), supporting = stringResource(R.string.password_length_rule),
                                    showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
                                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Next),
                                )
                                PasswordField(
                                    confirm, { confirm = it }, label = stringResource(R.string.auth_confirm_password),
                                    error = if (confirm.isNotEmpty() && !passwordsMatch) stringResource(R.string.auth_passwords_differ) else null,
                                    showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
                                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
                                    keyboardActions = KeyboardActions(onGo = { if (valid) submit() }),
                                )
                                FbButton(stringResource(R.string.auth_create_account), submit, Modifier.fillMaxWidth(), enabled = valid && (!invited || token.isNotEmpty()), loading = busy)
                                FbButton(stringResource(R.string.auth_have_account), { switch("login") }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
                            }
                            "forgot" -> {
                                val sent = stringResource(R.string.auth_reset_sent)
                                FbTextField(
                                    email, { email = it }, label = stringResource(R.string.auth_email), leadingIcon = R.drawable.ic_approved_link,
                                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Email, imeAction = ImeAction.Send),
                                )
                                FbButton(stringResource(R.string.auth_email_reset_link), { run({ notice = sent }) { accounts.requestPasswordReset(settings.instance, email.trim()) } }, Modifier.fillMaxWidth(), enabled = email.contains('@'), loading = busy)
                                FbButton(stringResource(R.string.auth_back_to_sign_in), { switch("login") }, Modifier.fillMaxWidth(), style = ButtonStyle.Quiet)
                            }
                            "reset" -> {
                                val done = stringResource(R.string.auth_reset_done)
                                if (token.isEmpty()) Notice(stringResource(R.string.auth_reset_needs_link), tone = Tone.Warning)
                                FbTextField(
                                    email, { email = it }, label = stringResource(R.string.auth_email),
                                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
                                )
                                PasswordField(
                                    password, { password = it }, label = stringResource(R.string.auth_new_password), supporting = stringResource(R.string.password_length_rule),
                                    showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
                                )
                                PasswordField(
                                    confirm, { confirm = it }, label = stringResource(R.string.auth_confirm_password),
                                    error = if (confirm.isNotEmpty() && !passwordsMatch) stringResource(R.string.auth_passwords_differ) else null,
                                    showLabel = stringResource(R.string.password_show), hideLabel = stringResource(R.string.password_hide),
                                )
                                FbButton(
                                    stringResource(R.string.auth_reset_action),
                                    { run({ switch("login"); notice = done }) { accounts.resetPassword(settings.instance, email.trim(), token, password) } },
                                    Modifier.fillMaxWidth(), enabled = token.isNotEmpty() && email.contains('@') && password.length >= 8 && passwordsMatch, loading = busy,
                                )
                            }
                            else -> {
                                val resent = stringResource(R.string.auth_verify_resent)
                                if (account != null) {
                                    FbButton(stringResource(R.string.auth_verify_resend), { run({ notice = resent }) { accounts.resendVerification() } }, Modifier.fillMaxWidth(), loading = busy)
                                    FbButton(stringResource(R.string.auth_verify_check), { run({ navigator.replace(Route.Account) }) { accounts.refresh() } }, Modifier.fillMaxWidth(), style = ButtonStyle.Secondary)
                                } else {
                                    FbButton(stringResource(R.string.auth_sign_in), { switch("login") }, Modifier.fillMaxWidth())
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private val Int.dp get() = androidx.compose.ui.unit.Dp(this.toFloat())
