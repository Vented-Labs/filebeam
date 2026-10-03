//! Native Account and Inbox panels. Secrets stay in retained input entities.

use std::{collections::HashSet, sync::Arc, time::Duration};

use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Task as GpuiTask, Window,
    div, px,
};
use gpui_component::{
    Disableable, Icon, WindowExt,
    button::{Button, ButtonVariants},
    dialog::DialogButtonProps,
    input::{Input, InputState},
    scroll::ScrollableElement,
    switch::Switch,
    theme::Theme,
};

use crate::{
    client::DesktopClient,
    model::{ClientCommand, DesktopSnapshot, NotificationChannel},
    theme::{Palette, palette_for},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Task {
    Login,
    Register,
    ResetRequest,
    Profile,
    Report,
    Delete,
    Keys,
    Contacts,
}

/// Retained Account form and account-scoped presentation state.
pub struct AccountPanel {
    client: Arc<DesktopClient>,
    snapshot: DesktopSnapshot,
    task: Task,
    pending: bool,
    pending_operation: Option<(String, usize, u64)>,
    clear_secret_drafts: bool,
    status: Option<String>,
    email: Entity<InputState>,
    username: Entity<InputState>,
    name: Entity<InputState>,
    password: Entity<InputState>,
    confirmation: Entity<InputState>,
    report_id: Entity<InputState>,
    report_category: Entity<InputState>,
    report_description: Entity<InputState>,
    key_import: Entity<InputState>,
    page_scroll: ScrollHandle,
    _refresh: GpuiTask<()>,
}

impl AccountPanel {
    pub fn new(client: Arc<DesktopClient>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this.update(cx, |panel, cx| panel.refresh(cx)).is_err() {
                    return;
                }
            }
        });
        let snapshot = client.snapshot();
        Self {
            task: if snapshot.account.authenticated {
                Task::Profile
            } else {
                Task::Login
            },
            snapshot,
            client,
            pending: false,
            pending_operation: None,
            clear_secret_drafts: false,
            status: None,
            email: input(window, cx, "you@example.com", false),
            username: input(window, cx, "username", false),
            name: input(window, cx, "Optional name", false),
            password: input(window, cx, "Password", true),
            confirmation: input(window, cx, "Required confirmation", false),
            report_id: input(window, cx, "Transfer ID", false),
            report_category: input(window, cx, "Category", false),
            report_description: input(window, cx, "Describe the issue", false),
            key_import: input(window, cx, "Paste approved private-key export", true),
            page_scroll: ScrollHandle::new(),
            _refresh: refresh,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.snapshot = self.client.snapshot();
        // A retained panel starts on the actual session state, rather than the
        // sign-in form that happened to be the historical constructor default.
        if self.snapshot.account.authenticated && self.task == Task::Login {
            self.task = Task::Profile;
        }
        if let Some((operation, operation_after, message_id_after)) = &self.pending_operation {
            if self
                .snapshot
                .messages
                .iter()
                .find(|message| message.id > *message_id_after && message.operation == *operation)
                .is_some()
            {
                self.pending = false;
                self.pending_operation = None;
                self.status = None;
            } else if self
                .snapshot
                .operations
                .iter()
                .skip(*operation_after)
                .any(|result| result.operation == *operation)
            {
                self.pending = false;
                self.pending_operation = None;
                self.status = Some("Completed.".into());
                self.clear_secret_drafts = true;
            }
        }
        cx.notify();
    }

    #[cfg(feature = "visual-test")]
    pub fn apply_visual_scenario(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.task = match id {
            "account-profile" => Task::Profile,
            "account-keys" | "account-key-wizard" => Task::Keys,
            "account-register" => Task::Register,
            "account-reset" => Task::ResetRequest,
            _ => Task::Login,
        };
        cx.notify();
    }

    fn text(input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().trim().to_owned()
    }
    fn secret(input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().to_string()
    }
    fn dispatch(&mut self, command: ClientCommand, window: &mut Window, cx: &mut Context<Self>) {
        let operation = command.operation().to_owned();
        let operation_after = self.snapshot.operations.len();
        let message_id_after = self
            .snapshot
            .messages
            .last()
            .map_or(0, |message| message.id);
        match self.client.dispatch(command) {
            Ok(()) => {
                self.pending = true;
                self.pending_operation = Some((operation, operation_after, message_id_after));
                self.status = Some("Working...".into());
            }
            Err(error) => {
                self.status = None;
                crate::toast::error(format!("{operation} failed"), error.to_string(), window, cx);
            }
        }
        cx.notify();
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let email = Self::text(&self.email, cx);
        let password = Self::secret(&self.password, cx);
        let username = Self::text(&self.username, cx);
        let name = Self::text(&self.name, cx);
        let confirmation = Self::text(&self.confirmation, cx);
        let report_id = Self::text(&self.report_id, cx);
        let report_category = Self::text(&self.report_category, cx);
        let report_description = Self::text(&self.report_description, cx);
        let values = FormValues {
            email: &email,
            password: &password,
            username: &username,
            confirmation: &confirmation,
            report_id: &report_id,
            report_category: &report_category,
            report_description: &report_description,
        };
        if let Err(error) = validate_form(self.task, &values) {
            self.status = Some(error.into());
            cx.notify();
            return;
        }
        match self.task {
            Task::Login => self.dispatch(
                ClientCommand::Login {
                    email,
                    password,
                    remember: true,
                },
                window,
                cx,
            ),
            Task::Register => self.dispatch(
                ClientCommand::Register {
                    username,
                    name: optional(name),
                    email,
                    password,
                },
                window,
                cx,
            ),
            Task::ResetRequest => {
                self.dispatch(ClientCommand::RequestPasswordReset { email }, window, cx)
            }
            Task::Report => self.dispatch(
                ClientCommand::Report {
                    transfer_id: report_id,
                    category: report_category,
                    description: report_description,
                    email: optional(email),
                },
                window,
                cx,
            ),
            Task::Delete => self.dispatch(
                ClientCommand::DeleteAccount {
                    current_password: password,
                    confirmation,
                },
                window,
                cx,
            ),
            Task::Profile | Task::Keys | Task::Contacts => {}
        }
    }
}

