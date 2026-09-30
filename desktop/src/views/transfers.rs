use std::{rc::Rc, sync::Arc, time::Duration};

use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, ClipboardItem, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    PathPromptOptions, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Task, Window, actions, div, px, size,
};
use gpui_component::{
    Disableable, Icon, Selectable, VirtualListScrollHandle, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    scroll::ScrollableElement,
    v_virtual_list,
};

use crate::{
    client::DesktopClient,
    model::{ClientCommand, DesktopSnapshot, JobSnapshot, TransferState, VerifiedResult},
    theme::{Palette, palette_for},
};
use gpui_component::theme::Theme;

const ROW_HEIGHT: f32 = 82.;
const ROW_PADDING: f32 = 16.;
const ROW_GAP: f32 = 12.;
const ROW_ICON_WIDTH: f32 = 34.;
const DETAIL_WIDTH: f32 = 330.;
const ACTION_WIDTH: f32 = 68.;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Active,
    Completed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Progress,
}

#[derive(Clone, Copy)]
enum JobAction {
    Pause,
    Resume,
    EndLive,
    Revoke,
    Discard,
    Export,
}

/// A retained transfer workspace. It owns UI state only; all transfer work stays in DesktopClient.
pub struct TransfersPanel {
    client: Arc<DesktopClient>,
    selected_id: Option<String>,
    filter: Filter,
    sort: Sort,
    search: Entity<InputState>,
    rows: Vec<JobSnapshot>,
    snapshot: DesktopSnapshot,
    notice: Option<String>,
    receipt_revision: u64,
    scroll_handle: VirtualListScrollHandle,
    _subscriptions: Vec<Subscription>,
    _refresh: Task<()>,
}

