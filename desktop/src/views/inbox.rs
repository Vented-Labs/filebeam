//! The account inbox: deliveries addressed to the signed-in username. Choosing one
//! never downloads or decrypts it, and filenames stay encrypted until it is received.

use std::{collections::HashSet, sync::Arc, time::Duration};

use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    input::{InputEvent, InputState},
    scroll::ScrollableElement,
};

use crate::{
    client::DesktopClient,
    live::Live,
    model::{AccountSnapshot, ClientCommand, DesktopSnapshot, KeyCustody},
    request::{Outcome, Request},
    shell::{OpenAccount, OpenTransfers},
    theme::{
        self, Palette,
        tokens::{geometry, radius},
    },
    ui::{
        MotionExt, TextExt, Tone, button, dialog, feedback, field, format, motion, page, row,
        surface, text,
    },
};

const DETAIL_WIDTH: f32 = 372.;
const DETAIL_WIDTH_COMPACT: f32 = 316.;
/// The state of a copy this device fetched that is ready to be decrypted and saved.
const STAGED: &str = "staged-locked";

/// What stands between the person and their deliveries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    SignedOut,
    Disabled,
    NoKey,
    BackupPending,
    Locked,
    Open,
}

fn gate(account: &AccountSnapshot) -> Gate {
    if !account.authenticated {
        Gate::SignedOut
    } else if !account.inbox_enabled {
        Gate::Disabled
    } else {
        match account.key_custody {
            KeyCustody::Unknown => Gate::NoKey,
            KeyCustody::PendingBackup { .. } => Gate::BackupPending,
            KeyCustody::Locked { .. } => Gate::Locked,
            _ => Gate::Open,
        }
    }
}

/// One row: a delivery on the server, a copy this device already fetched, or both.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Delivery {
    id: String,
    bytes: u64,
    item_count: Option<u64>,
    received_at: Option<String>,
    expires_at: Option<String>,
    parts: Vec<String>,
    /// State of the automatically fetched copy, when there is one.
    local: Option<String>,
    on_server: bool,
}

impl Delivery {
    fn staged(&self) -> bool {
        self.local.as_deref() == Some(STAGED)
    }

    fn title(&self) -> String {
        match self.item_count {
            Some(count) => format::plural(count, "encrypted item"),
            None => "Encrypted delivery".into(),
        }
    }

    fn local_status(&self) -> Option<(&'static str, Tone)> {
        match self.local.as_deref()? {
            STAGED => Some(("On this device", Tone::Success)),
            "downloading" => Some(("Downloading", Tone::Accent)),
            "paused-local" => Some(("Paused", Tone::Warning)),
            _ => None,
        }
    }
}

fn deliveries(snapshot: &DesktopSnapshot) -> Vec<Delivery> {
    let local = |id: &str| {
        snapshot
            .staged_inbox
            .iter()
            .find(|entry| entry.id == id && entry.state != "dismissed")
            .map(|entry| entry.state.clone())
    };
    let mut rows: Vec<Delivery> = snapshot
        .inbox
        .iter()
        .map(|item| Delivery {
            id: item.id.clone(),
            bytes: item.ciphertext_bytes,
            item_count: Some(item.item_count),
            received_at: Some(item.completed_at.clone()),
            expires_at: Some(item.expires_at.clone()),
            parts: item.item_ids.clone(),
            local: local(&item.id),
            on_server: true,
        })
        .collect();
    // A fetched copy can outlive its delivery on the server. It still has to be
    // saved or removed, so it keeps a row of its own.
    rows.extend(
        snapshot
            .staged_inbox
            .iter()
            .filter(|entry| {
                entry.state == STAGED && !snapshot.inbox.iter().any(|item| item.id == entry.id)
            })
            .map(|entry| Delivery {
                id: entry.id.clone(),
                bytes: entry.bytes,
                item_count: None,
                received_at: None,
                expires_at: None,
                parts: Vec::new(),
                local: Some(entry.state.clone()),
                on_server: false,
            }),
    );
    rows
}

/// A short result shown above the list until the next action.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Flash {
    Started,
    Saved,
    Failed(String),
}

