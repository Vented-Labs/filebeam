use std::{sync::Arc, time::Duration};

use filebeam_client_core::services::HistoryFilter;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, Task, Window, div, px,
};
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    theme::Theme,
};

use crate::{
    client::DesktopClient,
    model::{ClientCommand, HistorySnapshot},
    theme::palette_for,
};

pub struct HistoryPanel {
    client: Arc<DesktopClient>,
    snapshot: HistorySnapshot,
    kind: usize,
    driver: usize,
    status: usize,
    confirming: Option<String>,
    extending: Option<String>,
    hours: Entity<InputState>,
    pending: bool,
    notice: Option<String>,
    scroll: ScrollHandle,
    _refresh: Task<()>,
}

const KINDS: [&str; 3] = ["", "files", "note"];
const DRIVERS: [&str; 3] = ["", "http", "webrtc"];
const STATES: [&str; 11] = [
    "",
    "pending",
    "available",
    "live",
    "ended",
    "deleting",
    "expired",
    "deleted",
    "abandoned",
    "burned",
    "removed",
];

impl HistoryPanel {
    pub fn new(client: Arc<DesktopClient>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this
                    .update(cx, |panel, cx| {
                        let history = panel.client.snapshot().history;
                        if history.revision != panel.snapshot.revision {
                            panel.pending = false;
                            if history.error.is_none() {
                                panel.confirming = None;
                                panel.extending = None;
                                panel.notice = None;
                            }
                        }
                        panel.snapshot = history;
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        Self {
            snapshot: client.snapshot().history,
            client,
            kind: 0,
            driver: 0,
            status: 0,
            confirming: None,
            extending: None,
            hours: cx.new(|cx| InputState::new(window, cx).placeholder("Total hours")),
            pending: false,
            notice: None,
            scroll: ScrollHandle::new(),
            _refresh: refresh,
        }
    }

    fn submit(&mut self, command: ClientCommand, cx: &mut Context<Self>) {
        self.notice = None;
        match self.client.dispatch(command) {
            Ok(()) => self.pending = true,
            Err(error) => self.notice = Some(error.to_string()),
        }
        cx.notify();
    }

    fn load(&mut self, cursor: Option<String>, cx: &mut Context<Self>) {
        let value = |value: &str| (!value.is_empty()).then(|| value.to_owned());
        self.submit(
            ClientCommand::RefreshHistory {
                filter: HistoryFilter {
                    kind: value(KINDS[self.kind]),
                    driver: value(DRIVERS[self.driver]),
                    status: value(STATES[self.status]),
                },
                cursor,
            },
            cx,
        );
    }
}

impl Render for HistoryPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette_for(Theme::global(cx).mode);
        let mut body = div().flex().flex_col().gap_3().child(div().text_2xl().child("Transfer history"))
            .child(div().text_sm().text_color(p.muted).child("Outgoing encrypted transfers. Removed entries remain for 90 days. Names and share-link keys stay on your device."))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("history-type").label(format!("Type: {}", if self.kind == 0 { "all" } else { KINDS[self.kind] })).disabled(self.pending).on_click(cx.listener(|this, _, _, cx| { this.kind = (this.kind + 1) % KINDS.len(); this.load(None, cx); })))
                .child(Button::new("history-driver").label(format!("Transport: {}", if self.driver == 0 { "all" } else { DRIVERS[self.driver] })).disabled(self.pending).on_click(cx.listener(|this, _, _, cx| { this.driver = (this.driver + 1) % DRIVERS.len(); this.load(None, cx); })))
                .child(Button::new("history-status").label(format!("State: {}", if self.status == 0 { "all" } else { STATES[self.status] })).disabled(self.pending).on_click(cx.listener(|this, _, _, cx| { this.status = (this.status + 1) % STATES.len(); this.load(None, cx); })))
                .child(Button::new("history-refresh").label("Newest / Refresh").disabled(self.pending).on_click(cx.listener(|this, _, _, cx| this.load(None, cx)))));
        if let Some(error) = self.notice.as_ref().or(self.snapshot.error.as_ref()) {
            body = body.child(div().text_color(p.danger).child(error.clone()));
        }
        if self.pending {
            body = body.child(div().child("Updating history…"));
        }
        if self.snapshot.page.data.is_empty() {
            body =
                body.child(div().py_8().child(
                    "No outgoing transfers to display. Sign in to load your account history.",
                ));
        }
        for (index, entry) in self.snapshot.page.data.iter().enumerate() {
            let mut actions = div().flex().flex_wrap().gap_2();
            if self.confirming.as_deref() == Some(&entry.id) {
                let id = entry.id.clone();
                actions = actions
                    .child(div().child("Delete encrypted content for everyone?"))
                    .child(
                        Button::new(("history-confirm", index))
                            .label("Confirm deletion")
                            .danger()
                            .disabled(self.pending)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.submit(ClientCommand::DeleteHistory { id: id.clone() }, cx)
                            })),
                    )
                    .child(
                        Button::new(("history-cancel", index))
                            .label("Cancel")
                            .ghost()
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirming = None;
                                cx.notify();
                            })),
                    );
            } else if self.extending.as_deref() == Some(&entry.id) {
                let id = entry.id.clone();
                let maximum = entry.maximum_retention_hours;
                actions = actions
                    .child(div().w(px(160.)).child(Input::new(&self.hours)))
                    .child(div().text_sm().child(format!(
                        "Total hours, up to {maximum}. Latest expiry: {}",
                        entry.maximum_expires_at.as_deref().unwrap_or("unavailable")
                    )))
                    .child(
                        Button::new(("history-save", index))
                            .label("Save retention")
                            .disabled(self.pending)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                match this.hours.read(cx).value().parse::<u64>() {
                                    Ok(hours) if hours > 0 && hours <= maximum => this.submit(
                                        ClientCommand::ExtendHistory {
                                            id: id.clone(),
                                            retention_hours: hours,
                                        },
                                        cx,
                                    ),
                                    _ => {
                                        this.notice = Some(format!(
                                            "Enter total retention hours between 1 and {maximum}."
                                        ));
                                        cx.notify();
                                    }
                                }
                            })),
                    )
                    .child(
                        Button::new(("history-extend-cancel", index))
                            .label("Cancel")
                            .ghost()
                            .disabled(self.pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.extending = None;
                                cx.notify();
                            })),
                    );
            } else {
                if entry.can_extend {
                    let id = entry.id.clone();
                    let hours = entry
                        .retention_hours
                        .saturating_add(24)
                        .min(entry.maximum_retention_hours);
                    actions = actions.child(
                        Button::new(("history-extend", index))
                            .label("Extend")
                            .disabled(self.pending)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.extending = Some(id.clone());
                                this.hours.update(cx, |input, cx| {
                                    input.set_value(hours.to_string(), window, cx)
                                });
                                cx.notify();
                            })),
                    );
                }
                if entry.can_delete {
                    let id = entry.id.clone();
                    actions = actions.child(
                        Button::new(("history-delete", index))
                            .label("Delete")
                            .danger()
                            .disabled(self.pending)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirming = Some(id.clone());
                                cx.notify();
                            })),
                    );
                }
            }
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(div().child(format!(
                        "{} · {} · {} · {}",
                        entry.id, entry.kind, entry.driver, entry.status
                    )))
                    .child(div().text_sm().text_color(p.muted).child(format!(
                        "{} item(s) · {} encrypted bytes · created {}",
                        entry.item_count,
                        entry.ciphertext_bytes.max(entry.declared_ciphertext_bytes),
                        entry.created_at
                    )))
                    .child(div().text_sm().child(format!(
                        "{} {}",
                        if entry.removed_at.is_some() {
                            "Removed"
                        } else {
                            "Expires"
                        },
                        entry.removed_at.as_deref().unwrap_or(&entry.expires_at)
                    )))
                    .child(actions),
            );
        }
        if let Some(cursor) = self.snapshot.page.next_cursor.clone() {
            body = body.child(
                Button::new("history-older")
                    .label("Older transfers")
                    .disabled(self.pending)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.load(Some(cursor.clone()), cx)),
                    ),
            );
        }
        let _ = window;
        div()
            .id("history-page-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_6()
            .child(body)
    }
}
