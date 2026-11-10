//! Identity, the receiving address, reporting a transfer and deleting the account.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, prelude::FluentBuilder,
    px,
};
use gpui_component::input::Textarea;

use super::{AccountPanel, Area, Copied, REPORT_CATEGORIES};
use crate::{
    model::{ClientCommand, DesktopSnapshot},
    theme::{Palette, tokens::radius},
    ui::{
        TextExt, Tone, brand, button, dialog, feedback,
        field::{self, copy_field},
        format, row, surface, text,
    },
};

/// The transfer a report is about, taken from a pasted ID or share link.
pub(super) fn reported_transfer(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let path = trimmed.split(['#', '?']).next().unwrap_or(trimmed);
    let candidate = path.trim_end_matches('/').rsplit('/').next()?;
    ulid::Ulid::from_string(candidate)
        .is_ok()
        .then(|| candidate.to_owned())
}

impl AccountPanel {
    pub(super) fn profile(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let account = &snapshot.account;
        let email = account.email.clone().unwrap_or_default();
        let handle = account
            .username
            .as_ref()
            .map_or_else(|| email.clone(), |username| format!("@{username}"));
        let resending = self.waiting("resend verification");
        let address = account.profile_url.clone();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(self.notice(Area::Profile, p))
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.))
                            .child(brand::avatar(p, &handle, 56.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(text::title(handle).single_line().text_color(p.text))
                                    .child(
                                        text::small(email).single_line().text_color(p.text_muted),
                                    ),
                            )
                            .child(if account.email_verified {
                                feedback::pill_with_icon(p, "check", "Verified", Tone::Success)
                            } else {
                                feedback::pill(p, "Email not verified", Tone::Warning)
                            }),
                    )
                    .when(!account.email_verified, |this| {
                        this.child(
                            feedback::notice(
                                p,
                                "Confirm your email to finish setting up your account. The link \
                                 opens in your browser.",
                                Tone::Warning,
                            )
                            .items_center()
                            .child(
                                button::secondary("resend-verification")
                                    .small()
                                    .label("Send it again")
                                    .loading(resending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if !this.waiting("resend verification") {
                                            this.send(ClientCommand::ResendVerification, cx);
                                        }
                                    })),
                            ),
                        )
                    }),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(text::eyebrow("Your receiving link").text_color(p.text_subtle))
                    .child(match address {
                        Some(address) => div()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(
                                copy_field("account-copy-address", "link", address.clone())
                                    .action("Copy link")
                                    .primary()
                                    .copied(self.copied == Some(Copied::Address))
                                    .on_copy(cx.listener(move |this, _, _, cx| {
                                        this.copy(address.clone(), Copied::Address, cx)
                                    })),
                            )
                            .child(
                                text::small(
                                    "Share it so people can send you encrypted files and notes. \
                                     Who may use it is set under Receiving.",
                                )
                                .text_color(p.text_muted),
                            ),
                        None => div().child(
                            text::small("Your account has no username yet, so it has no link.")
                                .text_color(p.text_muted),
                        ),
                    }),
            )
            .child(
                surface::panel(p)
                    .p(px(8.))
                    .flex()
                    .flex_col()
                    .child(
                        row::link_row("account-report", "alert", "Report a transfer")
                            .detail("Tell this instance about abuse")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_report(window, cx)),
                            ),
                    )
                    .child(div().px(px(12.)).child(surface::hairline(p)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .px(px(12.))
                            .py(px(10.))
                            .rounded(px(radius::CONTROL))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(text::body_strong("Delete account").text_color(p.text))
                                    .child(
                                        text::small(
                                            "Removes your account, inbox and receiving key from \
                                             this instance for good.",
                                        )
                                        .text_color(p.text_muted),
                                    ),
                            )
                            .child(
                                button::danger("account-delete")
                                    .small()
                                    .icon("trash")
                                    .label("Delete…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_delete(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Asks what is wrong with a transfer. Reports work signed in or out.
    pub(super) fn open_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_notice(Area::Report, cx);
        let (id, category, description, email) = (
            self.report_id.clone(),
            self.report_category.clone(),
            self.report_description.clone(),
            self.report_email.clone(),
        );
        let (shown, submitted) = (cx.entity(), cx.entity());
        dialog::prompt("Report a transfer")
            .body("Reports go to the people who run this instance. They cannot read the content.")
            .confirm("Send report")
            .open_with(
                window,
                cx,
                move |p, _, cx| {
                    let problem = shown.read(cx).notice(Area::Report, p);
                    Some(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(field::field(
                                p,
                                "Transfer link or ID",
                                field::input(p, &id).id("report-transfer-id"),
                            ))
                            .child(field::field(
                                p,
                                "What is it?",
                                div()
                                    .w_full()
                                    .child(field::select(p, &category).id("report-category")),
                            ))
                            .child(field::field(
                                p,
                                "What happened",
                                Textarea::new(&description).h(px(96.)).bg(p.sunken),
                            ))
                            .child(field::optional_field(
                                p,
                                "Your email",
                                field::input(p, &email).id("report-email"),
                            ))
                            .children(problem)
                            .into_any_element(),
                    )
                },
                move |window, cx| {
                    let sent = submitted.update(cx, |this, cx| this.submit_report(cx));
                    // The dialog is drawn by the window, so it is asked to redraw.
                    window.refresh();
                    sent
                },
                |_, _| {},
            );
    }

    /// Sends the report if it is complete. Returns whether its dialog may close.
    fn submit_report(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(transfer_id) = reported_transfer(&Self::text(&self.report_id, cx)) else {
            self.say(
                Area::Report,
                Tone::Danger,
                "Paste the transfer's link, or the ID from it.",
                cx,
            );
            return false;
        };
        let description = self.report_description.read(cx).value().trim().to_owned();
        if description.chars().count() < 10 {
            self.say(
                Area::Report,
                Tone::Danger,
                "Describe what is wrong in a sentence or two.",
                cx,
            );
            return false;
        }
        let email = Self::text(&self.report_email, cx);
        if !email.is_empty() && !email.contains('@') {
            self.say(
                Area::Report,
                Tone::Danger,
                "That does not look like an email address.",
                cx,
            );
            return false;
        }
        let chosen = self.report_category.read(cx).selected_value().copied();
        let category = REPORT_CATEGORIES
            .iter()
            .find(|(_, label)| Some(*label) == chosen)
            .map_or("other", |(value, _)| *value);
        self.clear_notice(Area::Report, cx);
        self.send(
            ClientCommand::Report {
                transfer_id,
                category: category.into(),
                description,
                email: (!email.is_empty()).then_some(email),
            },
            cx,
        );
        true
    }

    /// Deleting needs the password and a typed word, so it cannot happen by accident.
    pub(super) fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_notice(Area::Delete, cx);
        let host = format::host(&self.snapshot(cx).instance.url);
        let (password, phrase) = (self.confirm_password.clone(), self.confirm_phrase.clone());
        let (forgotten_password, forgotten_phrase) = (password.clone(), phrase.clone());
        let (shown, submitted) = (cx.entity(), cx.entity());
        dialog::prompt("Delete your account?")
            .body(format!(
                "Your account, username, inbox and receiving key are removed from {host} for \
                 good. Files already saved on this device stay where they are."
            ))
            .confirm("Delete account")
            .destructive()
            .open_with(
                window,
                cx,
                move |p, _, cx| {
                    let problem = shown.read(cx).notice(Area::Delete, p);
                    Some(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(field::field(
                                p,
                                "Current password",
                                field::secret(p, &password).id("delete-password"),
                            ))
                            .child(field::field(
                                p,
                                "Type DELETE to confirm",
                                field::input(p, &phrase).id("delete-phrase"),
                            ))
                            .children(problem)
                            .into_any_element(),
                    )
                },
                move |window, cx| {
                    let sent = submitted.update(cx, |this, cx| this.submit_delete(cx));
                    window.refresh();
                    sent
                },
                move |window, cx| {
                    for input in [&forgotten_password, &forgotten_phrase] {
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                },
            );
    }

    fn submit_delete(&mut self, cx: &mut Context<Self>) -> bool {
        let current_password = Self::secret(&self.confirm_password, cx);
        let confirmation = Self::text(&self.confirm_phrase, cx);
        if current_password.is_empty() {
            self.say(
                Area::Delete,
                Tone::Danger,
                "Enter your current password.",
                cx,
            );
            return false;
        }
        if confirmation != "DELETE" {
            self.say(
                Area::Delete,
                Tone::Danger,
                "Type DELETE in capitals to confirm.",
                cx,
            );
            return false;
        }
        self.clear_notice(Area::Delete, cx);
        self.send(
            ClientCommand::DeleteAccount {
                current_password,
                confirmation,
            },
            cx,
        );
        true
    }
}