/// Retained inbox selection and its pending requests.
pub struct InboxPanel {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    selected: Option<String>,
    /// Parts left out of the next download. Everything is included to begin with.
    excluded: HashSet<String>,
    key_password: Entity<InputState>,
    requests: Vec<Request>,
    unlock_error: Option<String>,
    flash: Option<Flash>,
    copied: bool,
    list_scroll: ScrollHandle,
}

impl InboxPanel {
    pub fn new(live: Entity<Live>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = live.read(cx).client().clone();
        let key_password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Key password")
                .masked(true)
        });
        cx.subscribe_in(
            &key_password,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    if this.unlock_error.take().is_some() {
                        cx.notify();
                    }
                }
                InputEvent::PressEnter { .. } => this.unlock(window, cx),
                _ => {}
            },
        )
        .detach();
        cx.observe_in(&live, window, |this, live, window, cx| {
            let snapshot = live.read(cx).snapshot();
            this.on_snapshot(&snapshot, window, cx);
        })
        .detach();
        Self {
            client,
            live,
            selected: None,
            excluded: HashSet::new(),
            key_password,
            requests: Vec::new(),
            unlock_error: None,
            flash: None,
            copied: false,
            list_scroll: ScrollHandle::new(),
        }
    }

    fn snapshot(&self, cx: &Context<Self>) -> Arc<DesktopSnapshot> {
        self.live.read(cx).snapshot()
    }

    /// Asks for the current deliveries. The shell calls this when the page opens.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if gate(&self.snapshot(cx).account) == Gate::Open && !self.waiting("refresh inbox") {
            self.send(ClientCommand::RefreshInbox, cx);
        }
    }

    fn waiting(&self, operation: &str) -> bool {
        self.requests
            .iter()
            .any(|request| request.operation() == operation)
    }

    /// Settles answered requests and drops a selection whose delivery is gone.
    fn on_snapshot(
        &mut self,
        snapshot: &DesktopSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut settled = Vec::new();
        self.requests
            .retain(|request| match request.outcome(snapshot) {
                Outcome::Waiting => true,
                outcome => {
                    settled.push((request.operation(), outcome));
                    false
                }
            });
        for (operation, outcome) in settled {
            match (operation, outcome) {
                ("unlock receiving key", Outcome::Completed) => {
                    self.key_password
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    self.unlock_error = None;
                    self.send(ClientCommand::RefreshInbox, cx);
                }
                ("unlock receiving key", Outcome::Failed(_)) => {
                    // The service never repeats why a secret was refused.
                    self.unlock_error =
                        Some("That password did not unlock your key. Try again.".into());
                }
                ("receive inbox", Outcome::Completed) => self.flash = Some(Flash::Started),
                ("save staged inbox", Outcome::Completed) => self.flash = Some(Flash::Saved),
                ("delete inbox item" | "remove local staging", Outcome::Completed) => {
                    self.send(ClientCommand::RefreshInbox, cx);
                }
                (_, Outcome::Failed(detail)) => {
                    self.flash = Some(Flash::Failed(format::readable_error(&detail)));
                }
                _ => {}
            }
        }
        if let Some(id) = &self.selected
            && !deliveries(snapshot)
                .iter()
                .any(|delivery| &delivery.id == id)
        {
            self.selected = None;
            self.excluded.clear();
        }
        cx.notify();
    }

    fn send(&mut self, command: ClientCommand, cx: &mut Context<Self>) {
        match Request::send(&self.client, command) {
            Ok(request) => self.requests.push(request),
            Err(error) => {
                self.flash = Some(Flash::Failed(format::readable_error(&error.to_string())));
            }
        }
        cx.notify();
    }

    fn unlock(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.waiting("unlock receiving key") {
            return;
        }
        let password = self.key_password.read(cx).value().to_string();
        if password.is_empty() {
            self.unlock_error = Some("Enter your key password.".into());
            cx.notify();
            return;
        }
        self.unlock_error = None;
        self.send(ClientCommand::UnlockReceivingKey { password }, cx);
    }

    fn select(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(id.as_str()) {
            return;
        }
        let snapshot = self.snapshot(cx);
        let Some(delivery) = deliveries(&snapshot)
            .into_iter()
            .find(|delivery| delivery.id == id)
        else {
            return;
        };
        self.excluded.clear();
        self.flash = None;
        self.copied = false;
        // Parts are listed once; the service keeps them for later visits.
        if delivery.on_server && delivery.parts.is_empty() {
            self.send(ClientCommand::InspectInbox { id: id.clone() }, cx);
        }
        self.selected = Some(id);
        cx.notify();
    }

    fn toggle_part(&mut self, part: String, included: bool, cx: &mut Context<Self>) {
        if included {
            self.excluded.remove(&part);
        } else {
            self.excluded.insert(part);
        }
        cx.notify();
    }

    fn download(&mut self, delivery: &Delivery, cx: &mut Context<Self>) {
        let chosen: Vec<String> = delivery
            .parts
            .iter()
            .filter(|part| !self.excluded.contains(*part))
            .cloned()
            .collect();
        // Naming every part is the same as naming none, and works before they load.
        let item_ids =
            (!chosen.is_empty() && chosen.len() < delivery.parts.len()).then_some(chosen);
        self.flash = None;
        self.send(
            ClientCommand::ReceiveInbox {
                id: delivery.id.clone(),
                item_ids,
            },
            cx,
        );
    }

    fn save_copy(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Save here".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(destination) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.flash = None;
                this.send(ClientCommand::SaveStagedInbox { id, destination }, cx);
            });
        })
        .detach();
    }

    fn confirm_delete(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        dialog::prompt("Delete this delivery?")
            .body(
                "It is removed from your inbox for good. Anything already downloaded stays on \
                 this device.",
            )
            .confirm("Delete")
            .destructive()
            .open(window, cx, move |_, cx| {
                entity.update(cx, |this, cx| {
                    this.flash = None;
                    this.send(ClientCommand::DeleteInboxItem { id: id.clone() }, cx);
                });
            });
    }

    fn confirm_remove_copy(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        dialog::prompt("Remove the copy on this device?")
            .body(
                "The encrypted copy this device fetched is deleted. The delivery stays in your \
                 inbox until it expires.",
            )
            .confirm("Remove copy")
            .destructive()
            .open(window, cx, move |_, cx| {
                entity.update(cx, |this, cx| {
                    this.flash = None;
                    this.send(ClientCommand::DismissStagedInbox { id: id.clone() }, cx);
                });
            });
    }

    fn copy_address(&mut self, address: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(address));
        self.copied = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for InboxPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.snapshot(cx);
        let gate = gate(&snapshot.account);
        let refreshing = self.waiting("refresh inbox");
        let refresh = (gate == Gate::Open).then(|| {
            button::secondary("inbox-refresh")
                .icon("updates")
                .label("Refresh")
                .loading(refreshing)
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
        });
        let detail = match snapshot.account.username.as_deref() {
            Some(username) if gate == Gate::Open => {
                format!("Files and notes sent to @{username}.")
            }
            _ => "Files and notes sent to your username.".to_owned(),
        };
        let header = page::header_with(p, "Inbox", detail, div().children(refresh), window, cx);
        let body = match gate {
            Gate::Open => self.open(&snapshot, p, window, cx),
            Gate::Locked => self.locked(p, cx),
            closed => self.closed(closed, p),
        };
        let entrance = motion::entrance(
            SharedString::from(format!("inbox-body-{gate:?}")),
            1,
            window,
            cx,
        );
        page::fixed(
            window,
            geometry::CONTENT_MAX,
            div()
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(header)
                .child(div().flex_1().min_h_0().entering(entrance).child(body)),
        )
    }
}

