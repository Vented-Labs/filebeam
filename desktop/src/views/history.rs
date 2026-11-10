//! Everything this account has shared, from any device. The server only knows
//! sizes, times and states, so nothing here can name a file or open a link.

use std::{sync::Arc, time::Duration};

use filebeam_client_core::services::{HistoryEntry, HistoryFilter};
use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Window,
    div, prelude::FluentBuilder, px,
};
use gpui_component::{
    IndexPath,
    scroll::ScrollableElement,
    select::{SelectEvent, SelectState},
};

use crate::{
    assets::icon,
    client::DesktopClient,
    live::Live,
    model::{ClientCommand, DesktopSnapshot},
    request::{Outcome, Request},
    shell::OpenAccount,
    theme::{self, Palette, tokens::geometry},
    ui::{
        Hugged, MotionExt, TextExt, Tone, button, dialog, feedback, field, format, motion, page,
        rail::{Segment, rail},
        row, surface, text,
    },
};

const DETAIL_WIDTH: f32 = 372.;
const DETAIL_WIDTH_COMPACT: f32 = 316.;

const KINDS: [(&str, &str); 3] = [("", "All"), ("files", "Files"), ("note", "Notes")];
const METHODS: [(&str, &str); 3] = [("", "Any method"), ("http", "Server"), ("webrtc", "P2P")];
const STATUSES: [(&str, &str); 11] = [
    ("", "Any status"),
    ("pending", "Uploading"),
    ("available", "Available"),
    ("live", "Live"),
    ("ended", "Ended"),
    ("deleting", "Awaiting cleanup"),
    ("expired", "Expired"),
    ("deleted", "Deleted"),
    ("abandoned", "Abandoned upload"),
    ("burned", "Burned after reading"),
    ("removed", "Removed"),
];

fn status_label(status: &str) -> String {
    STATUSES
        .iter()
        .find(|(value, _)| !value.is_empty() && *value == status)
        .map_or_else(
            || {
                let mut characters = status.chars();
                characters.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + characters.as_str()
                })
            },
            |(_, label)| (*label).to_owned(),
        )
}

fn status_tone(status: &str) -> Tone {
    match status {
        "available" | "live" => Tone::Success,
        "pending" => Tone::Accent,
        "deleting" | "abandoned" => Tone::Warning,
        _ => Tone::Neutral,
    }
}

fn title(entry: &HistoryEntry) -> String {
    if entry.kind == "note" {
        "Encrypted note".into()
    } else {
        format::plural(entry.item_count, "encrypted file")
    }
}

fn method(entry: &HistoryEntry) -> &'static str {
    if entry.driver == "webrtc" {
        "P2P"
    } else {
        "Server"
    }
}

fn delivery(entry: &HistoryEntry) -> &'static str {
    if entry.delivery == "inbox" {
        "Sent to an inbox"
    } else {
        "Shared by link"
    }
}

fn size(entry: &HistoryEntry) -> String {
    format::bytes(entry.ciphertext_bytes.max(entry.declared_ciphertext_bytes))
}

/// Totals a transfer can be kept for, shortest first, ending at the longest allowed.
fn extension_choices(current: u64, maximum: u64) -> Vec<u64> {
    let mut choices: Vec<u64> = [24, 72, 168]
        .into_iter()
        .map(|more| current.saturating_add(more))
        .filter(|total| *total < maximum)
        .collect();
    if maximum > current {
        choices.push(maximum);
    }
    choices
}

/// What the page is waiting for from the service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loading {
    Page,
    Older,
}

/// Retained history filters, the pages loaded so far and the selected entry.
pub struct HistoryPanel {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    kind: usize,
    method: usize,
    status: usize,
    status_select: Entity<SelectState<Vec<&'static str>>>,
    entries: Vec<HistoryEntry>,
    next_cursor: Option<String>,
    revision: u64,
    loading: Option<Loading>,
    requests: Vec<Request>,
    error: Option<String>,
    selected: Option<String>,
    /// The total retention chosen for the selected entry, in hours.
    extend_to: Option<u64>,
    copied: bool,
    list_scroll: ScrollHandle,
}