impl Render for AccountPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.clear_secret_drafts {
            for input in [&self.password, &self.key_import] {
                input.update(cx, |input, cx| input.set_value(String::new(), window, cx));
            }
            self.clear_secret_drafts = false;
        }
        let p = palette_for(Theme::global(cx).mode);
        let page = if self.snapshot.account.authenticated && self.task == Task::Profile {
            self.profile(p, window, cx)
        } else if self.snapshot.account.authenticated && self.task == Task::Keys {
            self.keys(p, window, cx)
        } else if self.snapshot.account.authenticated && self.task == Task::Contacts {
            self.contacts(p, cx)
        } else {
            self.form(p, window, cx)
        };
        let authenticated = self.snapshot.account.authenticated;
        let content = div()
            .w_full()
            .bg(p.page)
            .text_color(p.text)
            .child(if authenticated {
                div().flex_none().child(page)
            } else {
                div().w_full().flex().justify_center().child(page)
            });
        if authenticated {
            crate::views::page::scroller("account-page-scroll", &self.page_scroll, window, content)
                .into_any_element()
        } else {
            crate::views::page::centered_scroller(
                "account-page-scroll",
                &self.page_scroll,
                window,
                content,
            )
            .into_any_element()
        }
    }
}

impl AccountPanel {
    fn contacts(&mut self, p: Palette, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = card(p)
            .max_w(px(850.))
            .child(section_heading(
                p,
                "Contacts",
                "Mutual friends on this instance. Your overrides control only incoming files.",
            ))
            .child(
                Button::new("contacts-back")
                    .label("Account")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.task = Task::Profile;
                        cx.notify();
                    })),
            )
            .child(field(p, "Exact username", &self.username))
            .child(
                Button::new("contacts-request")
                    .label("Send friend request")
                    .disabled(self.pending)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let username = Self::text(&this.username, cx)
                            .trim_start_matches('@')
                            .to_lowercase();
                        this.dispatch(
                            ClientCommand::ContactAction {
                                username,
                                action: "request".into(),
                                can_send: None,
                                auto_download: None,
                            },
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("contacts-refresh")
                    .label("Refresh contacts")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dispatch(ClientCommand::RefreshContacts, window, cx)
                    })),
            );
        if let Some(data) = self.snapshot.contacts.clone() {
            let defaults = data.settings;
            let policy = defaults.receiving_policy.clone();
            let auto = defaults.auto_download_friends;
            body = body
                .child(
                    Button::new("receiving-policy")
                        .label(format!("Who can send: {policy} (change)"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let next = match policy.as_str() {
                                "anyone" => "authenticated",
                                "authenticated" => "friends",
                                "friends" => "nobody",
                                _ => "anyone",
                            };
                            this.dispatch(
                                ClientCommand::ReceivingDefaults {
                                    policy: next.into(),
                                    auto_download: auto,
                                },
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    Switch::new("auto-friends")
                        .label("Automatically download from friends")
                        .checked(auto)
                        .on_click(cx.listener(|this, value, window, cx| {
                            if let Some(data) = &this.snapshot.contacts {
                                this.dispatch(
                                    ClientCommand::ReceivingDefaults {
                                        policy: data.settings.receiving_policy.clone(),
                                        auto_download: *value,
                                    },
                                    window,
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    Switch::new("auto-this-client")
                        .label("Stage eligible deliveries on this desktop")
                        .checked(self.snapshot.auto_receiving)
                        .on_click(cx.listener(|this, value, window, cx| {
                            this.dispatch(ClientCommand::AutomaticReceiving(*value), window, cx)
                        })),
                );
            for (index, contact) in data.contacts.into_iter().enumerate() {
                let mut row = card(p).child(section_heading(
                    p,
                    &format!("@{}", contact.username),
                    &contact.status,
                ));
                let actions: &[(&str, &str)] = match contact.status.as_str() {
                    "incoming" => &[("Accept", "accept"), ("Decline", "decline")],
                    "outgoing" => &[("Cancel request", "cancel")],
                    _ => &[("Remove friend", "remove")],
                };
                for (action_index, (label, action)) in actions.iter().enumerate() {
                    let username = contact.username.clone();
                    let action = (*action).to_owned();
                    row = row.child(
                        Button::new(("contact-action", index * 4 + action_index))
                            .label(*label)
                            .disabled(self.pending)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.dispatch(
                                    ClientCommand::ContactAction {
                                        username: username.clone(),
                                        action: action.clone(),
                                        can_send: None,
                                        auto_download: None,
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    );
                }
                if contact.status == "accepted" {
                    for (field_index, (label, value)) in [
                        ("Can send me files", contact.can_send),
                        ("Automatic download", contact.auto_download),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let username = contact.username.clone();
                        let other = if field_index == 0 {
                            contact.auto_download
                        } else {
                            contact.can_send
                        };
                        row = row.child(
                            Button::new(("contact-override", index * 2 + field_index))
                                .label(format!(
                                    "{label}: {} (change)",
                                    match value {
                                        None => "Inherit",
                                        Some(true) => "Allow",
                                        Some(false) => "Deny",
                                    }
                                ))
                                .disabled(self.pending)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let next = match value {
                                        None => Some(true),
                                        Some(true) => Some(false),
                                        Some(false) => None,
                                    };
                                    this.dispatch(
                                        ClientCommand::ContactAction {
                                            username: username.clone(),
                                            action: "preferences".into(),
                                            can_send: if field_index == 0 { next } else { other },
                                            auto_download: if field_index == 1 {
                                                next
                                            } else {
                                                other
                                            },
                                        },
                                        window,
                                        cx,
                                    );
                                })),
                        );
                    }
                }
                let username = contact.username;
                row = row.child(
                    Button::new(("contact-block", index))
                        .label("Block")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(
                                ClientCommand::ContactAction {
                                    username: username.clone(),
                                    action: "block".into(),
                                    can_send: None,
                                    auto_download: None,
                                },
                                window,
                                cx,
                            )
                        })),
                );
                body = body.child(row);
            }
            for (index, contact) in data.blocked.into_iter().enumerate() {
                let username = contact.username;
                body = body.child(
                    Button::new(("contact-unblock", index))
                        .label(format!("Unblock @{username}"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(
                                ClientCommand::ContactAction {
                                    username: username.clone(),
                                    action: "unblock".into(),
                                    can_send: None,
                                    auto_download: None,
                                },
                                window,
                                cx,
                            )
                        })),
                );
            }
        }
        for (index, staged) in self
            .snapshot
            .staged_inbox
            .clone()
            .into_iter()
            .filter(|entry| entry.state != "dismissed")
            .enumerate()
        {
            let id = staged.id;
            let dismiss_id = id.clone();
            body = body
                .child(meta(
                    p,
                    "Private incoming ciphertext",
                    &format!("{} — {}", byte_size(staged.bytes), staged.state),
                ))
                .child(
                    Button::new(("save-staged-inbox", index))
                        .label("Verify and save staged files")
                        .disabled(staged.state != "staged-locked" || self.pending)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.save_staged(id.clone(), cx)),
                        ),
                )
                .child(
                    Button::new(("dismiss-staged-inbox", index))
                        .label("Remove local ciphertext")
                        .disabled(self.pending)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.dispatch(
                                ClientCommand::DismissStagedInbox {
                                    id: dismiss_id.clone(),
                                },
                                window,
                                cx,
                            )
                        })),
                );
        }
        body.child(status(p, self.status.as_deref()))
    }
    fn save_staged(&mut self, id: String, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose folder for verified files".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(destination) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.dispatch(
                    ClientCommand::SaveStagedInbox { id, destination },
                    window,
                    cx,
                )
            });
        })
        .detach();
    }
    fn form(&mut self, p: Palette, _window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let title = match self.task {
            Task::Register => "Create your account",
            Task::ResetRequest => "Reset your password",
            Task::Report => "Report a transfer",
            Task::Delete => "Delete account",
            _ => "Sign in to Filebeam",
        };
        let detail = match self.task {
            Task::Register => "Create a Filebeam account.",
            Task::ResetRequest => "Request a password reset email.",
            Task::Report => "Report an issue with a transfer.",
            Task::Delete => "This permanently deletes your Filebeam account.",
            _ => "Sign in with your Filebeam account.",
        };
        let mut body = card(p)
            .w_full()
            .max_w(px(560.))
            .child(section_heading(p, title, detail));
        body = match self.task {
            Task::Register => body
                .child(field(p, "Username", &self.username))
                .child(field(p, "Name", &self.name))
                .child(field(p, "Email", &self.email))
                .child(field(p, "Password", &self.password)),
            Task::ResetRequest => body.child(field(p, "Email", &self.email)),
            Task::Report => body
                .child(field(p, "Transfer ID", &self.report_id))
                .child(field(p, "Category", &self.report_category))
                .child(field(p, "Description", &self.report_description))
                .child(field(p, "Contact email", &self.email)),
            Task::Delete => body
                .child(field(p, "Current password", &self.password))
                .child(field(p, "Required confirmation", &self.confirmation)),
            _ => body.child(field(p, "Email", &self.email)).child(field(
                p,
                "Password",
                &self.password,
            )),
        };
        body.child(status(p, self.status.as_deref()))
            .child(
                Button::new("account-submit")
                    .label("Continue")
                    .primary()
                    .disabled(self.pending)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.task == Task::Delete {
                            this.confirm_delete(window, cx);
                        } else {
                            this.submit(window, cx);
                        }
                    })),
            )
            .child(auth_switches(p, self.task, cx))
            .children(
                [(
                    "account-reset-request",
                    "Forgot password?",
                    Task::ResetRequest,
                )]
                .into_iter()
                .map(|(id, label, task)| {
                    Button::new(id)
                        .label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.task = task;
                            cx.notify();
                        }))
                }),
            )
    }

    fn profile(&mut self, p: Palette, _window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let account = &self.snapshot.account;
        let username = account.username.as_deref().unwrap_or("unknown");
        let initials: String = username.chars().take(2).collect::<String>().to_uppercase();
        let receiving = account
            .profile_url
            .clone()
            .unwrap_or_else(|| "No receiving URL until a username is configured.".into());
        div()
            .max_w(px(1100.))
            .flex()
            .flex_col()
            .gap(px(crate::views::page::HEADER_GAP))
            .child(crate::views::page::heading(
                p,
                "Account",
                "Identity, receiving address, and account-scoped preferences.",
            ))
            .child(Button::new("account-contacts").label("Contacts and receiving permissions").on_click(cx.listener(|this, _, window, cx| {
                this.task = Task::Contacts;
                this.dispatch(ClientCommand::RefreshContacts, window, cx);
            })))
            .child(div().flex().flex_row().flex_wrap().gap(px(20.)).children([
                card(p).flex_1().min_w(px(370.)).child(
                    div().flex().items_center().gap(px(14.)).child(
                        div().size(px(50.)).flex().items_center().justify_center().rounded(px(14.)).bg(p.selected).text_color(p.accent).text_size(px(17.)).child(initials)
                    ).child(div().flex_1().child(div().text_size(px(20.)).child(username.to_owned())).child(div().text_size(px(12.)).text_color(p.muted).child(format!("@{username}")))).child(
                        div().px(px(9.)).py(px(5.)).rounded(px(8.)).bg(if account.email_verified { p.selected } else { p.inset }).text_size(px(11.)).text_color(if account.email_verified { p.success } else { p.warning }).child(if account.email_verified { "Verified" } else { "Verification required" })
                    )
                ).child(meta(p, "Email", account.email.as_deref().unwrap_or("Unknown")))
                .child(copy_value(p, "Receiving link", &receiving))
                .child(Button::new("resend-verification").label("Resend verification").disabled(self.pending).on_click(cx.listener(|this, _, window, cx| this.dispatch(ClientCommand::ResendVerification, window, cx)))),
                card(p).flex_1().min_w(px(290.)).child(section_title(p, "folder", "Inbox preferences"))
                    .child(
                        Switch::new("inbox-enabled")
                            .label("Accept account-addressed deliveries")
                            .checked(account.inbox_enabled)
                            .on_click(cx.listener(|this, checked, window, cx| {
                                this.dispatch(ClientCommand::SetInboxEnabled(*checked), window, cx)
                            })),
                    )
                    .child(
                        Button::new("notification-database")
                            .label(match account.notification_channel { Some(NotificationChannel::Mail) => "Use in-app notifications", _ => "In-app notifications enabled" })
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch(
                                    ClientCommand::SetNotificationChannel(
                                        NotificationChannel::Database,
                                    ),
                                    window, cx,
                                )
                            })),
                    )
                    .child(meta(p, "Delivery", "Account-addressed files and notes remain separate from transfer history.")),
            ]))
            .child(card(p).max_w(px(540.)).child(section_title(p, "key", "Receiving key"))
                    .child(meta(p, "Custody", custody_label(&account.key_custody)))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child("Private key material is never rendered, copied, or logged."),
                    )
                    .child(
                        Button::new("key-wizard")
                            .label("Manage receiving key")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.task = Task::Keys;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(Button::new("account-logout").label("Sign out").on_click(
                        cx.listener(|this, _, window, cx| this.dispatch(ClientCommand::Logout, window, cx)),
                    ))
                    .child(
                        Button::new("account-report")
                            .label("Report a transfer")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.task = Task::Report;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("account-delete")
                            .label("Delete account")
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.task = Task::Delete;
                                cx.notify();
                            })),
                    ),
            )
            .child(status(p, self.status.as_deref()))
    }

    fn keys(&mut self, p: Palette, _window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let custody = self.snapshot.account.key_custody.clone();
        let mut body = card(p).max_w(px(620.)).child(section_heading(
            p,
            "Receiving key",
            "Private material is only passed to the typed custody service; it is never rendered or logged.",
        ));
        match custody {
            crate::model::KeyCustody::Unknown => {
                body = body
                    .child(field(
                        p,
                        "Current password (password custody)",
                        &self.password,
                    ))
                    .child(
                        Button::new("key-generate-password")
                            .label("Generate password-protected key")
                            .primary()
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let password = Self::secret(&this.password, cx);
                                if password.is_empty() {
                                    this.status = Some(
                                        "Current password is required for password custody.".into(),
                                    );
                                    cx.notify();
                                } else {
                                    this.dispatch(
                                        ClientCommand::GenerateReceivingKey {
                                            password: Some(password),
                                            replace: false,
                                            acknowledge_old_key_loss: false,
                                        },
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        Button::new("key-generate-self")
                            .label("Generate self-custody key")
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch(
                                    ClientCommand::GenerateReceivingKey {
                                        password: None,
                                        replace: false,
                                        acknowledge_old_key_loss: false,
                                    },
                                    window,
                                    cx,
                                )
                            })),
                    );
            }
            crate::model::KeyCustody::PendingBackup {
                generation_id,
                fingerprint,
            } => {
                body = body
                    .child(meta(p, "Pending fingerprint", &fingerprint))
                    .child(
                        Button::new("key-export-pending")
                            .label("Export backup")
                            .primary()
                            .disabled(self.pending)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.export_key(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("key-confirm-backup")
                            .label("Confirm exported backup")
                            .disabled(self.pending)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.dispatch(
                                    ClientCommand::ConfirmReceivingKeyBackup {
                                        generation_id: generation_id.clone(),
                                    },
                                    window,
                                    cx,
                                )
                            })),
                    );
            }
            crate::model::KeyCustody::Locked {
                version,
                fingerprint,
                custody_mode,
            } => {
                body = body
                    .child(meta(
                        p,
                        "Locked key",
                        &format!("v{version} - {fingerprint} ({custody_mode})"),
                    ))
                    .child(field(p, "Key password", &self.password))
                    .child(field(p, "Private key export", &self.key_import))
                    .child(
                        Button::new("key-unlock")
                            .label("Unlock key")
                            .primary()
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let password = Self::secret(&this.password, cx);
                                if password.is_empty() {
                                    this.status = Some("Key password is required.".into());
                                    cx.notify();
                                } else {
                                    this.dispatch(
                                        ClientCommand::UnlockReceivingKey { password },
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        Button::new("key-import")
                            .label("Import private key export")
                            .disabled(self.pending)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.import_key(window, cx)),
                            ),
                    );
            }
            crate::model::KeyCustody::Configured {
                version,
                fingerprint,
                custody_mode,
            } => {
                body = body
                    .child(meta(
                        p,
                        "Configured key",
                        &format!("v{version} - {fingerprint} ({custody_mode})"),
                    ))
                    .child(
                        Button::new("key-export")
                            .label("Export recovery key")
                            .disabled(self.pending)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.export_key(window, cx)),
                            ),
                    )
                    .child(field(p, "Current password", &self.password))
                    .child(
                        Button::new("key-replace")
                            .label("Replace key and acknowledge old-key loss")
                            .danger()
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let password = Self::secret(&this.password, cx);
                                if password.is_empty() {
                                    this.status = Some(
                                        "Current password is required to replace the key.".into(),
                                    );
                                    cx.notify();
                                } else {
                                    this.confirm_replace(password, window, cx);
                                }
                            })),
                    );
            }
            legacy => body = body.child(status(p, Some(custody_label(&legacy)))),
        }
        body.child(status(p, self.status.as_deref())).child(
            Button::new("keys-back")
                .label("Back to account")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.task = Task::Profile;
                    cx.notify();
                })),
        )
    }

    fn export_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_new_path(
            &std::path::PathBuf::from("/"),
            Some("filebeam-receiving-key.txt"),
        );
        let entity = cx.entity();
        cx.spawn_in(window, async move |_this, cx| {
            let Ok(Ok(Some(destination))) = receiver.await else {
                return;
            };
            let _ = entity.update_in(cx, |panel, window, cx| {
                panel.dispatch(
                    ClientCommand::ExportReceivingKey { destination },
                    window,
                    cx,
                )
            });
        })
        .detach();
    }
    fn import_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let export = Self::secret(&self.key_import, cx);
        if export.is_empty() {
            self.status = Some("Paste the approved private-key export.".into());
            cx.notify();
            return;
        }
        self.dispatch(ClientCommand::ImportReceivingKey { export }, window, cx);
    }
    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            dialog
                .title("Delete account?")
                .child("This permanently deletes your Filebeam account and cannot be undone.")
                .button_props(DialogButtonProps::default().ok_text("Delete account"))
                .on_ok(move |_, window, cx| {
                    entity.update(cx, |panel, cx| panel.submit(window, cx));
                    true
                })
        });
    }
    fn confirm_replace(&mut self, password: String, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            let password = password.clone();
            dialog
                .title("Replace receiving key?")
                .child("Files addressed to the old receiving key may become unrecoverable.")
                .button_props(DialogButtonProps::default().ok_text("Replace key"))
                .on_ok(move |_, window, cx| {
                    entity.update(cx, |panel, cx| {
                        panel.dispatch(
                            ClientCommand::GenerateReceivingKey {
                                password: Some(password.clone()),
                                replace: true,
                                acknowledge_old_key_loss: true,
                            },
                            window,
                            cx,
                        )
                    });
                    true
                })
        });
    }
}