impl InboxPanel {
    /// A full-page statement with one way forward.
    fn closed(&mut self, gate: Gate, p: Palette) -> AnyElement {
        let (icon, title, body, action) = match gate {
            Gate::SignedOut => (
                "archive",
                "Your private inbox",
                "Sign in to receive files and notes sent to your username.",
                "Sign in",
            ),
            Gate::Disabled => (
                "bell-off",
                "Inbox delivery is off",
                "Turn it on in Account to let people send to your username.",
                "Open Account",
            ),
            Gate::NoKey => (
                "key",
                "Set up your receiving key",
                "Deliveries are encrypted to a key only you hold. Create one to open your inbox.",
                "Set up in Account",
            ),
            _ => (
                "shield",
                "Finish backing up your key",
                "Your new receiving key is waiting for its backup. Confirm it in Account.",
                "Open Account",
            ),
        };
        let action = button::primary("inbox-gate-action")
            .label(action)
            .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenAccount), cx))
            .into_any_element();
        surface::panel(p)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(feedback::empty_state(icon, title, body).action(Some(action)))
            .into_any_element()
    }

    fn locked(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let unlocking = self.waiting("unlock receiving key");
        surface::panel(p)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(400.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(feedback::empty_state(
                        "lock",
                        "Your inbox is locked",
                        "Enter your key password to see what was sent to you. It never leaves \
                         this device.",
                    ))
                    .child(
                        div()
                            .w_full()
                            .px(px(24.))
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(field::secret(p, &self.key_password).id("inbox-key-password"))
                            .when_some(self.unlock_error.clone(), |this, error| {
                                this.child(field::hint(p, error, Tone::Danger))
                            })
                            .child(
                                button::primary("inbox-unlock")
                                    .fill()
                                    .icon("unlock")
                                    .label("Unlock inbox")
                                    .loading(unlocking)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.unlock(window, cx)),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn open(
        &mut self,
        snapshot: &DesktopSnapshot,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = deliveries(snapshot);
        if rows.is_empty() {
            let address = snapshot.account.profile_url.clone();
            let action = address.map(|address| {
                button::secondary("inbox-copy-address")
                    .icon(if self.copied { "check" } else { "copy" })
                    .label(if self.copied {
                        "Copied"
                    } else {
                        "Copy my receiving link"
                    })
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.copy_address(address.clone(), cx)),
                    )
                    .into_any_element()
            });
            return surface::panel(p)
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .children(
                    self.flash_notice(p, cx)
                        .map(|notice| div().w_full().max_w(px(460.)).px(px(24.)).child(notice)),
                )
                .child(
                    feedback::empty_state(
                        "archive",
                        "Nothing here yet",
                        "Files and notes people send to your username appear here.",
                    )
                    .action(action),
                )
                .into_any_element();
        }

        let compact = page::is_compact(window);
        let selected = self
            .selected
            .as_ref()
            .and_then(|id| rows.iter().find(|delivery| &delivery.id == id))
            .cloned();
        let count = rows.len();
        let list = div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(18.))
                    .py(px(14.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text::eyebrow("Sent to you").text_color(p.text_subtle))
                    .child(
                        text::caption(format::count(count as u64, "delivery", "deliveries"))
                            .text_color(p.text_subtle),
                    ),
            )
            .child(surface::hairline(p))
            .child(
                div()
                    .id("inbox-list-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.list_scroll)
                    .p(px(6.))
                    .flex()
                    .flex_col()
                    .children(rows.into_iter().map(|delivery| self.row(delivery, p, cx))),
            );
        let detail = self.detail(selected, p, window, cx);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(self.flash_notice(p, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(16.))
                    .child(
                        surface::panel(p)
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(list),
                    )
                    .child(
                        surface::panel(p)
                            .w(px(if compact {
                                DETAIL_WIDTH_COMPACT
                            } else {
                                DETAIL_WIDTH
                            }))
                            .flex_none()
                            .min_h_0()
                            .overflow_hidden()
                            .child(detail),
                    ),
            )
            .into_any_element()
    }

    fn flash_notice(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let flash = self.flash.clone()?;
        let (message, tone) = match &flash {
            Flash::Started => ("The download has started.".to_owned(), Tone::Accent),
            Flash::Saved => (
                "Verified and saved to the folder you chose.".to_owned(),
                Tone::Success,
            ),
            Flash::Failed(detail) => (detail.clone(), Tone::Danger),
        };
        Some(
            feedback::notice(p, message, tone)
                .items_center()
                .when(flash == Flash::Started, |this| {
                    this.child(
                        button::quiet("inbox-view-transfers")
                            .small()
                            .label("View in Transfers")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenTransfers), cx)
                            }),
                    )
                })
                .child(
                    button::icon_button("inbox-dismiss-flash", "x", "Dismiss")
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.flash = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn row(
        &self,
        delivery: Delivery,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.selected.as_deref() == Some(delivery.id.as_str());
        let id = delivery.id.clone();
        let received = delivery
            .received_at
            .as_deref()
            .map(format::relative)
            .unwrap_or_else(|| "No longer on the server".into());
        div()
            .id(SharedString::from(format!(
                "inbox-delivery-{}",
                delivery.id
            )))
            .w_full()
            .flex_none()
            .py(px(2.))
            .child(
                div()
                    .w_full()
                    .h(px(64.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(12.))
                    .rounded(px(radius::CONTROL + 2.))
                    .border_1()
                    .cursor_pointer()
                    .when(selected, |this| {
                        this.bg(p.selected_surface)
                            .border_color(p.choice_border.opacity(0.55))
                    })
                    .when(!selected, |this| {
                        this.border_color(gpui::transparent_black())
                            .hover(move |style| style.bg(p.hover))
                    })
                    .child(row::icon_tile(p, "archive", 40., true))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                text::body_strong(delivery.title())
                                    .single_line()
                                    .text_color(p.text),
                            )
                            .child(
                                text::caption(format!(
                                    "{} · {received}",
                                    format::bytes(delivery.bytes)
                                ))
                                .single_line()
                                .text_color(p.text_muted),
                            ),
                    )
                    .children(
                        delivery
                            .local_status()
                            .map(|(label, tone)| feedback::pill(p, label, tone)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
    }

    fn detail(
        &mut self,
        delivery: Option<Delivery>,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(delivery) = delivery else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    feedback::empty_state(
                        "info",
                        "Select a delivery",
                        "Choose what to download. Names stay encrypted until you do.",
                    )
                    .tone(Tone::Neutral),
                )
                .into_any_element();
        };
        let entrance = motion::entrance_with(
            SharedString::from(format!("inbox-detail-{}", delivery.id)),
            0,
            8.,
            motion::enter_quick(),
            window,
            cx,
        );
        let downloading = self.waiting("receive inbox");
        let saving = self.waiting("save staged inbox");
        let deleting = self.waiting("delete inbox item");
        let removing = self.waiting("remove local staging");
        let included = delivery
            .parts
            .iter()
            .filter(|part| !self.excluded.contains(*part))
            .count();
        let partial = included < delivery.parts.len();

        let mut facts = vec![("Size", format::bytes(delivery.bytes))];
        if let Some(received) = &delivery.received_at {
            facts.push(("Received", format::absolute(received)));
        }
        if let Some(expires) = &delivery.expires_at {
            facts.push(format::expiry(expires));
        }

        let panel = cx.entity().downgrade();
        let parts = (delivery.on_server).then(|| {
            let listing = if delivery.parts.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .py(px(4.))
                    .child(feedback::spinner(p.text_muted, 14.))
                    .child(text::small("Listing what is inside…").text_color(p.text_muted))
            } else {
                div()
                    .flex()
                    .flex_col()
                    .children(delivery.parts.iter().enumerate().map(|(index, part)| {
                        let checked = !self.excluded.contains(part);
                        let part = part.clone();
                        let name = format!("Item {}", index + 1);
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .py(px(6.))
                            .child(
                                field::check(
                                    SharedString::from(format!("inbox-part-{index}")),
                                    name.clone(),
                                    checked,
                                )
                                .on_change({
                                    let panel = panel.clone();
                                    move |included, _, cx| {
                                        let _ = panel.update(cx, |this, cx| {
                                            this.toggle_part(part.clone(), included, cx)
                                        });
                                    }
                                }),
                            )
                            .child(text::small_strong(name).text_color(p.text))
                    }))
            };
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(text::eyebrow("Contents").text_color(p.text_subtle))
                .child(listing)
                .child(
                    text::caption("Names stay encrypted until the delivery is downloaded.")
                        .text_color(p.text_subtle),
                )
        });

        let save_id = delivery.id.clone();
        let remove_id = delivery.id.clone();
        let delete_id = delivery.id.clone();
        let download = delivery.clone();
        div()
            .id("inbox-detail-scroll")
            .size_full()
            .overflow_y_scrollbar()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .entering(entrance)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .child(row::icon_tile(p, "archive", 44., true))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(6.))
                            .child(text::subtitle(delivery.title()).text_color(p.text))
                            .children(
                                delivery
                                    .local_status()
                                    .map(|(label, tone)| feedback::pill(p, label, tone)),
                            ),
                    ),
            )
            .when(delivery.staged(), |this| {
                this.child(feedback::notice(
                    p,
                    "This device already fetched an encrypted copy. Saving verifies and \
                     decrypts it.",
                    Tone::Accent,
                ))
            })
            .children(parts)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(text::eyebrow("Details").text_color(p.text_subtle))
                    .children(
                        facts
                            .into_iter()
                            .map(|(name, value)| row::fact(p, name, value)),
                    )
                    .child(row::fact(
                        p,
                        "Delivery ID",
                        format::abbreviated(&delivery.id, 8, 4),
                    )),
            )
            .child(
                div()
                    .mt_auto()
                    .pt(px(4.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .when(delivery.staged(), |this| {
                        this.child(
                            button::primary("inbox-save-copy")
                                .fill()
                                .icon("folder")
                                .label("Save to folder")
                                .loading(saving)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.save_copy(save_id.clone(), window, cx)
                                })),
                        )
                    })
                    .when(delivery.on_server && !delivery.staged(), |this| {
                        this.child(
                            button::primary("receive-inbox-delivery")
                                .fill()
                                .icon("download")
                                .label(if partial {
                                    format!("Download {}", format::plural(included as u64, "item"))
                                } else {
                                    "Download".to_owned()
                                })
                                .disabled(!delivery.parts.is_empty() && included == 0)
                                .loading(downloading)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.download(&download, cx)),
                                ),
                        )
                    })
                    .child(surface::hairline(p))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(2.))
                            .when(delivery.local.is_some(), |this| {
                                this.child(
                                    button::danger("inbox-remove-copy")
                                        .small()
                                        .icon("x")
                                        .label("Remove the copy on this device")
                                        .loading(removing)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.confirm_remove_copy(remove_id.clone(), window, cx)
                                        })),
                                )
                            })
                            .when(delivery.on_server, |this| {
                                this.child(
                                    button::danger("delete-inbox-item")
                                        .small()
                                        .icon("trash")
                                        .label("Delete from inbox")
                                        .loading(deleting)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.confirm_delete(delete_id.clone(), window, cx)
                                        })),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(feature = "visual-test")]