impl HistoryPanel {
    pub fn new(live: Entity<Live>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = live.read(cx).client().clone();
        let status_select = cx.new(|cx| {
            SelectState::new(
                STATUSES.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
                Some(IndexPath::default()),
                window,
                cx,
            )
        });
        cx.subscribe(
            &status_select,
            |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event
                    && let Some(index) = STATUSES.iter().position(|(_, label)| label == value)
                    && index != this.status
                {
                    this.status = index;
                    this.load(None, cx);
                }
            },
        )
        .detach();
        cx.observe(&live, |this, live, cx| {
            let snapshot = live.read(cx).snapshot();
            this.on_snapshot(&snapshot, cx);
        })
        .detach();
        let history = live.read(cx).snapshot().history.clone();
        Self {
            client,
            live,
            kind: 0,
            method: 0,
            status: 0,
            status_select,
            entries: history.page.data,
            next_cursor: history.page.next_cursor,
            revision: history.revision,
            loading: None,
            requests: Vec::new(),
            error: history.error.as_deref().map(format::readable_error),
            selected: None,
            extend_to: None,
            copied: false,
            list_scroll: ScrollHandle::new(),
        }
    }

    /// Loads the first page again. The shell calls this when the page opens.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.live.read(cx).snapshot().account.authenticated && self.loading.is_none() {
            self.load(None, cx);
        }
    }

    fn waiting(&self, operation: &str) -> bool {
        self.requests
            .iter()
            .any(|request| request.operation() == operation)
    }

    fn filter(&self) -> HistoryFilter {
        let value = |value: &str| (!value.is_empty()).then(|| value.to_owned());
        HistoryFilter {
            kind: value(KINDS[self.kind].0),
            driver: value(METHODS[self.method].0),
            status: value(STATUSES[self.status].0),
        }
    }

    fn filtering(&self) -> bool {
        self.kind != 0 || self.method != 0 || self.status != 0
    }

    fn load(&mut self, cursor: Option<String>, cx: &mut Context<Self>) {
        let loading = if cursor.is_some() {
            Loading::Older
        } else {
            Loading::Page
        };
        match Request::send(
            &self.client,
            ClientCommand::RefreshHistory {
                filter: self.filter(),
                cursor,
            },
        ) {
            Ok(request) => {
                self.requests
                    .retain(|pending| pending.operation() != request.operation());
                self.requests.push(request);
                self.loading = Some(loading);
                self.error = None;
            }
            Err(error) => self.error = Some(format::readable_error(&error.to_string())),
        }
        cx.notify();
    }

    fn send(&mut self, command: ClientCommand, cx: &mut Context<Self>) {
        match Request::send(&self.client, command) {
            Ok(request) => {
                self.requests.push(request);
                self.error = None;
            }
            Err(error) => self.error = Some(format::readable_error(&error.to_string())),
        }
        cx.notify();
    }

    /// Takes a newly published page, appending it when older entries were asked for.
    fn on_snapshot(&mut self, snapshot: &DesktopSnapshot, cx: &mut Context<Self>) {
        let mut failure = None;
        self.requests
            .retain(|request| match request.outcome(snapshot) {
                Outcome::Waiting => true,
                Outcome::Completed => false,
                Outcome::Failed(detail) => {
                    failure = Some(format::readable_error(&detail));
                    false
                }
            });
        if let Some(failure) = failure {
            self.error = Some(failure);
        }
        if !self.waiting("load history") {
            // A refused load publishes nothing new, so it must not keep spinning.
            if snapshot.history.revision == self.revision {
                self.loading = None;
            }
        }

        let history = &snapshot.history;
        if history.revision != self.revision {
            self.revision = history.revision;
            let older = self.loading.take() == Some(Loading::Older);
            match &history.error {
                // What is already listed stays put when a page fails to load.
                Some(error) => self.error = Some(format::readable_error(error)),
                None if older => {
                    for entry in &history.page.data {
                        if !self.entries.iter().any(|known| known.id == entry.id) {
                            self.entries.push(entry.clone());
                        }
                    }
                    self.next_cursor = history.page.next_cursor.clone();
                }
                None => {
                    self.entries = history.page.data.clone();
                    self.next_cursor = history.page.next_cursor.clone();
                }
            }
        }
        if !snapshot.account.authenticated {
            self.entries.clear();
            self.next_cursor = None;
            self.error = None;
            self.loading = None;
        }
        if let Some(id) = &self.selected
            && !self.entries.iter().any(|entry| &entry.id == id)
        {
            self.selected = None;
            self.extend_to = None;
        }
        cx.notify();
    }

    fn select(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() != Some(id.as_str())
            && self.entries.iter().any(|entry| entry.id == id)
        {
            self.selected = Some(id);
            self.extend_to = None;
            self.copied = false;
            cx.notify();
        }
    }

    fn extend(&mut self, entry: &HistoryEntry, cx: &mut Context<Self>) {
        let Some(hours) = self.extend_to else {
            return;
        };
        if hours > entry.retention_hours && hours <= entry.maximum_retention_hours {
            self.send(
                ClientCommand::ExtendHistory {
                    id: entry.id.clone(),
                    retention_hours: hours,
                },
                cx,
            );
            self.extend_to = None;
        }
    }

    fn confirm_delete(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        dialog::prompt("Delete this for everyone?")
            .body(
                "The link stops working and the encrypted content is removed from the server. \
                 This cannot be undone.",
            )
            .confirm("Delete for everyone")
            .destructive()
            .open(window, cx, move |_, cx| {
                entity.update(cx, |this, cx| {
                    this.send(ClientCommand::DeleteHistory { id: id.clone() }, cx);
                });
            });
    }

    fn copy_id(&mut self, id: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(id));
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