impl TransfersPanel {
    pub fn new(
        client: Arc<DesktopClient>,
        _: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search transfers"));
        cx.bind_keys([
            gpui::KeyBinding::new("ctrl-shift-p", PauseSelected, None),
            gpui::KeyBinding::new("ctrl-shift-r", ResumeSelected, None),
            gpui::KeyBinding::new("ctrl-shift-e", EndLiveSelected, None),
            gpui::KeyBinding::new("ctrl-shift-v", RevokeSelected, None),
            gpui::KeyBinding::new("ctrl-shift-d", DiscardSelected, None),
        ]);
        let client_for_refresh = client.clone();
        let refresh = cx.spawn(async move |this, cx| {
            let mut previous = String::new();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let snapshot = client_for_refresh.snapshot();
                let fingerprint = visible_fingerprint(&snapshot);
                if fingerprint != previous {
                    previous = fingerprint;
                    if this
                        .update(cx, |this, cx| {
                            this.snapshot = snapshot;
                            this.rebuild_rows(cx);
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        let mut panel = Self {
            snapshot: client.snapshot(),
            client,
            selected_id: None,
            filter: Filter::All,
            sort: Sort::Name,
            search: search.clone(),
            rows: Vec::new(),
            notice: None,
            receipt_revision: 0,
            scroll_handle: VirtualListScrollHandle::new(),
            _subscriptions: Vec::new(),
            _refresh: refresh,
        };
        panel.rebuild_rows(cx);
        panel
            ._subscriptions
            .push(cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.rebuild_rows(cx);
                }
            }));
        panel
    }

    pub fn select_job(&mut self, id: String, cx: &mut Context<Self>) {
        if self.snapshot.jobs.iter().any(|job| job.id == id) {
            self.selected_id = Some(id);
            cx.notify();
        }
    }

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_lowercase();
        self.rows = self.snapshot.jobs.clone();
        self.rows.retain(|job| {
            matches!(self.filter, Filter::All)
                || matches!(self.filter, Filter::Active)
                    && !matches!(job.state, TransferState::Complete)
                || matches!(self.filter, Filter::Completed)
                    && matches!(job.state, TransferState::Complete)
        });
        self.rows.retain(|job| {
            query.is_empty()
                || job.id.to_lowercase().contains(&query)
                || job.progress.name.to_lowercase().contains(&query)
        });
        self.rows.sort_by(|a, b| match self.sort {
            Sort::Name => a
                .progress
                .name
                .cmp(&b.progress.name)
                .then_with(|| a.id.cmp(&b.id)),
            Sort::Progress => a
                .progress
                .completed_bytes
                .cmp(&b.progress.completed_bytes)
                .then_with(|| a.id.cmp(&b.id)),
        });
        // Selection is keyed by stable job ID and is intentionally not cleared by filters.
        cx.notify();
    }

    fn dispatch(&mut self, action: JobAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(job) = self.selected_job().cloned() else {
            return;
        };
        if requires_confirmation(action) {
            let entity = cx.entity();
            window.open_dialog(cx, move |dialog, _, _| {
                let entity = entity.clone();
                let job_id = job.id.clone();
                dialog
                    .title(confirmation_title(action))
                    .child(confirmation_copy(action))
                    .on_ok(move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.perform(action, job_id.clone(), window, cx)
                        });
                        true
                    })
            });
        } else {
            self.perform(action, job.id, window, cx);
        }
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
        if matches!(action, JobAction::Export) {
            self.export(id, cx);
            return;
        }
        let command = match action {
            JobAction::Pause => ClientCommand::Pause { id },
            JobAction::Resume => ClientCommand::Resume { id },
            JobAction::EndLive => ClientCommand::EndLive { id },
            JobAction::Revoke => ClientCommand::Revoke { id },
            JobAction::Discard => ClientCommand::Discard { id },
            JobAction::Export => unreachable!(),
        };
        let operation = command.operation();
        match self.client.dispatch(command) {
            Ok(()) => self.notice = Some("Action requested.".into()),
            Err(error) => {
                crate::toast::error(format!("{operation} failed"), error.to_string(), window, cx);
                self.notice = None;
            }
        }
        cx.notify();
    }

    fn export(&mut self, id: String, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose export folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(destination) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                match this.client.dispatch(ClientCommand::ExportVerified {
                    transfer_id: id,
                    destination,
                }) {
                    Ok(()) => this.notice = Some("Verified export submitted.".into()),
                    Err(error) => {
                        this.notice = None;
                        crate::toast::error(
                            "Export verified failed",
                            error.to_string(),
                            window,
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn selected_job(&self) -> Option<&JobSnapshot> {
        self.selected_id
            .as_ref()
            .and_then(|id| self.snapshot.jobs.iter().find(|job| &job.id == id))
    }

    fn copy_receipt(&mut self, cx: &mut Context<Self>) {
        let Some(value) = self.selected_job().and_then(|job| job.share_url.clone()) else {
            return;
        };
        self.copy_value(value, "Receipt", cx);
    }

    fn copy_key(&mut self, cx: &mut Context<Self>) {
        let Some(value) = self.selected_job().and_then(|job| job.separate_key.clone()) else {
            return;
        };
        self.copy_value(value, "Key", cx);
    }

    fn copy_cli(&mut self, cx: &mut Context<Self>) {
        let Some(value) = self.selected_job().and_then(|job| job.cli_command.clone()) else {
            return;
        };
        self.copy_value(value, "CLI command", cx);
    }

    fn copy_value(&mut self, value: String, label: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value.clone()));
        self.notice = match cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .filter(|text| text == &value)
        {
            Some(_) => Some(format!("{label} copied to the clipboard.")),
            None => Some(format!("The system clipboard did not confirm the {label}.")),
        };
        self.receipt_revision = self.receipt_revision.wrapping_add(1);
        let revision = self.receipt_revision;
        let selected = self.selected_id.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2200))
                .await;
            let _ = this.update(cx, |this, cx| {
                let still_selected = this.selected_id == selected
                    && this.selected_job().is_some_and(|job| {
                        job.share_url.as_deref() == Some(&value)
                            || job.separate_key.as_deref() == Some(&value)
                            || job.cli_command.as_deref() == Some(&value)
                    });
                if this.receipt_revision == revision && still_selected {
                    this.notice = None;
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
        let p = palette_for(Theme::global(cx).mode);
        let vertical_padding = workspace_vertical_padding(window);
        // Keep the detail pane at compact desktop widths by folding progress into
        // the transfer column rather than allowing fixed columns to overflow.
        let compact = window.viewport_size().width < px(1100.);
        let selected = self.selected_job().cloned();
        let verified = selected.as_ref().and_then(|job| {
            self.snapshot
                .verified_results
                .iter()
                .find(|result| result.transfer_id == job.id)
                .cloned()
        });
        let sizes = Rc::new(vec![size(px(1.), px(ROW_HEIGHT)); self.rows.len()]);
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(if compact { 21. } else { 36. }))
            .child(
                div()
                    .size_full()
                    .min_h_0()
                    .max_w(px(crate::theme::tokens::geometry::CONTENT_MAX))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .pt(px(vertical_padding))
                    .gap(px(crate::views::page::HEADER_GAP))
                    .child(crate::views::page::heading(
                        p,
                        "Transfers",
                        "A clear view of what is moving, waiting, and ready.",
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .rounded(px(12.))
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border)
                            .child(
                                div()
                                    .px(px(14.))
                                    .py(px(12.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .border_b_1()
                                    .border_color(p.border)
                                    .child(filter_button(
                                        "all",
                                        "All",
                                        self.filter == Filter::All,
                                        Filter::All,
                                        cx,
                                    ))
                                    .child(filter_button(
                                        "active",
                                        "Active",
                                        self.filter == Filter::Active,
                                        Filter::Active,
                                        cx,
                                    ))
                                    .child(filter_button(
                                        "completed",
                                        "Completed",
                                        self.filter == Filter::Completed,
                                        Filter::Completed,
                                        cx,
                                    ))
                                    .child(div().flex_1())
                                    .child(Input::new(&self.search).w(px(190.)))
                                    .child(sort_button(self.sort, cx)),
                            )
                            .child(
                                div()
                                    .h(px(35.))
                                    .flex()
                                    .items_center()
                                    .border_b_1()
                                    .border_color(p.border_soft)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .px(px(ROW_PADDING))
                                            .flex()
                                            .items_center()
                                            .gap(px(ROW_GAP))
                                            .border_r_1()
                                            .border_color(p.border)
                                            .child(div().w(px(ROW_ICON_WIDTH)))
                                            .child(
                                                div()
                                                    .w(px(name_width(compact)))
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .truncate()
                                                    .text_size(px(10.))
                                                    .text_color(p.subtle)
                                                    .child("TRANSFER"),
                                            )
                                            .child(
                                                div()
                                                    .w(px(status_width(compact)))
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .truncate()
                                                    .text_size(px(10.))
                                                    .text_color(p.subtle)
                                                    .child("STATUS"),
                                            )
                                            .when(compact, |this| {
                                                this.child(div().flex_1().min_w_0())
                                            })
                                            .when(!compact, |this| {
                                                this.child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .text_ellipsis()
                                                        .truncate()
                                                        .text_size(px(10.))
                                                        .text_color(p.subtle)
                                                        .child("SIZE / PROGRESS"),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .w(px(ACTION_WIDTH))
                                                    .min_w_0()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .truncate()
                                                    .text_size(px(10.))
                                                    .text_color(p.subtle)
                                                    .child("ACTION"),
                                            ),
                                    )
                                    .child(div().w(px(DETAIL_WIDTH)).flex_none().h_full()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .min_h_0()
                                            .relative()
                                            .flex()
                                            .flex_col()
                                            .overflow_hidden()
                                            .border_r_1()
                                            .border_color(p.border)
                                            .child(
                                                v_virtual_list(
                                                    cx.entity(),
                                                    "transfer-rows",
                                                    sizes,
                                                    move |this, range, _, cx| {
                                                        range
                                                            .filter_map(|index| {
                                                                this.rows.get(index).cloned()
                                                            })
                                                            .map(|job| {
                                                                transfer_row(
                                                                    job,
                                                                    this.selected_id.as_deref(),
                                                                    compact,
                                                                    p,
                                                                    cx,
                                                                )
                                                            })
                                                            .collect()
                                                    },
                                                )
                                                .flex_1()
                                                .min_h_0()
                                                .pb(px(vertical_padding))
                                                .track_scroll(&self.scroll_handle),
                                            )
                                            .vertical_scrollbar(&self.scroll_handle),
                                    )
                                    .child(details(
                                        selected,
                                        verified,
                                        self.notice.clone(),
                                        vertical_padding,
                                        p,
                                        cx,
                                    )),
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

fn transfer_row(
    job: JobSnapshot,
    selected: Option<&str>,
    compact: bool,
    p: Palette,
    cx: &mut Context<TransfersPanel>,
) -> impl IntoElement + use<> {
    let id = job.id.clone();
    let action_id = id.clone();
    let panel = cx.entity();
    let context_id = id.clone();
    let capabilities = job.capabilities.clone();
    let row_action = row_action(&job);
    div()
        .id(SharedString::from(format!("transfer-{}", id)))
        .h(px(ROW_HEIGHT))
        .px(px(ROW_PADDING))
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .border_b_1()
        .border_color(p.border_soft)
        .bg(if selected == Some(id.as_str()) {
            p.selected
        } else {
            p.surface
        })
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
                .text_color(file_color(&job, p))
                .child(
                    Icon::default()
                        .path(format!("icons/{}.svg", file_icon(&job)))
                        .size(px(18.)),
                ),
        )
        .child(
            div()
                .w(px(name_width(compact)))
                .min_w_0()
                .child(div().text_size(px(13.)).child(job.progress.name.clone()))
                .child(
                    div()
                        .mt(px(3.))
                        .text_size(px(10.))
                        .text_color(p.subtle)
                        .child(format!(
                            "{} · {}",
                            direction_label(&job),
                            job.progress.phase
                        )),
                )
                .when(compact, |this| {
                    this.child(
                        div()
                            .mt(px(3.))
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child(progress_line(&job)),
                    )
                    .when(progress_percent(&job).is_some(), |this| {
                        this.child(progress_bar(&job, p))
                    })
                }),
        )
        .child(status_pill(&job, p, compact))
        .when(!compact, |this| {
            this.child(
                div()
                    .flex_1()
                    .min_w(px(100.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(progress_line(&job)),
                    )
                    .when(progress_percent(&job).is_some(), |this| {
                        this.child(progress_bar(&job, p))
                    }),
            )
        })
        .when(compact, |this| this.child(div().flex_1().min_w_0()))
        .child(
            Button::new(SharedString::from(format!("row-action-{}", job.id)))
                .label(row_action.0)
                .ghost()
                .w(px(ACTION_WIDTH))
                .disabled(!row_action.2)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_job(action_id.clone(), cx);
                    this.dispatch(row_action.1, window, cx);
                })),
        )
        .on_click(cx.listener(move |this, _, _, cx| this.select_job(id.clone(), cx)))
        .context_menu(move |menu, _, _| {
            context_action(
                menu,
                "Pause",
                JobAction::Pause,
                capabilities.can_pause,
                panel.clone(),
                context_id.clone(),
            )
            .item(
                PopupMenuItem::new("Resume")
                    .disabled(!capabilities.can_resume)
                    .on_click({
                        let panel = panel.clone();
                        let id = context_id.clone();
                        move |_, window, cx| {
                            let id = id.clone();
                            panel.update(cx, |this, cx| {
                                this.select_job(id, cx);
                                this.dispatch(JobAction::Resume, window, cx);
                            });
                        }
                    }),
            )
        })
}

fn context_action(
    menu: gpui_component::menu::PopupMenu,
    label: &'static str,
    action: JobAction,
    allowed: bool,
    panel: Entity<TransfersPanel>,
    id: String,
) -> gpui_component::menu::PopupMenu {
    menu.item(
        PopupMenuItem::new(label)
            .disabled(!allowed)
            .on_click(move |_, window, cx| {
                let id = id.clone();
                panel.update(cx, |this, cx| {
                    this.select_job(id, cx);
                    this.dispatch(action, window, cx);
                });
            }),
    )
}

fn details(
    job: Option<JobSnapshot>,
    verified: Option<VerifiedResult>,
    notice: Option<String>,
    bottom_padding: f32,
    p: Palette,
    cx: &mut Context<TransfersPanel>,
) -> impl IntoElement {
    let mut body = div()
        .id("transfer-detail-scroll")
        .w(px(DETAIL_WIDTH))
        .flex_none()
        .min_w_0()
        .min_h_0()
        .overflow_y_scrollbar()
        .p(px(20.))
        .pb(px(bottom_padding))
        .flex()
        .flex_col()
        .gap(px(12.));
    let Some(job) = job else {
        return body.child("Select a transfer to see its details.");
    };
    body = body
        .child(div().text_size(px(20.)).child(job.progress.name.clone()))
        .child(
            div()
                .text_color(p.muted)
                .child(format!("{} | {}", job.id, job.progress.phase)),
        )
        .child(
            div()
                .child(format!("Progress: {}", progress_line(&job)))
                .child(format!(
                    "Committed: {}",
                    bytes(job.progress.committed_bytes)
                ))
                .child(format!("On wire: {}", bytes(job.progress.wire_bytes))),
        )
        .child(action_buttons(&job, cx));
    if let Some(url) = job.share_url.as_ref() {
        body = body
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child("Receipt"),
            )
            .child(div().text_size(px(12.)).child(url.clone()))
            .child(
                Button::new("copy-receipt")
                    .label("Copy receipt")
                    .on_click(cx.listener(|this, _, _, cx| this.copy_receipt(cx))),
            );
    }
    if job.separate_key.is_some() {
        body = body.child(
            Button::new("copy-receipt-key")
                .label("Copy key")
                .on_click(cx.listener(|this, _, _, cx| this.copy_key(cx))),
        );
    }
    if job.cli_command.is_some() {
        body = body.child(
            Button::new("copy-receipt-cli")
                .label("Copy CLI command")
                .on_click(cx.listener(|this, _, _, cx| this.copy_cli(cx))),
        );
    }
    if let Some(result) = verified {
        if !result.exported_paths.is_empty() {
            body = body.child(format!(
                "Saved: {}",
                result
                    .exported_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if result.export_error.is_some() && job.capabilities.can_retry_export {
            body = body.child(Button::new("retry-export").label("Retry export").on_click(
                cx.listener(|this, _, window, cx| this.dispatch(JobAction::Export, window, cx)),
            ));
        }
    }
    body.children(notice.map(|message| div().text_size(px(11.)).text_color(p.muted).child(message)))
}

fn action_buttons(job: &JobSnapshot, cx: &mut Context<TransfersPanel>) -> impl IntoElement {
    let mut row = div().flex().flex_wrap().gap(px(8.));
    for (name, action, allowed) in [
        ("Pause", JobAction::Pause, job.capabilities.can_pause),
        ("Resume", JobAction::Resume, job.capabilities.can_resume),
        (
            "End live",
            JobAction::EndLive,
            job.capabilities.can_end_live,
        ),
        ("Revoke", JobAction::Revoke, job.capabilities.can_revoke),
        ("Discard", JobAction::Discard, job.capabilities.can_discard),
        (
            "Export verified",
            JobAction::Export,
            job.capabilities.can_export,
        ),
    ] {
        if allowed {
            row = row.child(
                Button::new(SharedString::from(format!("{}-{}", name, job.id)))
                    .label(name)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.dispatch(action, window, cx)),
                    ),
            );
        }
    }
    row
}

fn filter_button(
    id: &'static str,
    label: &'static str,
    selected: bool,
    filter: Filter,
    cx: &mut Context<TransfersPanel>,
) -> impl IntoElement {
    Button::new(id)
        .label(label)
        .ghost()
        .selected(selected)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.filter = filter;
            this.rebuild_rows(cx);
        }))
}

fn sort_button(sort: Sort, cx: &mut Context<TransfersPanel>) -> impl IntoElement {
    Button::new("sort-transfers")
        .label(if sort == Sort::Name {
            "Sort: name"
        } else {
            "Sort: progress"
        })
        .ghost()
        .on_click(cx.listener(|this, _, _, cx| {
            this.sort = if this.sort == Sort::Name {
                Sort::Progress
            } else {
                Sort::Name
            };
            this.rebuild_rows(cx);
        }))
}

fn progress_line(job: &JobSnapshot) -> String {
    let count = if job.progress.item_count > 0 {
        format!(
            " | {}/{} items",
            job.progress.item_index, job.progress.item_count
        )
    } else {
        String::new()
    };
    match job.progress.total_bytes {
        Some(total) => format!(
            "{} / {}{}",
            bytes(job.progress.completed_bytes),
            bytes(total),
            count
        ),
        None => format!(
            "{} transferred (size unknown){}",
            bytes(job.progress.completed_bytes),
            count
        ),
    }
}
fn bytes(value: u64) -> String {
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
fn progress_percent(job: &JobSnapshot) -> Option<f32> {
    job.progress
        .total_bytes
        .filter(|total| *total > 0)
        .map(|total| (job.progress.completed_bytes as f32 / total as f32).clamp(0., 1.))
}
fn progress_bar(job: &JobSnapshot, p: Palette) -> impl IntoElement {
    let width = progress_percent(job).unwrap_or_default() * 150.;
    div()
        .mt(px(6.))
        .w(px(150.))
        .h(px(3.))
        .rounded(px(2.))
        .bg(p.border)
        .child(
            div()
                .h_full()
                .w(px(width))
                .rounded(px(2.))
                .bg(status_color(job, p)),
        )
}
fn direction_label(job: &JobSnapshot) -> &'static str {
    match job.direction {
        crate::model::TransferDirection::Send => "Sent",
        crate::model::TransferDirection::Receive => "Received",
        crate::model::TransferDirection::Note => "Note",
        crate::model::TransferDirection::Service => "Service",
    }
}
fn file_icon(job: &JobSnapshot) -> &'static str {
    let name = job.progress.name.to_lowercase();
    if name.ends_with(".zip") || name.ends_with(".tar") || name.ends_with(".gz") {
        "archive"
    } else if name.ends_with(".mp4") || name.ends_with(".mov") || name.ends_with(".mkv") {
        "video"
    } else if name.ends_with(".txt") || name.ends_with(".md") || name.ends_with(".note") {
        "note"
    } else {
        "file"
    }
}
fn file_color(job: &JobSnapshot, p: Palette) -> gpui::Rgba {
    if matches!(file_icon(job), "archive" | "video") {
        p.warning
    } else {
        p.accent
    }
}
fn status_color(job: &JobSnapshot, p: Palette) -> gpui::Rgba {
    match job.state {
        TransferState::Complete => p.success,
        TransferState::Failed => p.danger,
        TransferState::Paused | TransferState::PauseRequested => p.warning,
        TransferState::Running => p.accent,
    }
}
fn status_pill(job: &JobSnapshot, p: Palette, compact: bool) -> impl IntoElement {
    div().w(px(status_width(compact))).child(
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(7.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(p.inset)
            .text_size(px(10.))
            .text_color(status_color(job, p))
            .child("●")
            .child(state_label(job)),
    )
}
fn name_width(compact: bool) -> f32 {
    if compact { 180. } else { 260. }
}
fn status_width(compact: bool) -> f32 {
    if compact { 140. } else { 193. }
}
fn state_label(job: &JobSnapshot) -> String {
    match job.state {
        TransferState::Complete if job.capabilities.can_export => "Verified · ready to save".into(),
        TransferState::Complete => "Complete".into(),
        TransferState::Failed => "Needs attention".into(),
        TransferState::Paused => "Paused".into(),
        TransferState::PauseRequested => "Pausing".into(),
        TransferState::Running => job.progress.phase.clone(),
    }
}
fn row_action(job: &JobSnapshot) -> (&'static str, JobAction, bool) {
    if job.capabilities.can_resume {
        ("Resume", JobAction::Resume, true)
    } else if job.capabilities.can_pause {
        ("Pause", JobAction::Pause, true)
    } else if job.capabilities.can_export {
        ("Save", JobAction::Export, true)
    } else if job.capabilities.can_discard {
        ("Discard", JobAction::Discard, true)
    } else {
        ("Details", JobAction::Pause, false)
    }
}
fn requires_confirmation(action: JobAction) -> bool {
    matches!(
        action,
        JobAction::EndLive | JobAction::Revoke | JobAction::Discard
    )
}
fn confirmation_title(action: JobAction) -> &'static str {
    match action {
        JobAction::EndLive => "End live transfer",
        JobAction::Revoke => "Revoke transfer",
        JobAction::Discard => "Discard transfer",
        _ => "Confirm action",
    }
}
fn confirmation_copy(action: JobAction) -> &'static str {
    match action {
        JobAction::EndLive => "This ends the live session for both peers.",
        JobAction::Revoke => "This revokes the shared transfer.",
        JobAction::Discard => "This removes the local transfer record.",
        _ => "",
    }
}
fn workspace_vertical_padding(window: &Window) -> f32 {
    if window.viewport_size().width < px(1100.) {
        21.
    } else {
        crate::theme::tokens::geometry::WORKSPACE_VERTICAL_PADDING
    }
}
fn visible_fingerprint(snapshot: &DesktopSnapshot) -> String {
    let jobs = snapshot
        .jobs
        .iter()
        .map(|job| {
            format!(
                "{}:{:?}:{}:{}:{}",
                job.id,
                job.state as u8,
                job.progress.completed_bytes,
                job.progress.committed_bytes,
                job.progress.wire_bytes
            )
        })
        .collect::<String>();
    let exports = snapshot
        .verified_results
        .iter()
        .map(|result| {
            format!(
                "{}:{:?}:{:?}",
                result.transfer_id,
                result.exported_paths,
                result.export_error.is_some()
            )
        })
        .collect::<String>();
    format!("{jobs}|{exports}")
}
