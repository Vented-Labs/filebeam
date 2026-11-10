import SwiftUI
import FilebeamDomain

/// Sign-in and account bootstrap: sign in, register, password recovery and reset, email
/// verification, and accepting an invitation. Passwords never leave this screen's memory.
struct AuthView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let initialMode: AuthMode
    @State private var mode: AuthMode
    @State private var busy = false
    @State private var error: String?
    @State private var notice: String?
    @State private var identity = ""
    @State private var username = ""
    @State private var name = ""
    @State private var email = ""
    @State private var password = ""
    @State private var confirm = ""
    @State private var remember = true
    @State private var invitationLink: String?
    @State private var resetLink: String?
    @State private var resetToken = ""
    @State private var emailLocked = false

    init(initialMode: AuthMode) {
        self.initialMode = initialMode
        _mode = State(initialValue: initialMode)
    }

    private var usernameValid: Bool { username.wholeMatch(of: /^[a-z0-9_]{3,24}$/) != nil }
    private var passwordsMatch: Bool { password == confirm }

    var body: some View {
        SubScreen(title) {
            Panel {
                VStack(alignment: .leading, spacing: Space.lg) {
                    FilebeamMark(height: 34)
                    Text(lead).fbText(.body, color: theme.textMuted)
                    Text("\(model.branding.name) · \(model.instanceTitle)").fbText(.caption, color: theme.textSubtle)
                    if let notice { Notice(notice, tone: .success) }
                    if let error { Notice(error, tone: .danger) }
                    form
                }
                .padding(Space.xl)
                .id(mode)
                .transition(.asymmetric(insertion: .opacity.combined(with: .move(edge: .trailing)), removal: .opacity))
            }
            .motion(Motion.spatial, value: mode)
        }
        .privacySensitive()
        .task { await claimLinks() }
    }

    @ViewBuilder private var form: some View {
        switch mode {
        case .login:
            let canSubmit = !identity.isBlank && !password.isEmpty
            FbTextField($identity, label: "Email or username", icon: .user, keyboard: .emailAddress, contentType: .username, submitLabel: .next)
            PasswordField($password, label: "Password", submitLabel: .go, onSubmit: { if canSubmit { signIn() } })
            SwitchRow("Keep me signed in", isOn: $remember)
            FbButton("Sign in", enabled: canSubmit, loading: busy) { signIn() }
            FbButton("Forgot password?", kind: .quiet) { switchTo(.forgot) }
            if model.policy?.registrationEnabled != false {
                HStack(spacing: 0) {
                    Text("New here?").fbText(.small, color: theme.textMuted)
                    FbButton("Create account", kind: .quiet, height: 40, fill: false) { switchTo(.register) }
                }
                .frame(maxWidth: .infinity)
            }
        case .register, .invitation:
            let invited = mode == .invitation
            let valid = usernameValid && email.contains("@") && password.count >= passwordMinimumLength && passwordsMatch
            FbTextField($username, label: "Username", supporting: "3 to 24 lowercase letters, numbers or underscores.", error: !username.isEmpty && !usernameValid ? "3 to 24 lowercase letters, numbers or underscores." : nil, icon: .user, contentType: .username, submitLabel: .next)
                .onChange(of: username) { _, value in username = String(value.lowercased().filter { $0.isLetter || $0.isNumber || $0 == "_" }.prefix(24)) }
            if invited { FbTextField($name, label: "Name (optional)", autocapitalization: .words, submitLabel: .next) }
            FbTextField($email, label: "Email", supporting: invited && emailLocked ? "This invitation is tied to this email address." : nil, enabled: !(invited && emailLocked), keyboard: .emailAddress, contentType: .emailAddress, submitLabel: .next)
            PasswordField($password, label: "Password", supporting: "Use at least 8 characters.", contentType: .newPassword, submitLabel: .next)
            PasswordField($confirm, label: "Confirm password", error: !confirm.isEmpty && !passwordsMatch ? "The passwords do not match." : nil, contentType: .newPassword, submitLabel: .go, onSubmit: { if valid { register() } })
            FbButton("Create account", enabled: valid && (!invited || invitationLink != nil), loading: busy) { register() }
            FbButton("I already have an account", kind: .quiet) { switchTo(.login) }
        case .forgot:
            FbTextField($email, label: "Email", icon: .link, keyboard: .emailAddress, contentType: .emailAddress, submitLabel: .send, onSubmit: { if email.contains("@") { requestReset() } })
            FbButton("Email reset link", enabled: email.contains("@"), loading: busy) { requestReset() }
            FbButton("Back to sign in", kind: .quiet) { switchTo(.login) }
        case .reset:
            if resetToken.isEmpty { Notice("Open the reset link from your email on this device to continue.", tone: .warning) }
            FbTextField($email, label: "Email", keyboard: .emailAddress, contentType: .emailAddress, submitLabel: .next)
            PasswordField($password, label: "New password", supporting: "Use at least 8 characters.", contentType: .newPassword, submitLabel: .next)
            PasswordField($confirm, label: "Confirm password", error: !confirm.isEmpty && !passwordsMatch ? "The passwords do not match." : nil, contentType: .newPassword)
            FbButton("Reset password", enabled: !resetToken.isEmpty && email.contains("@") && password.count >= passwordMinimumLength && passwordsMatch, loading: busy) { reset() }
        case .verify:
            if model.session != nil {
                FbButton("Resend verification email", loading: busy) { run(after: { notice = "Verification email sent." }) { try await model.service.resendVerification(instance: model.instance) } }
                FbButton("I have verified", kind: .secondary) { run(after: { model.replace(.account) }) { await model.refreshSession() } }
            } else {
                FbButton("Sign in") { switchTo(.login) }
            }
        }
    }

    private var title: String {
        switch mode { case .login: "Sign in"; case .register: "Create account"; case .forgot: "Reset password"; case .reset: "Choose a new password"; case .verify: "Verify your email"; case .invitation: "Accept invitation" }
    }

    private var lead: String {
        switch mode {
        case .login: "Welcome back. Sign in to manage your account."
        case .register: "Create an account to receive files by username and keep a transfer history."
        case .forgot: "Enter your account email and we will send a reset link."
        case .reset: "Set a new password for your account."
        case .verify: "Open the verification link we emailed you on this device."
        case .invitation: "You were invited to this Filebeam instance. Create your account to continue."
        }
    }

    private func switchTo(_ target: AuthMode) {
        withMotion(Motion.spatial) { mode = target }
        error = nil
        notice = nil
        password = ""
        confirm = ""
    }

    private func run(after: @escaping () -> Void = {}, _ block: @escaping () async throws -> Void) {
        error = nil
        notice = nil
        busy = true
        Task {
            do { try await block(); after() } catch { self.error = readableMessage(error) }
            busy = false
        }
    }

    private func signedIn() {
        model.toast("Signed in", tone: .success)
        password = ""
        confirm = ""
        model.replace(.account)
    }

    private func signIn() { run(after: signedIn) { try await model.login(identity: identity.trimmingCharacters(in: .whitespaces), password: password, remember: remember) } }

    private func register() {
        run(after: signedIn) {
            if mode == .invitation, let invitationLink {
                try await model.acceptInvitation(link: invitationLink, username: username, name: name.nilIfEmpty, email: email.trimmingCharacters(in: .whitespaces), password: password)
            } else {
                try await model.register(username: username, name: nil, email: email.trimmingCharacters(in: .whitespaces), password: password)
            }
        }
    }

    private func requestReset() { run(after: { notice = "If that email has an account, a reset link is on its way." }) { try await model.service.requestPasswordReset(instance: model.instance, email: email.trimmingCharacters(in: .whitespaces)) } }

    private func reset() {
        run(after: { switchTo(.login); notice = "Password updated. Sign in with the new password." }) {
            try await model.service.resetPassword(instance: model.instance, email: email.trimmingCharacters(in: .whitespaces), token: resetToken, password: password)
        }
    }

    /// Tokens from links are handed over once and kept only for this screen.
    private func claimLinks() async {
        if let link = model.pendingInvitationLink {
            model.pendingInvitationLink = nil
            invitationLink = link
            mode = .invitation
            do {
                let inspection = try await model.service.inspectInvite(link: link)
                if let bound = inspection.email { email = bound; emailLocked = true }
            } catch { self.error = readableMessage(error) }
        }
        if let link = model.pendingResetLink {
            model.pendingResetLink = nil
            resetLink = link
            mode = .reset
            if let components = URLComponents(string: link) {
                email = components.queryItems?.first { $0.name == "email" }?.value ?? email
                resetToken = components.queryItems?.first { $0.name == "token" }?.value ?? components.path.split(separator: "/").last.map(String.init) ?? ""
            }
        }
        if let link = model.pendingVerificationLink {
            model.pendingVerificationLink = nil
            mode = .verify
            if model.session != nil {
                run(after: { model.toast("Email verified", tone: .success); model.replace(.account) }) {
                    try await model.service.verifyEmail(link: link)
                    await model.refreshSession()
                }
            } else { notice = "Sign in first, then open the verification link again." }
        }
    }
}