impl Render for HistoryPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.live.read(cx).snapshot();
        let signed_in = snapshot.account.authenticated;
        let compact = page::is_compact(window);
        let reloading = self.loading == Some(Loading::Page);
        let refresh = signed_in.then(|| {
            button::secondary("history-refresh")
                .icon("updates")
                .label("Refresh")
                .loading(reloading)
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
        });
        let header = page::header_with(
            p,
            "History",
            "Everything you have shared from this account, on any device.",
            div().children(refresh),
            window,
            cx,
        );
        let entrance = motion::entrance(
            if signed_in {
                "history-body"
            } else {
                "history-signed-out"
            },
            1,
            window,
            cx,
        );

        let body: AnyElement = if signed_in {
            let filters = self.filters(p, compact, cx);
            let list = self.list(p, cx);
            let selected = self
                .selected
                .as_ref()
                .and_then(|id| self.entries.iter().find(|entry| &entry.id == id))
                .cloned();
            let detail = self.detail(selected, p, window, cx);
            div()
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(filters)
                .when_some(self.error.clone(), |this, error| {
                    this.child(
                        feedback::notice(p, error, Tone::Danger)
                            .items_center()
                            .child(
                                button::quiet("history-retry")
                                    .small()
                                    .label("Try again")
                                    .on_click(cx.listener(|this, _, _, cx| this.load(None, cx))),
                            ),
                    )
                })
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
        } else {
            let action = button::primary("history-sign-in")
                .label("Sign in")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenAccount), cx))
                .into_any_element();
            surface::panel(p)
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    feedback::empty_state(
                        "clock",
                        "Your sharing history",
                        "Sign in to see what you have shared, keep it longer or delete it for \
                     everyone.",
                    )
                    .action(Some(action)),
                )
                .into_any_element()
        };

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