/// Retained inbox selection. Choosing an item never unlocks, consumes, or downloads it.
pub struct InboxPanel {
    client: Arc<DesktopClient>,
    snapshot: DesktopSnapshot,
    selected: Option<String>,
    selected_items: HashSet<String>,
    pending: bool,
    pending_operation: Option<(String, usize, u64)>,
    status: Option<String>,
    unlock_password: Entity<InputState>,
    page_scroll: ScrollHandle,
    _refresh: GpuiTask<()>,
}
impl InboxPanel {
    pub fn new(client: Arc<DesktopClient>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this.update(cx, |panel, cx| panel.refresh(cx)).is_err() {
                    return;
                }
            }
        });
        Self {
            snapshot: client.snapshot(),
            client,
            selected: None,
            selected_items: HashSet::new(),
            pending: false,
            pending_operation: None,
            status: None,
            unlock_password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Key password")
                    .masked(true)
            }),
            page_scroll: ScrollHandle::new(),
            _refresh: refresh,
        }
    }
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.snapshot = self.client.snapshot();
        if let Some((operation, operation_after, message_id_after)) = &self.pending_operation {
            if self
                .snapshot
                .messages
                .iter()
                .find(|message| message.id > *message_id_after && message.operation == *operation)
                .is_some()
            {
                self.pending = false;
                self.pending_operation = None;
                self.status = None;
            } else if self
                .snapshot
                .operations
                .iter()
                .skip(*operation_after)
                .any(|result| result.operation == *operation)
            {
                self.pending = false;
                self.pending_operation = None;
                self.status = Some("Completed.".into());
            }
        }
        cx.notify();
    }
    #[cfg(feature = "visual-test")]
    pub fn apply_visual_scenario(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = None;
        self.status = match id {
            "inbox-failed" => Some(
                "Inbox refresh failed. Existing deliveries remain available; try again.".into(),
            ),
            "inbox-loading" => Some("Refreshing Inbox...".into()),
            _ => None,
        };
        cx.notify();
    }
    fn dispatch(&mut self, command: ClientCommand, window: &mut Window, cx: &mut Context<Self>) {
        let operation = command.operation().to_owned();
        let operation_after = self.snapshot.operations.len();
        let message_id_after = self
            .snapshot
            .messages
            .last()
            .map_or(0, |message| message.id);
        match self.client.dispatch(command) {
            Ok(()) => {
                self.pending = true;
                self.pending_operation = Some((operation, operation_after, message_id_after));
                self.status = Some("Working...".into());
            }
            Err(error) => {
                self.status = None;
                crate::toast::error(format!("{operation} failed"), error.to_string(), window, cx);
            }
        };
        cx.notify();
    }
    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let entity = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            let id = id.clone();
            dialog
                .title("Delete inbox delivery?")
                .child("This permanently deletes the selected delivery from your Inbox.")
                .button_props(DialogButtonProps::default().ok_text("Delete delivery"))
                .on_ok(move |_, window, cx| {
                    entity.update(cx, |panel, cx| {
                        panel.dispatch(
                            ClientCommand::DeleteInboxItem { id: id.clone() },
                            window,
                            cx,
                        )
                    });
                    true
                })
        });
    }
}
impl Render for InboxPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette_for(Theme::global(cx).mode);
        let account = &self.snapshot.account;
        let username = account.username.as_deref().unwrap_or("your account");
        let initials: String = username.chars().take(2).collect::<String>().to_uppercase();
        let mut body = div().max_w(px(1200.)).flex().flex_col().gap(px(16.)).child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .p(px(16.))
                .rounded(px(12.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(
                    div()
                        .size(px(38.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(11.))
                        .bg(p.selected)
                        .text_color(p.accent)
                        .child(initials),
                )
                .child(
                    div()
                        .flex_1()
                        .child(div().text_size(px(13.)).child(format!("@{username}")))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .child("Your receiving address"),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(match account.key_custody {
                            crate::model::KeyCustody::Locked { .. } => p.warning,
                            _ => p.success,
                        })
                        .child(match account.key_custody {
                            crate::model::KeyCustody::Locked { .. } => "Key locked",
                            _ => "Receiving key",
                        }),
                ),
        );
        if !self.snapshot.account.authenticated {
            body = body.child(empty_state(
                p,
                "IN",
                "Your private Inbox",
                "Sign in to receive files and notes addressed to your username.",
            ));
        } else if !self.snapshot.account.inbox_enabled {
            body = body.child(empty_state(
                p,
                "KEY",
                "Inbox delivery is disabled",
                "Enable account-addressed deliveries before accepting new Inbox items.",
            ));
        } else if matches!(
            self.snapshot.account.key_custody,
            crate::model::KeyCustody::Locked { .. }
        ) {
            body = body.child(card(p).max_w(px(480.)).child(empty_state(p, "LOCK", "Your Inbox is locked", "Unlock your receiving key to view protected items. Metadata remains concealed until then.")).child(field(p, "Key password", &self.unlock_password)).child(Button::new("inbox-unlock").label("Unlock Inbox").primary().disabled(self.pending).on_click(cx.listener(|this, _, window, cx| { let password = this.unlock_password.read(cx).value().trim().to_owned(); if password.is_empty() { this.status = Some("Key password is required.".into()); cx.notify(); } else { this.dispatch(ClientCommand::UnlockReceivingKey { password }, window, cx); } }))));
        } else if self.snapshot.inbox.is_empty() {
            body = body.child(empty_state(
                p,
                "IN",
                "Your Inbox is ready",
                "Files and notes addressed to your username will appear here.",
            ));
        } else {
            body = body.flex_1().min_h_0();
            let mut list = card(p)
                .id("inbox-list-scroll")
                .flex_1()
                .min_w(px(360.))
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(0.))
                .gap(px(0.))
                .child(
                    div()
                        .px(px(16.))
                        .py(px(14.))
                        .border_b_1()
                        .border_color(p.border_soft)
                        .child(section_title(p, "folder", "Shared with you"))
                        .child(
                            div()
                                .mt(px(4.))
                                .text_size(px(10.))
                                .text_color(p.subtle)
                                .child(format!("{} delivery record(s)", self.snapshot.inbox.len())),
                        ),
                );
            for item in &self.snapshot.inbox {
                let id = item.id.clone();
                let selected = self.selected.as_deref() == Some(item.id.as_str());
                list = list.child(inbox_row(
                    item,
                    selected,
                    p,
                    cx.listener(move |this, _, window, cx| {
                        this.selected = Some(id.clone());
                        this.selected_items.clear();
                        this.dispatch(ClientCommand::InspectInbox { id: id.clone() }, window, cx);
                        cx.notify();
                    }),
                ));
            }
            list = list.child(
                div()
                    .mt_auto()
                    .px(px(16.))
                    .pt(px(10.))
                    .pb(px(10.))
                    .flex()
                    .justify_between()
                    .border_t_1()
                    .border_color(p.border_soft)
                    .text_size(px(10.))
                    .text_color(p.subtle)
                    .child("Account-addressed content")
                    .child("Separate from transfers"),
            );
            let mut detail = card(p)
                .id("inbox-detail-scroll")
                .w(px(310.))
                .flex_none()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(16.))
                .child(section_title(p, "shield", "Item details"));
            if let Some(id) = self.selected.clone() {
                let item_ids = self
                    .snapshot
                    .inbox
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.item_ids.clone())
                    .unwrap_or_default();
                if item_ids.is_empty() {
                    detail = detail.child(status(p, Some("Loading authenticated delivery metadata. Filenames remain concealed until receipt.")));
                } else {
                    for (index, item_id) in item_ids.into_iter().enumerate() {
                        let checked = self.selected_items.contains(&item_id);
                        let item_id_for_click = item_id.clone();
                        detail = detail.child(
                            Button::new(("inbox-part", index))
                                .label(format!(
                                    "{}Encrypted item {item_id}",
                                    if checked { "Selected: " } else { "" }
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.selected_items.insert(item_id_for_click.clone()) {
                                        this.selected_items.remove(&item_id_for_click);
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                }
                detail = detail.child(meta(p, "Selected delivery", &id))
                    .child(Button::new("receive-inbox-delivery").label("Download selected delivery").primary().disabled(self.pending).on_click(cx.listener(|this, _, window, cx| { if let Some(id) = this.selected.clone() { let item_ids = (!this.selected_items.is_empty()).then(|| this.selected_items.iter().cloned().collect()); this.dispatch(ClientCommand::ReceiveInbox { id, item_ids }, window, cx); } })))
                    .child(Button::new("delete-inbox-item").label("Delete selected item").danger().disabled(self.pending).on_click(cx.listener(|this, _, window, cx| this.confirm_delete(window, cx))))
                    .child(status(p, Some("Item IDs are authenticated delivery metadata; filenames remain encrypted until receipt.")));
            } else {
                detail = detail.child(status(
                    p,
                    Some("Select a delivery to inspect its authenticated metadata."),
                ));
            }
            body = body.child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .gap(px(18.))
                    .child(list)
                    .child(detail),
            );
        }
        let content = div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(crate::views::page::HEADER_GAP))
            .child(crate::views::page::heading(
                p,
                "Inbox",
                "Files and notes delivered to your username.",
            ))
            .child(body)
            .child(status(p, self.status.as_deref()));
        crate::views::page::scroller(
            "inbox-page-scroll",
            &self.page_scroll,
            window,
            div().bg(p.page).text_color(p.text).child(content),
        )
    }
}

