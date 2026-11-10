//! The receiving key: what senders encrypt to, and the only thing that opens an
//! inbox. The private half is never drawn; it moves only between the service, the
//! operating system's save dialog and a masked field.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, prelude::FluentBuilder,
    px,
};

use super::{AccountPanel, Area, Copied};
use crate::{
    model::{ClientCommand, DesktopSnapshot, KeyCustody},
    theme::{Palette, tokens::radius},
    ui::{Tone, button, dialog, feedback, field, row, surface, text},
};

fn protection(custody_mode: &str) -> &'static str {
    match custody_mode {
        "password" => "Your account password",
        "self" => "A backup only you hold",
        _ => "Set on another device",
    }
}

fn state(custody: &KeyCustody) -> (&'static str, Tone) {
    match custody {
        KeyCustody::Configured { .. } => ("Ready", Tone::Success),
        KeyCustody::Locked { .. } => ("Locked on this device", Tone::Warning),
        KeyCustody::PendingBackup { .. } => ("Backup needed", Tone::Warning),
        _ => ("Not set up", Tone::Neutral),
    }
}

impl AccountPanel {
    /// Enter in the password field does what that state's main button does.
    pub(super) fn submit_key_password(&mut self, cx: &mut Context<Self>) {
        match self.snapshot(cx).account.key_custody {
            KeyCustody::Locked { .. } => self.unlock_key(cx),
            KeyCustody::Unknown => self.create_key(true, cx),
            _ => {}
        }
    }

    fn create_key(&mut self, with_password: bool, cx: &mut Context<Self>) {
        if self.waiting("generate receiving key") {
            return;
        }
        let password = with_password.then(|| Self::secret(&self.key_password, cx));
        if password.as_ref().is_some_and(String::is_empty) {
            self.say(Area::Key, Tone::Danger, "Enter your account password.", cx);
            return;
        }
        self.send(
            ClientCommand::GenerateReceivingKey {
                password,
                replace: false,
                acknowledge_old_key_loss: false,
            },
            cx,
        );
    }

    fn unlock_key(&mut self, cx: &mut Context<Self>) {
        if self.waiting("unlock receiving key") {
            return;
        }
        let password = Self::secret(&self.key_password, cx);
        if password.is_empty() {
            self.say(Area::Key, Tone::Danger, "Enter your key password.", cx);
            return;
        }
        self.send(ClientCommand::UnlockReceivingKey { password }, cx);
    }

    fn import_key(&mut self, cx: &mut Context<Self>) {
        if self.waiting("import receiving key") {
            return;
        }
        let export = Self::secret(&self.key_import, cx);
        if export.trim().is_empty() {
            self.say(
                Area::Key,
                Tone::Danger,
                "Paste the contents of your key backup.",
                cx,
            );
            return;
        }
        self.send(ClientCommand::ImportReceivingKey { export }, cx);
    }