impl InboxPanel {
    /// Puts the page in the state a capture scenario names, with nothing pending.
    pub fn prepare_for_capture(&mut self, scenario: &str, cx: &mut Context<Self>) {
        self.requests.clear();
        let first = self
            .snapshot(cx)
            .inbox
            .first()
            .map(|delivery| delivery.id.clone());
        match scenario {
            "inbox-selected" => self.selected = first,
            "inbox-download-started" => {
                self.selected = first;
                self.flash = Some(Flash::Started);
            }
            "inbox-locked-refused" => {
                self.unlock_error =
                    Some("That password did not unlock your key. Try again.".into());
            }
            _ => {}
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use filebeam_client_core::services::inbox_receiver::StagedInbox;
    use gpui::{TestAppContext, WindowHandle, test::TestWindowExt as _};
    use gpui_component::Root;

    use super::*;
    use crate::{
        model::{ClientError, ClientErrorCode, ClientMessage, InboxItem, OperationStatus},
        testing::{self, Commands},
    };

    fn account(key_custody: KeyCustody) -> AccountSnapshot {
        AccountSnapshot {
            authenticated: true,
            username: Some("ada".into()),
            profile_url: Some("https://files.example/@ada".into()),
            inbox_enabled: true,
            key_custody,
            ..AccountSnapshot::default()
        }
    }

    fn configured() -> KeyCustody {
        KeyCustody::Configured {
            version: 1,
            fingerprint: "ABCD".into(),
            custody_mode: "self".into(),
        }
    }

    fn item(id: &str, parts: &[&str]) -> InboxItem {
        InboxItem {
            id: id.into(),
            ciphertext_bytes: 2048,
            item_count: 2,
            completed_at: "2026-09-18T10:24:00Z".into(),
            expires_at: "2026-09-25T10:24:00Z".into(),
            item_ids: parts.iter().map(|part| (*part).to_owned()).collect(),
        }
    }

    fn staged(id: &str, state: &str) -> StagedInbox {
        StagedInbox {
            id: id.into(),
            bytes: 4096,
            state: state.into(),
            key_bundle_id: 1,
        }
    }

    fn panel(
        cx: &mut TestAppContext,
        snapshot: DesktopSnapshot,
    ) -> (WindowHandle<Root>, Entity<InboxPanel>, Commands) {
        testing::rooted(cx, snapshot, InboxPanel::new)
    }

    #[test]
    fn the_gate_names_the_first_thing_in_the_way() {
        assert_eq!(gate(&AccountSnapshot::default()), Gate::SignedOut);
        let mut disabled = account(configured());
        disabled.inbox_enabled = false;
        assert_eq!(gate(&disabled), Gate::Disabled);
        assert_eq!(gate(&account(KeyCustody::Unknown)), Gate::NoKey);
        assert_eq!(
            gate(&account(KeyCustody::PendingBackup {
                generation_id: "g".into(),
                fingerprint: "f".into(),
            })),
            Gate::BackupPending
        );
        assert_eq!(
            gate(&account(KeyCustody::Locked {
                version: 1,
                fingerprint: "f".into(),
                custody_mode: "password".into(),
            })),
            Gate::Locked
        );
        assert_eq!(gate(&account(configured())), Gate::Open);
    }

    #[test]
    fn fetched_copies_join_their_delivery_or_keep_a_row_of_their_own() {
        let snapshot = DesktopSnapshot {
            inbox: vec![item("a", &[]), item("b", &["one"])],
            staged_inbox: vec![
                staged("a", STAGED),
                staged("b", "dismissed"),
                staged("gone", STAGED),
                staged("gone-too", "unavailable"),
            ],
            ..DesktopSnapshot::default()
        };
        let rows = deliveries(&snapshot);
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "gone"]
        );
        assert!(rows[0].staged() && rows[0].on_server);
        assert_eq!(rows[1].local, None);
        assert!(rows[2].staged() && !rows[2].on_server);
        assert_eq!(rows[2].title(), "Encrypted delivery");
        assert_eq!(rows[0].title(), "2 encrypted items");
    }