fn inbox_row(
    item: &crate::model::InboxItem,
    selected: bool,
    p: Palette,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("inbox-delivery-{}", item.id)))
        .h(px(80.))
        .px(px(16.))
        .flex()
        .items_center()
        .gap(px(12.))
        .border_b_1()
        .border_color(p.border_soft)
        .bg(if selected { p.selected } else { p.surface })
        .hover(|style| style.bg(p.hover))
        .active(|style| style.bg(p.selected))
        .child(
            div()
                .size(px(34.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(p.raised)
                .text_color(p.accent)
                .child(Icon::default().path("icons/folder.svg").size(px(17.))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(12.))
                        .child(format!("{} encrypted item(s)", item.item_count)),
                )
                .child(
                    div()
                        .mt(px(3.))
                        .text_size(px(10.))
                        .text_color(p.subtle)
                        .child(format!(
                            "{} · {}",
                            byte_size(item.ciphertext_bytes),
                            item.completed_at
                        )),
                ),
        )
        .child(
            div()
                .w(px(104.))
                .text_size(px(10.))
                .text_color(if selected { p.accent } else { p.subtle })
                .child(if selected { "Selected" } else { "Inspect" }),
        )
        .on_click(on_click)
}

fn input(
    window: &mut Window,
    cx: &mut Context<AccountPanel>,
    placeholder: &'static str,
    masked: bool,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .masked(masked)
    })
}
fn optional(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
fn byte_size(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = value as f64;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
struct FormValues<'a> {
    email: &'a str,
    password: &'a str,
    username: &'a str,
    confirmation: &'a str,
    report_id: &'a str,
    report_category: &'a str,
    report_description: &'a str,
}

fn validate_form(task: Task, values: &FormValues<'_>) -> Result<(), &'static str> {
    let required = |value: &str, label| if value.is_empty() { Err(label) } else { Ok(()) };
    match task {
        Task::Login => required(values.email, "Email is required")
            .and_then(|_| required(values.password, "Password is required")),
        Task::Register => required(values.username, "Username is required")
            .and_then(|_| required(values.email, "Email is required"))
            .and_then(|_| required(values.password, "Password is required")),
        Task::ResetRequest => required(values.email, "Email is required"),
        Task::Report => required(values.report_id, "Transfer ID is required")
            .and_then(|_| required(values.report_category, "Report category is required"))
            .and_then(|_| required(values.report_description, "Report description is required")),
        Task::Delete => required(values.password, "Current password is required")
            .and_then(|_| required(values.confirmation, "Confirmation is required")),
        Task::Profile | Task::Keys | Task::Contacts => Ok(()),
    }
}
fn card(p: Palette) -> gpui::Div {
    div()
        .p(px(23.))
        .flex()
        .flex_col()
        .gap(px(18.))
        .rounded(px(12.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
}
fn section_title(p: Palette, icon: &str, title: &str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(p.selected)
                .text_color(p.accent)
                .child(
                    Icon::default()
                        .path(format!("icons/{icon}.svg"))
                        .size(px(15.)),
                ),
        )
        .child(div().text_size(px(18.)).child(title.to_owned()))
}
fn copy_value(p: Palette, label: &str, value: &str) -> gpui::Div {
    div()
        .p(px(10.))
        .rounded(px(8.))
        .bg(p.inset)
        .child(
            div()
                .text_size(px(10.))
                .text_color(p.subtle)
                .child(label.to_owned()),
        )
        .child(div().text_size(px(12.)).child(value.to_owned()))
}
fn empty_state(p: Palette, icon: &str, title: &str, detail: &str) -> gpui::Div {
    div()
        .min_h(px(180.))
        .p(px(23.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .rounded(px(12.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .px(px(8.))
                .py(px(5.))
                .rounded(px(7.))
                .bg(p.selected)
                .text_color(p.accent)
                .child(icon.to_owned()),
        )
        .child(div().text_size(px(18.)).child(title.to_owned()))
        .child(
            div()
                .max_w(px(360.))
                .text_size(px(12.))
                .text_color(p.muted)
                .child(detail.to_owned()),
        )
}
fn auth_switches(p: Palette, task: Task, cx: &mut Context<AccountPanel>) -> gpui::Div {
    let (primary, secondary, target) = match task {
        Task::Login => ("New to Filebeam?", "Create account", Task::Register),
        Task::Register => ("Already have an account?", "Sign in", Task::Login),
        Task::ResetRequest => ("Remembered your password?", "Sign in", Task::Login),
        _ => ("", "", Task::Login),
    };
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(div().text_size(px(11.)).text_color(p.muted).child(primary))
        .child(
            Button::new("account-switch-task")
                .label(secondary)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.task = target;
                    cx.notify();
                })),
        )
}
fn section_heading(p: Palette, title: &str, detail: &str) -> gpui::Div {
    div()
        .child(div().text_size(px(18.)).child(title.to_owned()))
        .child(
            div()
                .text_size(px(12.))
                .text_color(p.muted)
                .child(detail.to_owned()),
        )
}
fn field(p: Palette, label: &str, input: &Entity<InputState>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .text_size(px(11.))
                .text_color(p.muted)
                .child(label.to_owned()),
        )
        .child(Input::new(input).w_full().bordered(true))
}
fn meta(p: Palette, label: &str, value: &str) -> gpui::Div {
    div()
        .p(px(10.))
        .rounded(px(8.))
        .bg(p.inset)
        .child(
            div()
                .text_size(px(10.))
                .text_color(p.subtle)
                .child(label.to_owned()),
        )
        .child(div().text_size(px(12.)).child(value.to_owned()))
}
fn status(p: Palette, value: Option<&str>) -> gpui::Div {
    div()
        .text_size(px(11.))
        .text_color(p.muted)
        .child(value.unwrap_or("").to_owned())
}
fn custody_label(value: &crate::model::KeyCustody) -> &'static str {
    match value {
        crate::model::KeyCustody::Unknown => "Not configured",
        crate::model::KeyCustody::Local => "Local",
        crate::model::KeyCustody::PasswordWrapped => "Password protected",
        crate::model::KeyCustody::ServerWrapped => "Server wrapped",
        crate::model::KeyCustody::PendingBackup { .. } => "Backup required",
        crate::model::KeyCustody::Locked { .. } => "Locked",
        crate::model::KeyCustody::Configured { .. } => "Configured",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_form_values_preserve_nonempty_drafts() {
        assert_eq!(optional("value".into()).as_deref(), Some("value"));
        assert_eq!(optional(String::new()), None);
    }

    #[test]
    fn login_validation_requires_secret_input() {
        assert_eq!(
            validate_form(
                Task::Login,
                &FormValues {
                    email: "a@example.test",
                    password: "",
                    username: "",
                    confirmation: "",
                    report_id: "",
                    report_category: "",
                    report_description: "",
                },
            ),
            Err("Password is required")
        );
    }
}