    fn export_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_new_path(
            &crate::platform::user_directory(),
            Some("filebeam-receiving-key.txt"),
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(destination))) = receiver.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.send(ClientCommand::ExportReceivingKey { destination }, cx);
            });
        })
        .detach();
    }

    /// Replacing asks for the password in a prompt that spells out what is lost.
    fn confirm_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_notice(Area::Key, cx);
        let password = self.confirm_password.clone();
        let forgotten = password.clone();
        let (shown, submitted) = (cx.entity(), cx.entity());
        dialog::prompt("Replace your receiving key?")
            .body(
                "A new key takes over from this one. Anything already sent to the old key may \
                 never open again, on any device.",
            )
            .confirm("Replace key")
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
                                field::secret(p, &password).id("replace-password"),
                            ))
                            .children(problem)
                            .into_any_element(),
                    )
                },
                move |window, cx| {
                    let sent = submitted.update(cx, |this, cx| {
                        let password = Self::secret(&this.confirm_password, cx);
                        if password.is_empty() {
                            this.say(
                                Area::Delete,
                                Tone::Danger,
                                "Enter your current password.",
                                cx,
                            );
                            return false;
                        }
                        this.clear_notice(Area::Delete, cx);
                        this.send(
                            ClientCommand::GenerateReceivingKey {
                                password: Some(password),
                                replace: true,
                                acknowledge_old_key_loss: true,
                            },
                            cx,
                        );
                        true
                    });
                    window.refresh();
                    sent
                },
                move |window, cx| {
                    forgotten.update(cx, |input, cx| input.set_value("", window, cx));
                },
            );
    }

    pub(super) fn keys(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let custody = snapshot.account.key_custody.clone();
        let (label, tone) = state(&custody);
        let creating = self.waiting("generate receiving key");
        let unlocking = self.waiting("unlock receiving key");
        let importing = self.waiting("import receiving key");
        let exporting = self.waiting("export receiving key");
        let confirming = self.waiting("confirm receiving-key backup");

        let summary = surface::panel(p)
            .p(px(22.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(row::icon_tile(p, "key", 44., true))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text::subtitle("Receiving key").text_color(p.text))
                            .child(
                                text::small(
                                    "Senders encrypt to this key, so only you can open what \
                                     arrives.",
                                )
                                .text_color(p.text_muted),
                            ),
                    )
                    .child(feedback::pill(p, label, tone)),
            )
            .when_some(
                match &custody {
                    KeyCustody::Configured {
                        version,
                        fingerprint,
                        custody_mode,
                    }
                    | KeyCustody::Locked {
                        version,
                        fingerprint,
                        custody_mode,
                    } => Some((fingerprint.clone(), Some((*version, custody_mode.clone())))),
                    KeyCustody::PendingBackup { fingerprint, .. } => {
                        Some((fingerprint.clone(), None))
                    }
                    _ => None,
                },
                |this, (fingerprint, details)| {
                    let copied = self.copied == Some(Copied::Fingerprint);
                    let value = fingerprint.clone();
                    this.child(surface::hairline(p))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.))
                                .child(
                                    text::small("Fingerprint")
                                        .flex_none()
                                        .text_color(p.text_muted),
                                )
                                .child(
                                    button::ghost("key-copy-fingerprint")
                                        .small()
                                        .icon(if copied { "check" } else { "copy" })
                                        .label(fingerprint)
                                        .tooltip("Copy the fingerprint")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.copy(value.clone(), Copied::Fingerprint, cx)
                                        })),
                                ),
                        )
                        .when_some(details, |this, (version, custody_mode)| {
                            this.child(row::fact(p, "Protected by", protection(&custody_mode)))
                                .child(row::fact(p, "Version", version.to_string()))
                        })
                },
            );

        let step = |number: &'static str, title: &'static str, detail: &'static str| {
            div()
                .flex()
                .items_start()
                .gap(px(14.))
                .child(
                    div()
                        .size(px(28.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(p.accent_surface)
                        .child(text::small_strong(number).text_color(p.accent_text)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(text::body_strong(title).text_color(p.text))
                        .child(text::small(detail).text_color(p.text_muted)),
                )
        };
        let option = |title: &'static str, detail: &'static str| {
            div()
                .p(px(18.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .rounded(px(radius::CARD))
                .border_1()
                .border_color(p.border)
                .bg(p.raised)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(text::body_strong(title).text_color(p.text))
                        .child(text::small(detail).text_color(p.text_muted)),
                )
        };

        let actions: AnyElement = match custody {
            KeyCustody::PendingBackup { generation_id, .. } => surface::panel(p)
                .p(px(22.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(text::eyebrow("Finish setting up").text_color(p.text_subtle))
                .child(
                    step(
                        "1",
                        "Save your key backup",
                        "Keep the file somewhere only you can reach, such as a password manager.",
                    )
                    .child(
                        button::secondary("key-export-pending")
                            .small()
                            .icon("download")
                            .label("Save backup…")
                            .loading(exporting)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.export_key(window, cx)),
                            ),
                    ),
                )
                .child(
                    step(
                        "2",
                        "Confirm it is saved",
                        "The key becomes active. From then on the backup is the only way to \
                         restore it.",
                    )
                    .child(
                        button::primary("key-confirm-backup")
                            .small()
                            .label("I saved it")
                            .loading(confirming)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.waiting("confirm receiving-key backup") {
                                    this.send(
                                        ClientCommand::ConfirmReceivingKeyBackup {
                                            generation_id: generation_id.clone(),
                                        },
                                        cx,
                                    );
                                }
                            })),
                    ),
                )
                .into_any_element(),
            KeyCustody::Locked { .. } => div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    option(
                        "Unlock with your password",
                        "Use the password that protected this key when it was created.",
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(field::secret(p, &self.key_password).id("key-password")),
                            )
                            .child(
                                button::primary("key-unlock")
                                    .icon("unlock")
                                    .label("Unlock")
                                    .loading(unlocking)
                                    .on_click(cx.listener(|this, _, _, cx| this.unlock_key(cx))),
                            ),
                    ),
                )
                .child(
                    option(
                        "Restore from a backup",
                        "Paste the contents of the backup file you saved for this key.",
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div().flex_1().min_w_0().child(
                                    field::secret(p, &self.key_import).id("key-import-value"),
                                ),
                            )
                            .child(
                                button::secondary("key-import")
                                    .label("Restore")
                                    .loading(importing)
                                    .on_click(cx.listener(|this, _, _, cx| this.import_key(cx))),
                            ),
                    ),
                )
                .into_any_element(),
            KeyCustody::Configured { .. } => surface::panel(p)
                .p(px(8.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(12.))
                        .py(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(text::body_strong("Key backup").text_color(p.text))
                                .child(
                                    text::small(
                                        "Save a copy you can restore from on another device.",
                                    )
                                    .text_color(p.text_muted),
                                ),
                        )
                        .child(
                            button::secondary("key-export")
                                .small()
                                .icon("download")
                                .label("Save backup…")
                                .loading(exporting)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.export_key(window, cx)),
                                ),
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
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(text::body_strong("Replace this key").text_color(p.text))
                                .child(
                                    text::small(
                                        "Only if it may have been exposed. What was sent to the \
                                         old key may never open again.",
                                    )
                                    .text_color(p.text_muted),
                                ),
                        )
                        .child(
                            button::danger("key-replace")
                                .small()
                                .label("Replace…")
                                .loading(creating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_replace(window, cx)
                                })),
                        ),
                )
                .into_any_element(),
            _ => div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    option(
                        "Protect it with my password",
                        "Recommended. The key is stored encrypted with your account password, so \
                         you can unlock it on any device you sign in to.",
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(field::secret(p, &self.key_password).id("key-password")),
                            )
                            .child(
                                button::primary("key-generate-password")
                                    .label("Create key")
                                    .loading(creating)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.create_key(true, cx)),
                                    ),
                            ),
                    ),
                )
                .child(
                    option(
                        "Keep it myself",
                        "Only this device holds the key. You save a backup file, and nobody else \
                         can restore it for you.",
                    )
                    .child(
                        div().flex().child(
                            button::secondary("key-generate-self")
                                .label("Create and back up")
                                .loading(creating)
                                .on_click(cx.listener(|this, _, _, cx| this.create_key(false, cx))),
                        ),
                    ),
                )
                .into_any_element(),
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(summary)
            .children(self.notice(Area::Key, p))
            .child(actions)
            .into_any_element()
    }
}