impl HistoryPanel {
    fn filters(
        &mut self,
        p: Palette,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let kinds = cx.entity().downgrade();
        let methods = cx.entity().downgrade();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .child(
                rail("history-type", "Type", self.kind)
                    .segment_width(if compact { 68. } else { 76. })
                    .segments(KINDS.iter().map(|(_, label)| Segment::new(*label)))
                    .on_select(move |index, _, cx| {
                        let _ = kinds.update(cx, |this, cx| {
                            if this.kind != index {
                                this.kind = index;
                                this.load(None, cx);
                            }
                        });
                    }),
            )
            .child(
                rail("history-driver", "Method", self.method)
                    .segment_width(if compact { 92. } else { 104. })
                    .segments(METHODS.iter().map(|(_, label)| Segment::new(*label)))
                    .on_select(move |index, _, cx| {
                        let _ = methods.update(cx, |this, cx| {
                            if this.method != index {
                                this.method = index;
                                this.load(None, cx);
                            }
                        });
                    }),
            )
            .child(
                div()
                    .w(px(210.))
                    .flex_none()
                    .child(field::select(p, &self.status_select).id("history-status")),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("lock").size(px(13.)).text_color(p.text_subtle))
                    .child(
                        text::caption("Names and link keys stay on your devices.")
                            .text_color(p.text_subtle),
                    ),
            )
    }

    fn list(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.entries.is_empty() {
            if self.loading.is_some() {
                return div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(10.))
                    .child(feedback::spinner(p.text_muted, 16.))
                    .child(text::small("Loading your history…").text_color(p.text_muted))
                    .into_any_element();
            }
            let filtering = self.filtering();
            let (title, body) = if self.error.is_some() {
                (
                    "History is unavailable",
                    "It could not be loaded just now. Nothing has been lost.",
                )
            } else if filtering {
                (
                    "Nothing matches",
                    "No transfers match these filters. Try widening them.",
                )
            } else {
                (
                    "Nothing shared yet",
                    "Files and notes you share while signed in are listed here.",
                )
            };
            let reset = (filtering && self.error.is_none()).then(|| {
                button::secondary("history-clear-filters")
                    .label("Show everything")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.kind = 0;
                        this.method = 0;
                        this.status = 0;
                        this.status_select.update(cx, |select, cx| {
                            select.set_selected_index(Some(IndexPath::default()), window, cx);
                        });
                        this.load(None, cx);
                    }))
                    .into_any_element()
            });
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(feedback::empty_state("clock", title, body).action(reset))
                .into_any_element();
        }
        let loading_older = self.loading == Some(Loading::Older);
        let older = self.next_cursor.clone().map(|cursor| {
            div().flex().justify_center().py(px(10.)).child(
                button::secondary("history-older")
                    .small()
                    .label("Load older")
                    .loading(loading_older)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.loading.is_none() {
                            this.load(Some(cursor.clone()), cx);
                        }
                    })),
            )
        });
        let rows: Vec<_> = self
            .entries
            .clone()
            .into_iter()
            .map(|entry| self.row(entry, p, cx))
            .collect();
        div()
            .id("history-page-scroll")
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .hugged()
            .flex()
            .flex_col()
            .children(rows)
            .children(older)
            .into_any_element()
    }

    fn row(
        &self,
        entry: HistoryEntry,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.selected.as_deref() == Some(entry.id.as_str());
        let id = entry.id.clone();
        let live = matches!(entry.status.as_str(), "available" | "live" | "pending");
        row::slot()
            .id(SharedString::from(format!("history-entry-{}", entry.id)))
            .child(
                row::hugging(p, row::Fit::PANEL, selected)
                    .child(row::lead_tile(
                        p,
                        row::Fit::PANEL,
                        if entry.kind == "note" {
                            "note"
                        } else {
                            "folder"
                        },
                        live,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                text::body_strong(title(&entry))
                                    .single_line()
                                    .text_color(p.text),
                            )
                            .child(
                                text::caption(format!(
                                    "{} · {} · {}",
                                    size(&entry),
                                    method(&entry),
                                    format::relative(&entry.created_at)
                                ))
                                .single_line()
                                .text_color(p.text_muted),
                            ),
                    )
                    .child(feedback::pill(
                        p,
                        status_label(&entry.status),
                        status_tone(&entry.status),
                    )),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
    }

    fn detail(
        &mut self,
        entry: Option<HistoryEntry>,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    feedback::empty_state(
                        "info",
                        "Select a transfer",
                        "Keep it longer, or delete it for everyone.",
                    )
                    .tone(Tone::Neutral),
                )
                .into_any_element();
        };
        let entrance = motion::entrance_with(
            SharedString::from(format!("history-detail-{}", entry.id)),
            0,
            8.,
            motion::enter_quick(),
            window,
            cx,
        );
        let extending = self.waiting("extend history transfer");
        let deleting = self.waiting("delete history transfer");

        let mut facts = vec![
            ("Size", size(&entry)),
            ("Method", method(&entry).to_owned()),
            ("Delivery", delivery(&entry).to_owned()),
            ("Created", format::absolute(&entry.created_at)),
        ];
        match &entry.removed_at {
            Some(removed) => facts.push(("Removed", format::absolute(removed))),
            None => {
                facts.push(format::expiry(&entry.expires_at));
                facts.push(("Kept for", format::retention(entry.retention_hours)));
            }
        }
        if entry.burn_on_read {
            facts.push(("After reading", "Burned".to_owned()));
        }

        let choices = extension_choices(entry.retention_hours, entry.maximum_retention_hours);
        let extension = (entry.can_extend && !choices.is_empty()).then(|| {
            let longest = choices.last().copied();
            let chosen = self.extend_to;
            let target = entry.clone();
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(text::eyebrow("Keep it longer").text_color(p.text_subtle))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(choices.into_iter().map(|hours| {
                            let selected = chosen == Some(hours);
                            let id = SharedString::from(format!("history-extend-{hours}"));
                            if selected {
                                button::tonal(id)
                            } else {
                                button::secondary(id)
                            }
                            .small()
                            .label(if Some(hours) == longest {
                                format!("{} (longest)", format::retention(hours))
                            } else {
                                format::retention(hours)
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.extend_to = (!selected).then_some(hours);
                                    cx.notify();
                                },
                            ))
                        })),
                )
                .child(
                    button::primary("history-save")
                        .fill()
                        .icon("clock")
                        .label(match chosen {
                            Some(hours) => format!("Keep for {}", format::retention(hours)),
                            None => "Choose how long".to_owned(),
                        })
                        .disabled(chosen.is_none())
                        .loading(extending)
                        .on_click(cx.listener(move |this, _, _, cx| this.extend(&target, cx))),
                )
                .child(
                    text::caption(format!(
                        "Counted from when it was shared. This instance allows up to {}.",
                        format::retention(entry.maximum_retention_hours)
                    ))
                    .text_color(p.text_subtle),
                )
        });

        let copy_id = entry.id.clone();
        let delete_id = entry.id.clone();
        div()
            .id("history-detail-scroll")
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
                    .child(row::icon_tile(
                        p,
                        if entry.kind == "note" {
                            "note"
                        } else {
                            "folder"
                        },
                        44.,
                        true,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(6.))
                            .child(text::subtitle(title(&entry)).text_color(p.text))
                            .child(feedback::pill(
                                p,
                                status_label(&entry.status),
                                status_tone(&entry.status),
                            )),
                    ),
            )
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .child(
                                text::small("Transfer ID")
                                    .flex_none()
                                    .text_color(p.text_muted),
                            )
                            .child(
                                button::ghost("history-copy-id")
                                    .small()
                                    .icon(if self.copied { "check" } else { "copy" })
                                    .label(format::abbreviated(&entry.id, 8, 4))
                                    .tooltip("Copy the transfer ID")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.copy_id(copy_id.clone(), cx)
                                    })),
                            ),
                    ),
            )
            .children(extension)
            .when(entry.can_delete, |this| {
                this.child(
                    div()
                        .mt_auto()
                        .pt(px(4.))
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(8.))
                        .child(surface::hairline(p))
                        .child(
                            button::danger("history-delete")
                                .small()
                                .icon("trash")
                                .label("Delete for everyone")
                                .loading(deleting)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.confirm_delete(delete_id.clone(), window, cx)
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(feature = "visual-test")]
impl HistoryPanel {
    /// Puts the page in the state a capture scenario names, with nothing pending.
    pub fn prepare_for_capture(&mut self, scenario: &str, cx: &mut Context<Self>) {
        self.requests.clear();
        self.loading = (scenario == "history-loading").then_some(Loading::Page);
        self.error = self
            .live
            .read(cx)
            .snapshot()
            .history
            .error
            .as_deref()
            .map(format::readable_error);
        if scenario == "history-selected" {
            self.selected = self.entries.first().map(|entry| entry.id.clone());
            self.extend_to = Some(96);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use filebeam_client_core::services::HistoryPage;
    use gpui::{TestAppContext, WindowHandle, test::TestWindowExt as _};
    use gpui_component::Root;

    use super::*;
    use crate::{
        model::{ClientError, ClientErrorCode, ClientMessage, HistorySnapshot, OperationStatus},
        testing::{self, Commands},
    };

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            kind: "files".into(),
            driver: "http".into(),
            delivery: "link".into(),
            status: "available".into(),
            item_count: 2,
            ciphertext_bytes: 2048,
            declared_ciphertext_bytes: 4096,
            created_at: "2026-09-18T10:24:00Z".into(),
            completed_at: Some("2026-09-18T10:25:00Z".into()),
            published_at: None,
            expires_at: "2026-09-19T10:25:00Z".into(),
            removed_at: None,
            retention_hours: 24,
            maximum_retention_hours: 168,
            maximum_expires_at: Some("2026-09-25T10:25:00Z".into()),
            burn_on_read: false,
            can_delete: true,
            can_extend: true,
        }
    }

    fn signed_in(entries: Vec<HistoryEntry>, next_cursor: Option<&str>) -> DesktopSnapshot {
        let mut snapshot = DesktopSnapshot::default();
        snapshot.account.authenticated = true;
        snapshot.history = HistorySnapshot {
            page: HistoryPage {
                data: entries,
                next_cursor: next_cursor.map(str::to_owned),
            },
            revision: 1,
            ..HistorySnapshot::default()
        };
        snapshot
    }

    fn panel(
        cx: &mut TestAppContext,
        snapshot: DesktopSnapshot,
    ) -> (WindowHandle<Root>, Entity<HistoryPanel>, Commands) {
        testing::rooted(cx, snapshot, HistoryPanel::new)
    }

    #[test]
    fn labels_and_extension_choices_follow_the_entry() {
        assert_eq!(status_label("pending"), "Uploading");
        assert_eq!(status_label("deleting"), "Awaiting cleanup");
        assert_eq!(status_label("unheard"), "Unheard");
        assert_eq!(status_tone("live"), Tone::Success);
        assert_eq!(status_tone("expired"), Tone::Neutral);

        let mut note = entry("n");
        note.kind = "note".into();
        note.driver = "webrtc".into();
        note.delivery = "inbox".into();
        assert_eq!(title(&note), "Encrypted note");
        assert_eq!(method(&note), "P2P");
        assert_eq!(delivery(&note), "Sent to an inbox");
        assert_eq!(title(&entry("f")), "2 encrypted files");
        assert_eq!(size(&entry("f")), "4.0 KiB");

        assert_eq!(extension_choices(24, 168), [48, 96, 168]);
        assert_eq!(extension_choices(24, 48), [48]);
        assert_eq!(extension_choices(168, 168), Vec::<u64>::new());
        assert_eq!(extension_choices(1, 10_000), [25, 73, 169, 10_000]);
    }

    #[gpui::test]
    fn filters_reload_and_older_pages_are_appended(cx: &mut TestAppContext) {
        let mut snapshot = signed_in(vec![entry("a"), entry("b")], Some("next"));
        let (_window, panel, commands) = panel(cx, snapshot.clone());
        panel.update(cx, |panel, cx| {
            assert_eq!(panel.entries.len(), 2);
            panel.kind = 2;
            panel.method = 1;
            panel.load(None, cx);
            assert_eq!(panel.loading, Some(Loading::Page));
        });
        let Ok(ClientCommand::RefreshHistory { filter, cursor }) = commands.try_recv() else {
            panic!("the filtered page is requested")
        };
        assert_eq!(filter.kind.as_deref(), Some("note"));
        assert_eq!(filter.driver.as_deref(), Some("http"));
        assert_eq!(filter.status, None);
        assert_eq!(cursor, None);

        // The first page replaces what was listed.
        snapshot.history.revision = 2;
        snapshot.history.page.data = vec![entry("c")];
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, cx);
            assert_eq!(panel.loading, None);
            assert_eq!(panel.entries.len(), 1);
            panel.load(panel.next_cursor.clone(), cx);
            assert_eq!(panel.loading, Some(Loading::Older));
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::RefreshHistory { cursor: Some(cursor), .. }) if cursor == "next"
        ));

        // An older page joins it, without repeating an entry.
        snapshot.history.revision = 3;
        snapshot.history.page.data = vec![entry("c"), entry("d")];
        snapshot.history.page.next_cursor = None;
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, cx);
            let ids: Vec<_> = panel
                .entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect();
            assert_eq!(ids, ["c", "d"]);
            assert_eq!(panel.next_cursor, None);
        });
    }

    #[gpui::test]
    fn a_failed_load_keeps_the_list_and_stops_waiting(cx: &mut TestAppContext) {
        let mut snapshot = signed_in(vec![entry("a")], None);
        let (_window, panel, _commands) = panel(cx, snapshot.clone());
        panel.update(cx, |panel, cx| panel.refresh(cx));
        snapshot.history.revision = 2;
        snapshot.history.page = HistoryPage::default();
        snapshot.history.error = Some("native account request returned 503".into());
        snapshot.messages.push(ClientMessage {
            id: 1,
            operation: "load history".into(),
            error: ClientError {
                code: ClientErrorCode::Remote,
                detail: "native account request returned 503".into(),
            },
        });
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, cx);
            assert_eq!(panel.loading, None);
            assert_eq!(panel.entries.len(), 1);
            assert_eq!(
                panel.error.as_deref(),
                Some("The server had a problem. Try again shortly.")
            );
        });
    }

    #[gpui::test]
    fn extending_needs_a_choice_and_deleting_asks_first(cx: &mut TestAppContext) {
        let mut snapshot = signed_in(vec![entry("a")], None);
        let (window, panel, commands) = panel(cx, snapshot.clone());
        panel.update(cx, |panel, cx| {
            panel.select("a".into(), cx);
            panel.select("missing".into(), cx);
            assert_eq!(panel.selected.as_deref(), Some("a"));
            panel.extend(&entry("a"), cx);
        });
        assert!(commands.try_recv().is_err(), "nothing is chosen yet");

        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("history-extend-96", cx);
        });
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("history-save", cx);
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::ExtendHistory { id, retention_hours: 96 }) if id == "a"
        ));

        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("history-delete", cx);
        });
        assert!(commands.try_recv().is_err(), "deleting asks first");
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("dialog-confirm", cx);
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::DeleteHistory { id }) if id == "a"
        ));

        // The entry disappearing from the reloaded page clears the selection.
        snapshot.history.revision = 2;
        snapshot.history.page.data.clear();
        snapshot.operations.push(OperationStatus {
            id: 1,
            operation: "delete history transfer".into(),
        });
        panel.update(cx, |panel, cx| {
            panel.on_snapshot(&snapshot, cx);
            assert!(!panel.waiting("delete history transfer"));
            assert_eq!(panel.selected, None);
        });
    }

    #[gpui::test]
    fn signed_out_shows_no_list_and_never_asks_the_service(cx: &mut TestAppContext) {
        let (window, panel, commands) = panel(cx, DesktopSnapshot::default());
        panel.update(cx, |panel, cx| panel.refresh(cx));
        assert!(commands.try_recv().is_err());
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("history-sign-in").is_some());
            assert!(window.try_find("history-status").is_none());
        });
    }
}
