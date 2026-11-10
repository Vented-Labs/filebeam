//! Signing in, creating an account and asking for a password reset.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, prelude::FluentBuilder,
    px,
};

use super::{AccountPanel, Area, AuthMode};
use crate::{
    model::{ClientCommand, DesktopSnapshot},
    shell::OpenSettings,
    theme::Palette,
    ui::{
        MotionExt, Tone, brand, button, field, format, motion,
        rail::{Segment, rail},
        surface, text,
    },
};

/// Checks a form before anything is sent, so a slip never costs a round trip.
pub(super) fn validate(
    mode: AuthMode,
    username: &str,
    email: &str,
    password: &str,
) -> Result<(), &'static str> {
    if mode == AuthMode::Register {
        if username.is_empty() {
            return Err("Choose a username.");
        }
        if !(3..=24).contains(&username.len())
            || !username
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err("Usernames are 3 to 24 lowercase letters, numbers or underscores.");
        }
    }
    match mode {
        AuthMode::SignIn if email.is_empty() => return Err("Enter your email or username."),
        _ if email.is_empty() => return Err("Enter your email address."),
        AuthMode::Register | AuthMode::Reset if !email.contains('@') => {
            return Err("That does not look like an email address.");
        }
        _ => {}
    }
    match mode {
        AuthMode::SignIn if password.is_empty() => Err("Enter your password."),
        AuthMode::Register if password.chars().count() < 8 => {
            Err("Use at least 8 characters for your password.")
        }
        _ => Ok(()),
    }
}

impl AccountPanel {
    fn auth_operation(&self) -> &'static str {
        match self.auth {
            AuthMode::SignIn => "login",
            AuthMode::Register => "register",
            AuthMode::Reset => "request password reset",
        }
    }

    pub(super) fn set_auth_mode(&mut self, mode: AuthMode, cx: &mut Context<Self>) {
        if self.auth != mode {
            self.auth = mode;
            self.clear_notice(Area::Auth, cx);
            cx.notify();
        }
    }

    pub(super) fn submit_auth(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.snapshot(cx).account.authenticated || self.waiting(self.auth_operation()) {
            return;
        }
        let email = Self::text(&self.email, cx);
        let password = Self::secret(&self.password, cx);
        let username = Self::text(&self.username, cx)
            .trim_start_matches('@')
            .to_lowercase();
        let name = Self::text(&self.name, cx);
        if let Err(problem) = validate(self.auth, &username, &email, &password) {
            self.say(Area::Auth, Tone::Danger, problem, cx);
            return;
        }
        let command = match self.auth {
            AuthMode::SignIn => ClientCommand::Login {
                email,
                password,
                remember: true,
            },
            AuthMode::Register => ClientCommand::Register {
                username,
                name: (!name.is_empty()).then_some(name),
                email,
                password,
            },
            AuthMode::Reset => ClientCommand::RequestPasswordReset { email },
        };
        self.send(command, cx);
    }

    pub(super) fn auth_page(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = format::host(&snapshot.instance.url);
        let mode = self.auth;
        let busy = self.waiting(self.auth_operation());
        let (title, detail, action) = match mode {
            AuthMode::SignIn => (
                "Welcome back",
                format!("Sign in to your account on {host}."),
                "Sign in",
            ),
            AuthMode::Register => (
                "Create your account",
                format!("Get a username people can send to on {host}."),
                "Create account",
            ),
            AuthMode::Reset => (
                "Reset your password",
                "We will email you a link to choose a new one.".to_owned(),
                "Send reset link",
            ),
        };
        let panel = cx.entity().downgrade();
        let modes = (mode != AuthMode::Reset).then(|| {
            rail(
                "account-auth-mode",
                "Sign in or create an account",
                usize::from(mode == AuthMode::Register),
            )
            .fill()
            .segment(Segment::new("Sign in"))
            .segment(Segment::new("Create account"))
            .on_select(move |index, _, cx| {
                let _ = panel.update(cx, |this, cx| {
                    this.set_auth_mode(
                        if index == 0 {
                            AuthMode::SignIn
                        } else {
                            AuthMode::Register
                        },
                        cx,
                    );
                });
            })
        });
        let heading = motion::entrance(("account-auth-heading", title), 0, window, cx);
        let card = motion::entrance("account-auth-card", 1, window, cx);

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .entering(heading)
                    .child(div().mb(px(6.)).child(brand::mark(p, 40.)))
                    .child(text::headline(title).text_color(p.text))
                    .child(text::body(detail).text_center().text_color(p.text_muted)),
            )
            .child(
                surface::panel(p)
                    .w_full()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .entering(card)
                    .children(modes)
                    .when(mode == AuthMode::Register, |this| {
                        this.child(field::field(
                            p,
                            "Username",
                            field::input(p, &self.username)
                                .id("account-username")
                                .prefix(text::body("@").text_color(p.text_subtle)),
                        ))
                        .child(field::optional_field(
                            p,
                            "Name",
                            field::input(p, &self.name).id("account-name"),
                        ))
                    })
                    .child(field::field(
                        p,
                        if mode == AuthMode::SignIn {
                            "Email or username"
                        } else {
                            "Email"
                        },
                        field::input(p, &self.email).id("account-email"),
                    ))
                    .when(mode != AuthMode::Reset, |this| {
                        this.child(
                            field::field(
                                p,
                                "Password",
                                field::secret(p, &self.password).id("account-password"),
                            )
                            .when(mode == AuthMode::Register, |this| {
                                this.child(field::hint(
                                    p,
                                    "At least 8 characters. It also protects your receiving key \
                                     if you choose that.",
                                    Tone::Neutral,
                                ))
                            }),
                        )
                    })
                    .children(self.notice(Area::Auth, p))
                    .child(
                        button::primary("account-submit")
                            .large()
                            .fill()
                            .label(action)
                            .loading(busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.submit_auth(window, cx)),
                            ),
                    )
                    .child(
                        div().flex().justify_center().child(match mode {
                            AuthMode::SignIn => button::quiet("account-reset-request")
                                .small()
                                .label("Forgot your password?")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_auth_mode(AuthMode::Reset, cx)
                                })),
                            _ => button::quiet("account-back-to-sign-in")
                                .small()
                                .icon("arrow-left")
                                .label("Back to sign in")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_auth_mode(AuthMode::SignIn, cx)
                                })),
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                text::caption(format!("Accounts belong to {host}."))
                                    .text_color(p.text_subtle),
                            )
                            .child(
                                button::quiet("account-change-instance")
                                    .small()
                                    .label("Use another instance")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(OpenSettings), cx)
                                    }),
                            ),
                    )
                    .child(
                        button::ghost("account-report")
                            .small()
                            .icon("alert")
                            .label("Report a transfer")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_report(window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }
}
