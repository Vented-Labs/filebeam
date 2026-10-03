use std::{sync::Arc, time::Duration};

use filebeam_client_core::services::HistoryFilter;
use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, ScrollHandle, Styled, Task,
    Window, div, px,
};
use gpui_component::{
    Disableable, IndexPath, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    select::{Select, SelectEvent, SelectState},
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
    kind_select: Entity<SelectState<Vec<&'static str>>>,
    driver_select: Entity<SelectState<Vec<&'static str>>>,
    status_select: Entity<SelectState<Vec<&'static str>>>,
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
const KIND_LABELS: [&str; 3] = ["Files and notes", "Files", "Notes"];
const DRIVER_LABELS: [&str; 3] = ["All transports", "Stored HTTP", "Live WebRTC"];
const STATUS_LABELS: [&str; 11] = [
    "All states",
    "Uploading",
    "Available",
    "Live",
    "Ended",
    "Awaiting cleanup",
    "Expired",
    "Deleted",
    "Abandoned upload",
    "Burned after reading",
    "Removed",
];
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
        let kind_select = cx.new(|cx| {
            SelectState::new(KIND_LABELS.to_vec(), Some(IndexPath::default()), window, cx)
        });
        let driver_select = cx.new(|cx| {
            SelectState::new(
                DRIVER_LABELS.to_vec(),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        let status_select = cx.new(|cx| {
            SelectState::new(
                STATUS_LABELS.to_vec(),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        cx.subscribe(
            &kind_select,
            |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event
                    && let Some(index) = KIND_LABELS.iter().position(|label| label == value)
                {
                    this.kind = index;
                    this.load(None, cx);
                }
            },
        )
        .detach();
        cx.subscribe(
            &driver_select,
            |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event
                    && let Some(index) = DRIVER_LABELS.iter().position(|label| label == value)
                {
                    this.driver = index;
                    this.load(None, cx);
                }
            },
        )
        .detach();
        cx.subscribe(
            &status_select,
            |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event
                    && let Some(index) = STATUS_LABELS.iter().position(|label| label == value)
                {
                    this.status = index;
                    this.load(None, cx);
                }
            },
        )
        .detach();
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
            kind_select,
            driver_select,
            status_select,
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
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(crate::views::page::HEADER_GAP))
            .child(crate::views::page::heading(
                p,
                "Transfer history",
                "Your outgoing encrypted files and notes, across devices.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .p(px(16.))
                            .flex()
                            .flex_wrap()
                            .items_end()
                            .gap(px(12.))
                            .child(
                                div()
                                    .w(px(180.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div().text_size(px(10.)).text_color(p.muted).child("Type"),
                                    )
                                    .child(
                                        Select::new(&self.kind_select)
                                            .id("history-type")
                                            .disabled(self.pending),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(180.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(p.muted)
                                            .child("Transport"),
                                    )
                                    .child(
                                        Select::new(&self.driver_select)
                                            .id("history-driver")
                                            .disabled(self.pending),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(180.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(p.muted)
                                            .child("Status"),
                                    )
                                    .child(
                                        Select::new(&self.status_select)
                                            .id("history-status")
                                            .disabled(self.pending),
                                    ),
                            )
                            .child(
                                Button::new("history-refresh")
                                    .label("Refresh")
                                    .icon(crate::assets::icon("rotate-right"))
                                    .outline()
                                    .disabled(self.pending)
                                    .on_click(cx.listener(|this, _, _, cx| this.load(None, cx))),
                            ),
                    )
                    .child(
                        div()
                            .px(px(16.))
                            .py(px(10.))
                            .border_t_1()
                            .border_color(p.border_soft)
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(p.subtle)
                            .child(crate::assets::icon("lock").size(px(14.)))
                            .child("Names and share-link keys stay on your device."),
                    ),
            );
        if let Some(error) = self.notice.as_ref().or(self.snapshot.error.as_ref()) {
            body = body.child(div().text_color(p.danger).child(error.clone()));
        }
        if self.pending {
            body = body.child(div().child("Updating history…"));
        }
        if self.snapshot.page.data.is_empty() {
            body =
                body.child(
                    div()
                        .h(px(260.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.surface)
                        .child(
                            div()
                                .size(px(48.))
                                .rounded(px(12.))
                                .bg(p.selected)
                                .text_color(p.accent)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(crate::assets::icon("clock").size(px(24.))),
                        )
                        .child(div().text_size(px(16.)).child("No transfers to show"))
                        .child(div().text_size(px(12.)).text_color(p.muted).child(
                            "Sign in to load your history, or adjust the selected filters.",
                        )),
                );
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
                            .primary()
                            .with_size(Size::Small)
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
                            .icon(crate::assets::icon("clock"))
                            .outline()
                            .with_size(Size::Small)
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
                            .icon(crate::assets::icon("trash"))
                            .ghost()
                            .with_size(Size::Small)
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
                    .gap(px(10.))
                    .p(px(18.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .size(px(36.))
                                    .rounded(px(9.))
                                    .bg(p.selected)
                                    .text_color(p.accent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        crate::assets::icon(if entry.kind == "note" {
                                            "note"
                                        } else {
                                            "folder"
                                        })
                                        .size(px(18.)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_size(px(14.)).child(
                                        if entry.kind == "note" {
                                            "Encrypted note".into()
                                        } else {
                                            format!("{} encrypted file(s)", entry.item_count)
                                        },
                                    ))
                                    .child(
                                        div()
                                            .mt(px(3.))
                                            .text_size(px(10.))
                                            .text_color(p.subtle)
                                            .child(entry.id.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(8.))
                                    .py(px(4.))
                                    .rounded(px(12.))
                                    .bg(p.raised)
                                    .text_size(px(10.))
                                    .text_color(
                                        if matches!(entry.status.as_str(), "available" | "live") {
                                            p.success
                                        } else if matches!(
                                            entry.status.as_str(),
                                            "pending" | "deleting"
                                        ) {
                                            p.warning
                                        } else {
                                            p.muted
                                        },
                                    )
                                    .child(status_label(&entry.status)),
                            ),
                    )
                    .child(
                        div()
                            .pl(px(48.))
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(format!(
                                "{} encrypted · {} · {}",
                                format_bytes(
                                    entry.ciphertext_bytes.max(entry.declared_ciphertext_bytes)
                                ),
                                if entry.driver == "webrtc" {
                                    "Live WebRTC"
                                } else {
                                    "Stored HTTP"
                                },
                                if entry.delivery == "inbox" {
                                    "Inbox delivery"
                                } else {
                                    "Link share"
                                }
                            )),
                    )
                    .child(
                        div()
                            .pl(px(48.))
                            .text_size(px(11.))
                            .text_color(p.subtle)
                            .child(format!("Created {}", entry.created_at)),
                    )
                    .child(
                        div()
                            .pl(px(48.))
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(format!(
                                "{} {}",
                                if entry.removed_at.is_some() {
                                    "Removed"
                                } else {
                                    "Expires"
                                },
                                entry.removed_at.as_deref().unwrap_or(&entry.expires_at)
                            )),
                    )
                    .child(
                        div()
                            .pt(px(10.))
                            .mt(px(4.))
                            .border_t_1()
                            .border_color(p.border_soft)
                            .child(actions),
                    ),
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
        crate::views::page::scroller("history-page-scroll", &self.scroll, window, body)
    }
}

fn status_label(status: &str) -> String {
    match status {
        "pending" => "Uploading".into(),
        "deleting" => "Awaiting cleanup".into(),
        "burned" => "Burned after reading".into(),
        "abandoned" => "Abandoned upload".into(),
        value => {
            let mut chars = value.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024. * 1024.))
    } else {
        format!("{:.1} GiB", bytes as f64 / (1024. * 1024. * 1024.))
    }
}
