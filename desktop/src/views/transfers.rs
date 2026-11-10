//! Transfers on this device: what is moving, waiting and done, with the link and
//! actions for the selected one. All transfer work stays in the service worker.

use std::{rc::Rc, sync::Arc, time::Duration};

use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, actions, div, prelude::FluentBuilder, px, size,
};
use gpui_component::{
    VirtualListScrollHandle,
    input::{InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    scroll::ScrollableElement,
    v_virtual_list,
};

use crate::{
    assets::icon,
    client::DesktopClient,
    live::Live,
    model::{
        ClientCommand, DesktopSnapshot, JobSnapshot, TransferDirection, TransferState,
        VerifiedResult,
    },
    theme::{
        self, Palette,
        tokens::{geometry, radius},
    },
    ui::{
        MotionExt, TextExt, Tone, button, dialog, feedback,
        field::{self, copy_field},
        format, motion, page,
        progress::{percent_label, progress},
        row, surface, text,
    },
};

const ROW_HEIGHT: f32 = 68.;
const DETAIL_WIDTH: f32 = 372.;
const DETAIL_WIDTH_COMPACT: f32 = 316.;

actions!(
    transfers_panel,
    [
        PauseSelected,
        ResumeSelected,
        EndLiveSelected,
        RevokeSelected,
        DiscardSelected
    ]
);

/// Registers the selected-transfer shortcuts. Call once per application.
pub fn bind_keys(cx: &mut gpui::App) {
    let key = |suffix: &str| format!("{}-shift-{suffix}", crate::shell::modifier());
    cx.bind_keys([
        gpui::KeyBinding::new(&key("p"), PauseSelected, None),
        gpui::KeyBinding::new(&key("r"), ResumeSelected, None),
        gpui::KeyBinding::new(&key("e"), EndLiveSelected, None),
        gpui::KeyBinding::new(&key("v"), RevokeSelected, None),
        gpui::KeyBinding::new(&key("d"), DiscardSelected, None),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    All,
    Active,
    Completed,
    Failed,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Active, Self::Completed, Self::Failed];

    const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Active => "In progress",
            Self::Completed => "Completed",
            Self::Failed => "Needs attention",
        }
    }

    fn matches(self, job: &JobSnapshot) -> bool {
        match self {
            Self::All => true,
            Self::Active => !matches!(job.state, TransferState::Complete | TransferState::Failed),
            Self::Completed => job.state == TransferState::Complete,
            Self::Failed => job.state == TransferState::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sort {
    Recent,
    Name,
    Progress,
}

impl Sort {
    const fn label(self) -> &'static str {
        match self {
            Self::Recent => "Newest first",
            Self::Name => "By name",
            Self::Progress => "By progress",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Recent => Self::Name,
            Self::Name => Self::Progress,
            Self::Progress => Self::Recent,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JobAction {
    Pause,
    Resume,
    EndLive,
    Revoke,
    Discard,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copied {
    Link,
    Key,
    Command,
    Id,
}

/// A retained transfer workspace. It owns presentation state only.
pub struct TransfersPanel {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    selected_id: Option<String>,
    filter: Filter,
    sort: Sort,
    search: Entity<InputState>,
    rows: Vec<JobSnapshot>,
    copied: Option<Copied>,
    copy_generation: u64,
    scroll_handle: VirtualListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

/// Runtime job identifiers are assigned in order, so a larger one is newer.
fn recency(job: &JobSnapshot) -> u64 {
    job.id.parse::<u64>().unwrap_or(0)
}

fn fraction(job: &JobSnapshot) -> Option<f32> {
    job.progress
        .total_bytes
        .filter(|total| *total > 0)
        .map(|total| (job.progress.completed_bytes as f32 / total as f32).clamp(0., 1.))
}

fn transferred(job: &JobSnapshot) -> String {
    match job.progress.total_bytes {
        Some(total) if job.state == TransferState::Complete => format::bytes(total),
        Some(total) => format!(
            "{} of {}",
            format::bytes(job.progress.completed_bytes),
            format::bytes(total)
        ),
        None => format::bytes(job.progress.completed_bytes),
    }
}

fn direction_label(job: &JobSnapshot) -> &'static str {
    let finished = job.state == TransferState::Complete;
    match job.direction {
        TransferDirection::Send if finished => "Sent",
        TransferDirection::Send => "Sending",
        TransferDirection::Receive if finished => "Received",
        TransferDirection::Receive => "Receiving",
        TransferDirection::Note => "Note",
        TransferDirection::Service => "Service",
    }
}

/// The whole size when it is known, otherwise what has moved so far.
fn size_label(job: &JobSnapshot) -> String {
    format::bytes(
        job.progress
            .total_bytes
            .unwrap_or(job.progress.completed_bytes),
    )
}

fn status(job: &JobSnapshot) -> (String, Tone) {
    match job.state {
        TransferState::Complete if job.capabilities.can_export => {
            ("Verified".into(), Tone::Success)
        }
        TransferState::Complete => ("Complete".into(), Tone::Success),
        TransferState::Failed => ("Needs attention".into(), Tone::Danger),
        TransferState::Paused => ("Paused".into(), Tone::Warning),
        TransferState::PauseRequested => ("Pausing".into(), Tone::Warning),
        TransferState::Running if job.progress.phase.is_empty() => {
            ("In progress".into(), Tone::Accent)
        }
        TransferState::Running => (job.progress.phase.clone(), Tone::Accent),
    }
}

fn display_name(job: &JobSnapshot) -> String {
    if job.progress.name.is_empty() {
        match job.direction {
            TransferDirection::Note => "Encrypted note".into(),
            _ => "Transfer".into(),
        }
    } else {
        job.progress.name.clone()
    }
}

fn tile_icon(job: &JobSnapshot) -> &'static str {
    if job.direction == TransferDirection::Note {
        "note"
    } else {
        format::file_icon(&job.progress.name)
    }
}

/// The one action a row offers without opening its details.
fn quick_action(job: &JobSnapshot) -> Option<(&'static str, JobAction)> {
    let can = &job.capabilities;
    if can.can_resume {
        Some(("Resume", JobAction::Resume))
    } else if can.can_pause {
        Some(("Pause", JobAction::Pause))
    } else if can.can_export {
        Some(("Save", JobAction::Export))
    } else {
        None
    }
}

fn confirmation(action: JobAction) -> Option<(&'static str, &'static str, &'static str)> {
    match action {
        JobAction::EndLive => Some((
            "End this live session?",
            "The connection closes for both people. Anything not yet received stops.",
            "End session",
        )),
        JobAction::Revoke => Some((
            "Revoke this link?",
            "The link stops working and the encrypted content is removed from the server.",
            "Revoke link",
        )),
        JobAction::Discard => Some((
            "Remove from this device?",
            "The local record and any privately held copy are removed. A shared link keeps \
             working until it expires or is revoked.",
            "Remove",
        )),
        JobAction::Pause | JobAction::Resume | JobAction::Export => None,
    }
}

impl TransfersPanel {
    pub fn new(live: Entity<Live>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = live.read(cx).client().clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search transfers"));
        let subscriptions = vec![
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.rebuild_rows(cx);
                }
            }),
            cx.observe(&live, |this, _, cx| this.rebuild_rows(cx)),
        ];
        let mut panel = Self {
            client,
            live,
            selected_id: None,
            filter: Filter::All,
            sort: Sort::Recent,
            search,
            rows: Vec::new(),
            copied: None,
            copy_generation: 0,
            scroll_handle: VirtualListScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        panel.rebuild_rows(cx);
        panel
    }

    pub fn select_job(&mut self, id: String, cx: &mut Context<Self>) {
        if self.snapshot(cx).jobs.iter().any(|job| job.id == id) {
            if self.selected_id.as_deref() != Some(&id) {
                self.copied = None;
            }
            self.selected_id = Some(id);
            cx.notify();
        }
    }

    fn snapshot(&self, cx: &Context<Self>) -> Arc<DesktopSnapshot> {
        self.live.read(cx).snapshot()
    }

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot(cx);
        let query = self.search.read(cx).value().trim().to_lowercase();
        self.rows = snapshot
            .jobs
            .iter()
            .filter(|job| job.is_transfer() && self.filter.matches(job))
            .filter(|job| {
                query.is_empty()
                    || job.id.to_lowercase().contains(&query)
                    || job.progress.name.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        match self.sort {
            // Stable, so jobs without a numeric identifier keep the service's order.
            Sort::Recent => self.rows.sort_by_key(|job| std::cmp::Reverse(recency(job))),
            Sort::Name => self.rows.sort_by(|a, b| {
                a.progress
                    .name
                    .to_lowercase()
                    .cmp(&b.progress.name.to_lowercase())
                    .then_with(|| a.id.cmp(&b.id))
            }),
            Sort::Progress => self.rows.sort_by(|a, b| {
                fraction(b)
                    .unwrap_or(0.)
                    .total_cmp(&fraction(a).unwrap_or(0.))
                    .then_with(|| a.id.cmp(&b.id))
            }),
        }
        // Selection is keyed by job and survives filtering, but not removal.
        if let Some(id) = &self.selected_id
            && !snapshot.jobs.iter().any(|job| &job.id == id)
        {
            self.selected_id = None;
        }
        cx.notify();
    }

    fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.filter = filter;
        self.rebuild_rows(cx);
    }

    fn dispatch(&mut self, action: JobAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id.clone() else {
            return;
        };
        let Some((title, detail, confirm)) = confirmation(action) else {
            self.perform(action, id, window, cx);
            return;
        };
        let entity = cx.entity();
        dialog::prompt(title)
            .body(detail)
            .confirm(confirm)
            .destructive()
            .open(window, cx, move |window, cx| {
                entity.update(cx, |this, cx| this.perform(action, id.clone(), window, cx));
            });
    }

    fn pause_selected(&mut self, _: &PauseSelected, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(JobAction::Pause, window, cx);
    }
    fn resume_selected(&mut self, _: &ResumeSelected, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(JobAction::Resume, window, cx);
    }
    fn end_live_selected(
        &mut self,
        _: &EndLiveSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dispatch(JobAction::EndLive, window, cx);
    }
    fn revoke_selected(&mut self, _: &RevokeSelected, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(JobAction::Revoke, window, cx);
    }
    fn discard_selected(
        &mut self,
        _: &DiscardSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dispatch(JobAction::Discard, window, cx);
    }

    fn perform(
        &mut self,
        action: JobAction,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (command, failure) = match action {
            JobAction::Export => {
                self.export(id, window, cx);
                return;
            }
            JobAction::Pause => (ClientCommand::Pause { id }, "Could not pause"),
            JobAction::Resume => (ClientCommand::Resume { id }, "Could not resume"),
            JobAction::EndLive => (ClientCommand::EndLive { id }, "Could not end the session"),
            JobAction::Revoke => (ClientCommand::Revoke { id }, "Could not revoke the link"),
            JobAction::Discard => (ClientCommand::Discard { id }, "Could not remove it"),
        };
        if let Err(error) = self.client.dispatch(command) {
            crate::toast::error(
                failure,
                format::readable_error(&error.to_string()),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn export(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
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
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(error) = this.client.dispatch(ClientCommand::ExportVerified {
                    transfer_id: id,
                    destination,
                }) {
                    crate::toast::error(
                        "Could not save",
                        format::readable_error(&error.to_string()),
                        window,
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn copy(&mut self, value: String, kind: Copied, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.copied = Some(kind);
        self.copy_generation = self.copy_generation.wrapping_add(1);
        let generation = self.copy_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_generation == generation {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for TransfersPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.snapshot(cx);
        let compact = page::is_compact(window);
        let listed: Vec<_> = snapshot
            .jobs
            .iter()
            .filter(|job| job.is_transfer())
            .collect();
        let selected = self
            .selected_id
            .as_ref()
            .and_then(|id| snapshot.jobs.iter().find(|job| &job.id == id))
            .cloned();
        let verified = selected.as_ref().and_then(|job| {
            snapshot
                .verified_results
                .iter()
                .find(|result| result.transfer_id == job.id)
                .cloned()
        });
        let filtering = self.filter != Filter::All || !self.search.read(cx).value().is_empty();
        let header_entrance = motion::entrance("transfers-toolbar", 1, window, cx);
        let body_entrance = motion::entrance("transfers-body", 2, window, cx);

        let chips: Vec<_> = Filter::ALL
            .into_iter()
            .filter_map(|filter| {
                let count = listed.iter().filter(|job| filter.matches(job)).count();
                // "Needs attention" only appears when something does.
                (filter != Filter::Failed || count > 0).then(|| {
                    let id = SharedString::from(format!("transfer-filter-{}", filter.label()));
                    if self.filter == filter {
                        button::tonal(id)
                    } else {
                        button::ghost(id)
                    }
                    .small()
                    .label(format!("{}  {count}", filter.label()))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx)))
                })
            })
            .collect();

        let list: AnyElement = if self.rows.is_empty() {
            let (title, body) = if filtering {
                ("No transfers match", "Try a different filter or search.")
            } else {
                (
                    "Nothing here yet",
                    "Transfers you send and receive on this device appear here.",
                )
            };
            let reset = filtering.then(|| {
                button::secondary("transfers-clear-filters")
                    .label("Show all transfers")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.search
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        this.set_filter(Filter::All, cx);
                    }))
                    .into_any_element()
            });
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(feedback::empty_state("transfers", title, body).action(reset))
                .into_any_element()
        } else {
            let sizes = Rc::new(vec![size(px(1.), px(ROW_HEIGHT)); self.rows.len()]);
            div()
                .size_full()
                .min_h_0()
                .relative()
                .child(
                    v_virtual_list(
                        cx.entity(),
                        "transfer-rows",
                        sizes,
                        move |this, range, _, cx| {
                            range
                                .filter_map(|index| this.rows.get(index).cloned())
                                .map(|job| this.row(job, p, cx))
                                .collect()
                        },
                    )
                    .size_full()
                    .p(px(6.))
                    .track_scroll(&self.scroll_handle),
                )
                .vertical_scrollbar(&self.scroll_handle)
                .into_any_element()
        };

        let detail = self.detail(selected, verified, p, window, cx);
        let header = page::header(
            p,
            "Transfers",
            "Running, paused and finished transfers on this device.",
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
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .entering(header_entrance)
                        .children(chips)
                        .child(div().flex_1())
                        .child(
                            div()
                                .w(px(if compact { 180. } else { 240. }))
                                .flex_none()
                                .child(
                                    field::input(p, &self.search)
                                        .id("transfer-search")
                                        .prefix(
                                            icon("search").size(px(15.)).text_color(p.text_subtle),
                                        )
                                        .cleanable(true),
                                ),
                        )
                        .child(
                            button::secondary("sort-transfers")
                                .icon("sort")
                                .label(self.sort.label())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sort = this.sort.next();
                                    this.rebuild_rows(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .gap(px(16.))
                        .entering(body_entrance)
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
                ),
        )
        .on_action(cx.listener(Self::pause_selected))
        .on_action(cx.listener(Self::resume_selected))
        .on_action(cx.listener(Self::end_live_selected))
        .on_action(cx.listener(Self::revoke_selected))
        .on_action(cx.listener(Self::discard_selected))
    }
}

impl TransfersPanel {
    fn row(
        &self,
        job: JobSnapshot,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.selected_id.as_deref() == Some(job.id.as_str());
        let (label, tone) = status(&job);
        let active = !matches!(job.state, TransferState::Complete | TransferState::Failed);
        let quick = quick_action(&job);
        let id = job.id.clone();
        let panel = cx.entity();
        let capabilities = job.capabilities.clone();
        let menu_id = job.id.clone();
        let meta = format!("{} · {}", direction_label(&job), transferred(&job));

        div()
            .id(SharedString::from(format!("transfer-{}", job.id)))
            .w_full()
            .h(px(ROW_HEIGHT))
            .py(px(2.))
            .child(
                div()
                    .size_full()
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
                    .child(row::icon_tile(
                        p,
                        tile_icon(&job),
                        40.,
                        job.state != TransferState::Failed,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                text::body_strong(display_name(&job))
                                    .single_line()
                                    .text_color(p.text),
                            )
                            .child(if active && fraction(&job).is_some() {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(
                                        div().w(px(120.)).flex_none().child(
                                            progress(
                                                SharedString::from(format!(
                                                    "row-progress-{}",
                                                    job.id
                                                )),
                                                fraction(&job),
                                            )
                                            .height(4.),
                                        ),
                                    )
                                    .child(
                                        text::caption(meta).single_line().text_color(p.text_muted),
                                    )
                            } else {
                                div().child(
                                    text::caption(meta).single_line().text_color(p.text_muted),
                                )
                            }),
                    )
                    .child(feedback::pill(p, label, tone))
                    .when_some(quick, |this, (name, action)| {
                        let action_id = job.id.clone();
                        this.child(
                            button::secondary(SharedString::from(format!("row-action-{}", job.id)))
                                .small()
                                .label(name)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_job(action_id.clone(), cx);
                                    this.dispatch(action, window, cx);
                                })),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_job(id.clone(), cx)))
            .context_menu(move |menu, _, _| {
                let item = |label: &'static str, action: JobAction, allowed: bool| {
                    let (panel, id) = (panel.clone(), menu_id.clone());
                    PopupMenuItem::new(label)
                        .disabled(!allowed)
                        .on_click(move |_, window, cx| {
                            let id = id.clone();
                            panel.update(cx, |this, cx| {
                                this.select_job(id, cx);
                                this.dispatch(action, window, cx);
                            });
                        })
                };
                menu.item(item("Pause", JobAction::Pause, capabilities.can_pause))
                    .item(item("Resume", JobAction::Resume, capabilities.can_resume))
                    .item(item(
                        "Save to folder",
                        JobAction::Export,
                        capabilities.can_export,
                    ))
                    .separator()
                    .item(item(
                        "End live session",
                        JobAction::EndLive,
                        capabilities.can_end_live,
                    ))
                    .item(item(
                        "Revoke link",
                        JobAction::Revoke,
                        capabilities.can_revoke,
                    ))
                    .item(item(
                        "Remove from this device",
                        JobAction::Discard,
                        capabilities.can_discard,
                    ))
            })
    }

    fn detail(
        &mut self,
        job: Option<JobSnapshot>,
        verified: Option<VerifiedResult>,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(job) = job else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    feedback::empty_state(
                        "info",
                        "Select a transfer",
                        "Its link, progress and actions appear here.",
                    )
                    .tone(Tone::Neutral),
                )
                .into_any_element();
        };
        let entrance = motion::entrance_with(
            SharedString::from(format!("transfer-detail-{}", job.id)),
            0,
            8.,
            motion::enter_quick(),
            window,
            cx,
        );
        let (label, tone) = status(&job);
        let progress_fraction = fraction(&job);
        let active = !matches!(job.state, TransferState::Complete | TransferState::Failed);
        let can = job.capabilities.clone();
        let copy = |value: String, kind: Copied| {
            cx.listener(move |this, _, _, cx| this.copy(value.clone(), kind, cx))
        };
        let act = |action: JobAction| {
            cx.listener(move |this, _, window, cx| this.dispatch(action, window, cx))
        };

        let mut facts = vec![
            ("Kind", direction_label(&job).to_owned()),
            ("Size", size_label(&job)),
        ];
        if job.progress.item_count > 1 {
            facts.push((
                "Items",
                if active {
                    format!("{} of {}", job.progress.item_index, job.progress.item_count)
                } else {
                    job.progress.item_count.to_string()
                },
            ));
        }
        if let Some(origin) = &job.origin {
            facts.push(("Instance", format::host(origin)));
        }

        let primary = if can.can_resume {
            Some(
                button::primary("detail-resume")
                    .label("Resume")
                    .on_click(act(JobAction::Resume)),
            )
        } else if can.can_export {
            Some(
                button::primary("detail-export")
                    .icon("folder")
                    .label(
                        if verified.as_ref().is_some_and(|r| r.export_error.is_some()) {
                            "Try another folder"
                        } else if verified
                            .as_ref()
                            .is_some_and(|r| !r.exported_paths.is_empty())
                        {
                            "Save another copy"
                        } else {
                            "Save to folder"
                        },
                    )
                    .on_click(act(JobAction::Export)),
            )
        } else if can.can_pause {
            Some(
                button::secondary("detail-pause")
                    .label("Pause")
                    .on_click(act(JobAction::Pause)),
            )
        } else {
            None
        };

        div()
            .id("transfer-detail-scroll")
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
                        tile_icon(&job),
                        44.,
                        job.state != TransferState::Failed,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(6.))
                            .child(text::subtitle(display_name(&job)).text_color(p.text))
                            .child(feedback::pill(p, label, tone)),
                    ),
            )
            .when(active, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            progress(
                                SharedString::from(format!("detail-progress-{}", job.id)),
                                progress_fraction,
                            )
                            .height(8.),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .child(text::caption(transferred(&job)).text_color(p.text_muted))
                                .when_some(progress_fraction, |this, fraction| {
                                    this.child(
                                        text::caption(percent_label(fraction))
                                            .text_color(p.text_muted),
                                    )
                                }),
                        ),
                )
            })
            .when_some(job.error.as_ref(), |this, error| {
                this.child(feedback::notice(
                    p,
                    format::readable_error(&error.detail),
                    Tone::Danger,
                ))
            })
            .when_some(job.peer_warning.clone(), |this, warning| {
                this.child(feedback::notice(p, warning, Tone::Warning))
            })
            .when_some(job.share_url.clone(), |this, link| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(text::eyebrow("Share").text_color(p.text_subtle))
                        .child(
                            copy_field("copy-receipt", "link", link.clone())
                                .action("Copy link")
                                .primary()
                                .copied(self.copied == Some(Copied::Link))
                                .on_copy(copy(link, Copied::Link)),
                        )
                        .when_some(job.separate_key.clone(), |this, key| {
                            this.child(
                                // The key is never drawn; only the clipboard receives it.
                                copy_field("copy-receipt-key", "key", "Decryption key · hidden")
                                    .action("Copy key")
                                    .copied(self.copied == Some(Copied::Key))
                                    .on_copy(copy(key, Copied::Key)),
                            )
                        })
                        .when_some(job.cli_command.clone(), |this, command| {
                            this.child(
                                copy_field("copy-receipt-cli", "terminal", command.clone())
                                    .action("Copy")
                                    .copied(self.copied == Some(Copied::Command))
                                    .on_copy(copy(command, Copied::Command)),
                            )
                        }),
                )
            })
            .when_some(verified, |this, result| {
                this.when_some(result.export_error.as_ref(), |this, error| {
                    this.child(feedback::notice(
                        p,
                        format::readable_error(&error.detail),
                        Tone::Danger,
                    ))
                })
                .when(!result.exported_paths.is_empty(), |this| {
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(text::eyebrow("Saved to").text_color(p.text_subtle))
                            .child(
                                surface::well(p)
                                    .px(px(12.))
                                    .py(px(10.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .children(result.exported_paths.iter().map(|path| {
                                        text::code(path.display().to_string())
                                            .single_line()
                                            .text_color(p.text_muted)
                                    })),
                            ),
                    )
                })
            })
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
                                button::ghost("copy-transfer-id")
                                    .small()
                                    .icon(if self.copied == Some(Copied::Id) {
                                        "check"
                                    } else {
                                        "copy"
                                    })
                                    .label(format::abbreviated(
                                        job.checkpoint_id.as_deref().unwrap_or(&job.id),
                                        8,
                                        4,
                                    ))
                                    .tooltip("Copy the transfer ID")
                                    .on_click(copy(
                                        job.checkpoint_id.clone().unwrap_or_else(|| job.id.clone()),
                                        Copied::Id,
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .mt_auto()
                    .pt(px(4.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(primary.map(|action| action.fill()))
                    .when(can.can_pause && can.can_export, |this| {
                        this.child(
                            button::secondary("detail-pause")
                                .fill()
                                .label("Pause")
                                .on_click(act(JobAction::Pause)),
                        )
                    })
                    .when(
                        can.can_end_live || can.can_revoke || can.can_discard,
                        |this| {
                            this.child(surface::hairline(p)).child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .gap(px(2.))
                                    .when(can.can_end_live, |this| {
                                        this.child(
                                            button::danger("detail-end-live")
                                                .small()
                                                .icon("x")
                                                .label("End live session")
                                                .on_click(act(JobAction::EndLive)),
                                        )
                                    })
                                    .when(can.can_revoke, |this| {
                                        this.child(
                                            button::danger("detail-revoke")
                                                .small()
                                                .icon("slash")
                                                .label("Revoke link")
                                                .on_click(act(JobAction::Revoke)),
                                        )
                                    })
                                    .when(can.can_discard, |this| {
                                        this.child(
                                            button::danger("detail-discard")
                                                .small()
                                                .icon("trash")
                                                .label("Remove from this device")
                                                .on_click(act(JobAction::Discard)),
                                        )
                                    }),
                            )
                        },
                    ),
            )
            .into_any_element()
    }
}

#[cfg(feature = "visual-test")]
impl TransfersPanel {
    pub fn confirm_revoke_for_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(JobAction::Revoke, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, WindowHandle, test::TestWindowExt as _};
    use gpui_component::Root;

    use super::*;
    use crate::{
        model::{TransferCapabilities, TransferProgress},
        testing::{self, Commands},
    };

    fn job(
        id: &str,
        name: &str,
        direction: TransferDirection,
        state: TransferState,
    ) -> JobSnapshot {
        JobSnapshot {
            id: id.into(),
            direction,
            state,
            progress: TransferProgress {
                name: name.into(),
                total_bytes: Some(100),
                completed_bytes: if state == TransferState::Complete {
                    100
                } else {
                    40
                },
                ..TransferProgress::default()
            },
            share_url: None,
            separate_key: None,
            cli_command: None,
            checkpoint_id: None,
            origin: None,
            error: None,
            peer_warning: None,
            capabilities: TransferCapabilities::default(),
        }
    }

    fn panel(
        cx: &mut TestAppContext,
        jobs: Vec<JobSnapshot>,
    ) -> (WindowHandle<Root>, Entity<TransfersPanel>, Commands) {
        testing::rooted(
            cx,
            DesktopSnapshot {
                jobs,
                ..DesktopSnapshot::default()
            },
            TransfersPanel::new,
        )
    }

    fn mixed() -> Vec<JobSnapshot> {
        vec![
            job(
                "1",
                "b.zip",
                TransferDirection::Send,
                TransferState::Complete,
            ),
            job(
                "2",
                "a.pdf",
                TransferDirection::Receive,
                TransferState::Running,
            ),
            job(
                "3",
                "prepare",
                TransferDirection::Service,
                TransferState::Complete,
            ),
            job("4", "c.mp4", TransferDirection::Send, TransferState::Failed),
        ]
    }

    #[gpui::test]
    fn the_list_hides_internal_jobs_and_shows_the_newest_first(cx: &mut TestAppContext) {
        let (_window, panel, _commands) = panel(cx, mixed());
        panel.update(cx, |panel, cx| {
            let ids = |panel: &TransfersPanel| {
                panel
                    .rows
                    .iter()
                    .map(|job| job.id.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(ids(panel), ["4", "2", "1"]);

            panel.sort = Sort::Name;
            panel.rebuild_rows(cx);
            assert_eq!(ids(panel), ["2", "1", "4"]);

            panel.set_filter(Filter::Active, cx);
            assert_eq!(ids(panel), ["2"]);
            panel.set_filter(Filter::Failed, cx);
            assert_eq!(ids(panel), ["4"]);
            panel.set_filter(Filter::Completed, cx);
            assert_eq!(ids(panel), ["1"]);
        });
    }

    #[gpui::test]
    fn selection_survives_filtering_and_removal_waits_for_confirmation(cx: &mut TestAppContext) {
        let mut jobs = mixed();
        jobs[0].capabilities.can_pause = true;
        jobs[0].capabilities.can_discard = true;
        let (window, panel, commands) = panel(cx, jobs);
        let act = |cx: &mut TestAppContext, action: JobAction| {
            testing::in_window(cx, window, |window, cx| {
                panel.update(cx, |panel, cx| panel.dispatch(action, window, cx));
            });
        };
        let click = |cx: &mut TestAppContext, id: &'static str| {
            testing::in_window(cx, window, |window, cx| {
                window.render_frame(cx);
                window.click(id, cx);
            });
        };

        panel.update(cx, |panel, cx| {
            panel.select_job("1".into(), cx);
            panel.select_job("missing".into(), cx);
            assert_eq!(panel.selected_id.as_deref(), Some("1"));
            panel.set_filter(Filter::Active, cx);
            assert_eq!(panel.selected_id.as_deref(), Some("1"));
        });
        act(cx, JobAction::Pause);
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::Pause { id }) if id == "1"
        ));

        // Cancelling leaves the transfer alone; confirming removes it.
        act(cx, JobAction::Discard);
        assert!(commands.try_recv().is_err(), "removal asks first");
        click(cx, "dialog-cancel");
        assert!(commands.try_recv().is_err());
        act(cx, JobAction::Discard);
        click(cx, "dialog-confirm");
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::Discard { id }) if id == "1"
        ));
    }

    #[gpui::test]
    fn rows_and_the_detail_pane_render_for_every_state(cx: &mut TestAppContext) {
        let mut jobs = mixed();
        // Both could be paused, but only the transfer is listed with its action.
        jobs[1].capabilities.can_pause = true;
        jobs[2].capabilities.can_pause = true;
        jobs[0].share_url = Some("https://files.example/1#k=v1.key".into());
        jobs[0].capabilities.can_revoke = true;
        let (window, panel, _commands) = panel(cx, jobs);
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            assert!(window.try_find("transfer-search").is_some());
            assert!(window.try_find("sort-transfers").is_some());
            assert!(window.try_find("row-action-2").is_some());
            assert!(window.try_find("row-action-3").is_none());
        });
        panel.update(cx, |panel, cx| panel.select_job("1".into(), cx));
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("copy-receipt").is_some());
            assert!(window.try_find("detail-revoke").is_some());
        });
    }

    #[test]
    fn statuses_and_quick_actions_follow_capabilities() {
        let mut running = job("1", "a", TransferDirection::Send, TransferState::Running);
        assert_eq!(status(&running).0, "In progress");
        assert_eq!(direction_label(&running), "Sending");
        running.progress.phase = "Uploading".into();
        assert_eq!(status(&running).0, "Uploading");
        assert_eq!(quick_action(&running), None);
        running.capabilities.can_pause = true;
        assert_eq!(quick_action(&running), Some(("Pause", JobAction::Pause)));
        running.capabilities.can_resume = true;
        assert_eq!(quick_action(&running), Some(("Resume", JobAction::Resume)));

        let mut done = job(
            "2",
            "a",
            TransferDirection::Receive,
            TransferState::Complete,
        );
        assert_eq!(status(&done), ("Complete".into(), Tone::Success));
        assert_eq!(direction_label(&done), "Received");
        done.capabilities.can_export = true;
        assert_eq!(status(&done).0, "Verified");
        assert_eq!(transferred(&done), "100 B");
        assert_eq!(size_label(&running), "100 B");
        assert!(confirmation(JobAction::Pause).is_none());
        assert!(confirmation(JobAction::Revoke).is_some());
    }
}