    #[gpui::test]
    fn unlocking_sends_the_password_once_and_explains_a_refusal(cx: &mut TestAppContext) {
        let mut snapshot = DesktopSnapshot {
            account: account(KeyCustody::Locked {
                version: 1,
                fingerprint: "f".into(),
                custody_mode: "password".into(),
            }),
            ..DesktopSnapshot::default()
        };
        let (window, panel, commands) = panel(cx, snapshot.clone());
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("inbox-unlock", cx);
        });
        assert!(
            commands.try_recv().is_err(),
            "an empty password is not sent"
        );
        panel.read_with(cx, |panel, _| assert!(panel.unlock_error.is_some()));

        testing::in_window(cx, window, |window, cx| {
            let input = panel.read(cx).key_password.clone();
            input.update(cx, |input, cx| input.set_value("hunter2", window, cx));
            panel.update(cx, |panel, cx| {
                panel.unlock(window, cx);
                panel.unlock(window, cx);
            });
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::UnlockReceivingKey { password }) if password == "hunter2"
        ));
        assert!(
            commands.try_recv().is_err(),
            "a pending unlock is not repeated"
        );

        snapshot.messages.push(ClientMessage {
            id: 1,
            operation: "unlock receiving key".into(),
            error: ClientError {
                code: ClientErrorCode::Crypto,
                detail: "operation failed; sensitive detail was withheld".into(),
            },
        });
        testing::in_window(cx, window, |window, cx| {
            panel.update(cx, |panel, cx| {
                panel.on_snapshot(&snapshot, window, cx);
                assert!(!panel.waiting("unlock receiving key"));
                assert_eq!(
                    panel.unlock_error.as_deref(),
                    Some("That password did not unlock your key. Try again.")
                );
            });
        });
    }

    #[gpui::test]
    fn a_selection_lists_parts_once_and_downloads_what_is_chosen(cx: &mut TestAppContext) {
        let mut snapshot = DesktopSnapshot {
            account: account(configured()),
            inbox: vec![item("a", &[]), item("b", &["one", "two"])],
            ..DesktopSnapshot::default()
        };
        let (window, panel, commands) = panel(cx, snapshot.clone());
        panel.update(cx, |panel, cx| panel.select("a".into(), cx));
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::InspectInbox { id }) if id == "a"
        ));
        panel.update(cx, |panel, cx| {
            panel.select("b".into(), cx);
            panel.select("missing".into(), cx);
            assert_eq!(panel.selected.as_deref(), Some("b"));
        });
        assert!(
            commands.try_recv().is_err(),
            "known parts are not listed again"
        );

        // Everything is included until a part is left out.
        let delivery = deliveries(&snapshot).remove(1);
        panel.update(cx, |panel, cx| panel.download(&delivery, cx));
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::ReceiveInbox { id, item_ids: None }) if id == "b"
        ));
        panel.update(cx, |panel, cx| {
            panel.toggle_part("one".into(), false, cx);
            panel.download(&delivery, cx);
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::ReceiveInbox { item_ids: Some(parts), .. }) if parts == ["two"]
        ));

        snapshot.operations.push(OperationStatus {
            id: 1,
            operation: "receive inbox".into(),
        });
        testing::in_window(cx, window, |window, cx| {
            panel.update(cx, |panel, cx| {
                panel.on_snapshot(&snapshot, window, cx);
                assert_eq!(panel.flash, Some(Flash::Started));
            });
            window.render_frame(cx);
            assert!(window.try_find("inbox-view-transfers").is_some());
        });
    }

    #[gpui::test]
    fn deleting_asks_first_and_a_removed_delivery_clears_the_selection(cx: &mut TestAppContext) {
        let mut snapshot = DesktopSnapshot {
            account: account(configured()),
            inbox: vec![item("a", &["one"])],
            ..DesktopSnapshot::default()
        };
        let (window, panel, commands) = panel(cx, snapshot.clone());
        panel.update(cx, |panel, cx| panel.select("a".into(), cx));
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("delete-inbox-item", cx);
        });
        assert!(commands.try_recv().is_err(), "deleting asks first");
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("dialog-confirm", cx);
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::DeleteInboxItem { id }) if id == "a"
        ));

        snapshot.inbox.clear();
        snapshot.operations.push(OperationStatus {
            id: 1,
            operation: "delete inbox item".into(),
        });
        testing::in_window(cx, window, |window, cx| {
            panel.update(cx, |panel, cx| {
                panel.on_snapshot(&snapshot, window, cx);
                assert_eq!(panel.selected, None);
            });
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::RefreshInbox)
        ));
    }

    #[gpui::test]
    fn every_gate_and_the_populated_inbox_render(cx: &mut TestAppContext) {
        for account in [
            AccountSnapshot::default(),
            account(KeyCustody::Unknown),
            account(KeyCustody::Locked {
                version: 1,
                fingerprint: "f".into(),
                custody_mode: "password".into(),
            }),
            account(configured()),
        ] {
            let (window, _panel, _commands) = panel(
                cx,
                DesktopSnapshot {
                    account,
                    ..DesktopSnapshot::default()
                },
            );
            testing::in_window(cx, window, |window, cx| window.render_frame(cx));
        }
        let (window, panel, _commands) = panel(
            cx,
            DesktopSnapshot {
                account: account(configured()),
                inbox: vec![item("a", &["one", "two"])],
                staged_inbox: vec![staged("a", STAGED), staged("gone", STAGED)],
                ..DesktopSnapshot::default()
            },
        );
        for id in ["a", "gone"] {
            panel.update(cx, |panel, cx| panel.select(id.into(), cx));
            testing::in_window(cx, window, |window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("inbox-save-copy").is_some());
                assert!(window.try_find("inbox-remove-copy").is_some());
                assert_eq!(window.try_find("delete-inbox-item").is_some(), id == "a");
            });
        }
    }
}
