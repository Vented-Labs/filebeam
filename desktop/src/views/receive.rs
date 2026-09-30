//! Retained receive workflow. Transfer data remains in the service worker.

use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, ClickEvent, ClipboardItem, Context, Entity, IntoElement, ParentElement,
    PathPromptOptions, Render, ScrollHandle, SharedString, Styled, Task, Window, div, px,
};
use gpui_component::{
    Icon, Sizable, Size, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    dialog::DialogButtonProps,
    input::{Input, InputState},
};

use crate::{
    client::DesktopClient,
    model::{
        ClientCommand, DesktopSnapshot, PromptAnswer, PromptKind, ReceiveKind, TransferDirection,
        TransferState,
    },
    theme::{Palette, palette_for},
};
use gpui_component::theme::Theme;

/// Native receive form and its local presentation state.
pub struct ReceivePanel {
    client: Arc<DesktopClient>,
    home: PathBuf,
    link: Entity<InputState>,
    secret: Entity<InputState>,
    snapshot: DesktopSnapshot,
    status: Option<String>,
    selected_items: HashSet<String>,
    selected_operation: Option<String>,
    selection_frozen: bool,
    note_text: Option<(String, String)>,
    consumed_notes: HashSet<String>,
    page_scroll: ScrollHandle,
    _refresh: Task<()>,
}

impl ReceivePanel {
    pub fn new(
        client: Arc<DesktopClient>,
        home: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let refresh_client = client.clone();
        let refresh = cx.spawn(async move |this, cx| {
            let mut version = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let snapshot = refresh_client.snapshot();
                let next = snapshot_version(&snapshot);
                if next != version {
                    version = next;
                    if this
                        .update(cx, |panel, cx| {
                            for note in snapshot.notes.iter().filter(|note| note.text_available) {
                                if panel.consumed_notes.insert(note.id.clone())
                                    && let Some(text) = panel.client.take_opened_note(&note.id)
                                {
                                    panel.note_text = Some((note.id.clone(), text));
                                }
                            }
                            panel.snapshot = snapshot;
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        Self {
            snapshot: client.snapshot(),
            client,
            home,
            link: cx.new(|cx| InputState::new(window, cx).placeholder("Transfer link or ID")),
            secret: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Required key or password")
                    .masked(true)
            }),
            status: None,
            selected_items: HashSet::new(),
            selected_operation: None,
            selection_frozen: false,
            note_text: None,
            consumed_notes: HashSet::new(),
            page_scroll: ScrollHandle::new(),
            _refresh: refresh,
        }
    }

    /// Replaces the draft without rewriting a full URL to the configured origin.
    pub fn set_link(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        self.link
            .update(cx, |input, cx| input.set_value(link, window, cx));
        self.status = None;
        self.selected_items.clear();
        self.selected_operation = None;
        self.selection_frozen = false;
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.snapshot = self.client.snapshot();
        cx.notify();
    }

    fn draft(&self, cx: &Context<Self>) -> String {
        self.link.read(cx).value().trim().to_owned()
    }

    fn validation(&self, cx: &Context<Self>) -> Result<String, String> {
        let raw = self.draft(cx);
        if raw.is_empty() {
            return Err("Enter a transfer link or ID.".into());
        }
        let configured = &self.snapshot.instance.url;
        filebeam_transfer_native::protocol::parse_link_for_instance(&raw, configured)
            .map(|parsed| format!("Ready: {}", parsed.instance))
            .map_err(|error| error.to_string())
    }

    fn paste(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        match cx.read_from_clipboard().and_then(|item| item.text()) {
            Some(text) if !text.trim().is_empty() => self.set_link(text, window, cx),
            _ => {
                self.status = Some("Clipboard does not contain text.".into());
                cx.notify();
            }
        }
    }

    fn inspect(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let link = match self.validation(cx) {
            Ok(_) => self.draft(cx),
            Err(error) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        self.status = match self.client.dispatch(ClientCommand::InspectReceive { link }) {
            Ok(()) => Some("Inspecting encrypted manifest before download.".into()),
            Err(error) => {
                crate::toast::error("Inspect receive failed", error.to_string(), window, cx);
                None
            }
        };
        cx.notify();
    }

    fn select_item(
        &mut self,
        operation_id: String,
        item_id: String,
        checked: bool,
        cx: &mut Context<Self>,
    ) {
        if self.selected_operation.as_deref() != Some(&operation_id) {
            self.selected_operation = Some(operation_id);
            self.selected_items.clear();
            self.selection_frozen = false;
        }
        if checked {
            self.selected_items.insert(item_id);
        } else {
            self.selected_items.remove(&item_id);
        }
        cx.notify();
    }

    fn select_all(&mut self, operation_id: String, item_ids: Vec<String>, cx: &mut Context<Self>) {
        self.selected_operation = Some(operation_id);
        self.selected_items = item_ids.into_iter().collect();
        self.selection_frozen = false;
        cx.notify();
    }

    fn freeze_selection(
        &mut self,
        operation_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_items.is_empty() {
            self.status = Some("Select at least one manifest item.".into());
            cx.notify();
            return;
        }
        let result = self.client.dispatch(ClientCommand::SelectReceiveItems {
            operation_id,
            item_ids: self.selected_items.iter().cloned().collect(),
        });
        match result {
            Ok(()) => {
                self.selection_frozen = true;
                self.status = Some("Selection validated. Start is now explicit.".into());
            }
            Err(error) => {
                self.selection_frozen = false;
                self.status = None;
                crate::toast::error("Confirm selection failed", error.to_string(), window, cx);
            }
        }
        cx.notify();
    }

    fn start_prepared(
        &mut self,
        operation_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selection_frozen {
            self.status = Some("Confirm the selected items before starting.".into());
            cx.notify();
            return;
        }
        match self
            .client
            .dispatch(ClientCommand::StartPreparedReceive { operation_id })
        {
            Ok(()) => self.status = Some("Download and verification started.".into()),
            Err(error) => {
                self.status = None;
                crate::toast::error("Start download failed", error.to_string(), window, cx);
            }
        }
        cx.notify();
    }

    fn open_note(
        &mut self,
        link: String,
        burn_on_read: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if burn_on_read {
            let entity = cx.entity();
            window.open_dialog(cx, move |dialog, _, _| {
                let entity = entity.clone();
                let link = link.clone();
                dialog
                    .title("Open burn-on-read note?")
                    .child("Opening this note may permanently remove it for everyone with access.")
                    .button_props(DialogButtonProps::default().ok_text("Open note"))
                    .on_ok(move |_, window, cx| {
                        entity.update(cx, |panel, cx| {
                            panel.dispatch_open_note(link.clone(), true, window, cx)
                        });
                        true
                    })
            });
        } else {
            self.dispatch_open_note(link, false, window, cx);
        }
    }

    fn dispatch_open_note(
        &mut self,
        link: String,
        burn_acknowledged: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let password = self.secret.read(cx).value().trim().to_owned();
        match self.client.dispatch(ClientCommand::OpenNote {
            link,
            password: (!password.is_empty()).then_some(password),
            burn_acknowledged,
        }) {
            Ok(()) => self.status = Some("Opening note...".into()),
            Err(error) => {
                self.status = None;
                crate::toast::error("Open note failed", error.to_string(), window, cx);
            }
        }
        cx.notify();
    }

    fn copy_note(&mut self, cx: &mut Context<Self>) {
        if let Some((_, text)) = &self.note_text {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            self.status = Some("Note copied to the clipboard.".into());
            cx.notify();
        }
    }

    fn save_note(&mut self, cx: &mut Context<Self>) {
        let Some((_, text)) = self.note_text.clone() else {
            return;
        };
        let receiver = cx.prompt_for_new_path(&self.home, Some("filebeam-note.txt"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let result = std::fs::write(path, text).map_err(|error| error.to_string());
            let _ = this.update_in(cx, |panel, window, cx| {
                panel.status = match result {
                    Ok(()) => Some("Note saved to the selected file.".into()),
                    Err(error) => {
                        crate::toast::error("Save note failed", error, window, cx);
                        None
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn retry_burn(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.client.dispatch(ClientCommand::RetryBurn { id }) {
            crate::toast::error("Retry burn failed", error.to_string(), window, cx);
        }
        cx.notify();
    }

    fn answer_prompt(
        &mut self,
        transfer_id: String,
        prompt_id: u64,
        answer: PromptAnswer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.client.dispatch(ClientCommand::AnswerPrompt {
            transfer_id,
            prompt_id,
            answer,
        }) {
            crate::toast::error("Answer prompt failed", error.to_string(), window, cx);
        }
        cx.notify();
    }

    fn open_prompt(
        &mut self,
        transfer_id: String,
        prompt_id: u64,
        kind: PromptKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        let cancel_entity = entity.clone();
        let cancel_transfer = transfer_id.clone();
        let secret = self.secret.clone();
        let title = match kind {
            PromptKind::ShareKey => "Share key required",
            PromptKind::Password => "Password required",
            PromptKind::PeerConsent => "Allow peer connection?",
            PromptKind::Directory => "Directory transfer",
            PromptKind::ShareReady => "Sender is ready",
        };
        window.open_dialog(cx, move |dialog, _, _| {
            let entity = entity.clone();
            let transfer_id = transfer_id.clone();
            let cancel_entity = cancel_entity.clone();
            let cancel_transfer = cancel_transfer.clone();
            dialog
                .title(title)
                .when(
                    matches!(kind, PromptKind::ShareKey | PromptKind::Password),
                    |dialog| dialog.child(Input::new(&secret)),
                )
                .child(match kind {
                    PromptKind::PeerConsent => "A direct connection can expose your network address to the other participant. Both devices need to remain online.",
                    PromptKind::Directory => "Download the directory as a ZIP archive.",
                    PromptKind::ShareReady => "Confirm that you want to start receiving.",
                    PromptKind::ShareKey | PromptKind::Password => "Used to unlock this transfer.",
                })
                .button_props(DialogButtonProps::default().ok_text("Continue"))
                .on_ok(move |_, window, cx| {
                    entity
                        .update(cx, |panel, cx| {
                            let answer = match kind {
                                PromptKind::ShareKey | PromptKind::Password => {
                                    PromptAnswer::Secret(panel.secret.read(cx).value().to_string())
                                }
                                PromptKind::PeerConsent | PromptKind::ShareReady => {
                                    PromptAnswer::AllowPeer(true)
                                }
                                PromptKind::Directory => {
                                    PromptAnswer::Directory(crate::model::DirectoryMode::Zip)
                                }
                            };
                            panel.answer_prompt(transfer_id.clone(), prompt_id, answer, window, cx);
                        });
                    true
                })
                .on_cancel(move |_, window, cx| {
                    if kind == PromptKind::PeerConsent {
                        cancel_entity.update(cx, |panel, cx| {
                            panel.answer_prompt(cancel_transfer.clone(), prompt_id, PromptAnswer::AllowPeer(false), window, cx);
                        });
                    }
                    true
                })
        });
    }

    fn export_verified(&mut self, transfer_id: String, cx: &mut Context<Self>) {
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
            let _ = this.update_in(cx, |panel, window, cx| {
                match panel.client.dispatch(ClientCommand::ExportVerified {
                    transfer_id,
                    destination,
                }) {
                    Ok(()) => panel.status = Some("Verified export submitted.".into()),
                    Err(error) => {
                        panel.status = None;
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

    fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = palette_for(Theme::global(cx).mode);
        let validation = self.validation(cx);
        let receive = cx.listener(Self::inspect);
        let paste = cx.listener(Self::paste);
        div().w_full().flex().flex_col().gap(px(crate::views::page::HEADER_GAP))
            .child(crate::views::page::heading(
                p,
                "Receive",
                "Open a transfer from its link or ID.",
            ))
            .child(
                div()
                    .w_full()
                    .max_w(px(797.))
                    .rounded(px(18.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .overflow_hidden()
                    .child(
                        div()
                            .p(px(36.))
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(14.))
                                    .child(
                                        div()
                                            .size(px(48.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(12.))
                                            .bg(p.selected)
                                            .text_color(p.accent)
                                            .child(
                                                Icon::default()
                                                    .path("icons/download.svg")
                                                    .size(px(22.)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .child(div().text_size(px(18.)).child("Receive a transfer"))
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(p.muted)
                                                    .child("Files and notes, from any Filebeam client."),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.muted)
                                    .child("Paste a complete Filebeam link or enter a transfer ID. Full links retain their origin and key fragment."),
                            )
                            .child(div().text_size(px(11.)).text_color(p.muted).child("TRANSFER LINK OR ID"))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        Input::new(&self.link)
                                            .flex_1()
                                            .min_h(px(36.))
                                            .with_size(Size::Large),
                                    )
                                    .child(
                                        Button::new("paste-link")
                                            .label("Paste")
                                            .h(px(36.))
                                            .min_h(px(36.))
                                            .flex_none()
                                            .with_size(Size::Large)
                                            .on_click(paste),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(if validation.is_ok() { p.muted } else { p.subtle })
                                    .child(validation.clone().unwrap_or_else(|error| error)),
                            )
                            .child(
                                div()
                                    .py(px(12.))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .border_t_1()
                                    .border_color(p.border_soft)
                                    .child(
                                        Icon::default()
                                            .path("icons/key.svg")
                                            .size(px(15.))
                                            .text_color(p.subtle),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .child("Have a separately shared key?"),
                                            )
                                            .child(
                                                div()
                                                    .mt(px(3.))
                                                    .text_size(px(11.))
                                                    .text_color(p.subtle)
                                                    .child("Enter it only when Filebeam requests it."),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .p(px(12.))
                                    .rounded(px(9.))
                                    .bg(p.raised)
                                    .border_1()
                                    .border_color(p.border)
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child("Links stay intact, including the decryption key. You choose when to start."),
                            ),
                    )
                    .child(
                        div()
                            .h(px(70.))
                            .px(px(36.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .bg(p.raised)
                            .border_t_1()
                            .border_color(p.border_soft)
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child("Verify first. Save when ready."),
                            )
                            .child(
                                Button::new("download-verify")
                                    .label("Download and verify")
                                    .h(px(36.))
                                    .min_h(px(36.))
                                    .flex_none()
                                    .with_size(Size::Large)
                                    .primary()
                                    .on_click(receive),
                            ),
                        ),
                    )
            .when_some(self.status.clone(), |this, status| this.child(div().text_size(px(12.)).text_color(p.muted).child(status)))
            .child(self.jobs(p))
            .child(self.previews(window, cx, p))
            .child(self.verified(cx, p))
            .child(self.note_viewer(cx, p))
            .child(self.prompts(window, cx, p))
    }

    fn previews(&self, _: &mut Window, cx: &mut Context<Self>, p: Palette) -> impl IntoElement {
        div()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(
                self.snapshot
                    .receive_previews
                    .iter()
                    .map(|preview| match preview.kind {
                        ReceiveKind::File => {
                            let Some(operation_id) = preview.operation_id.clone() else {
                                return div().child("Preparing manifest...").into_any_element();
                            };
                            let all_ids = preview
                                .items
                                .iter()
                                .map(|item| item.id.clone())
                                .collect::<Vec<_>>();
                            let select_operation_id = operation_id.clone();
                            let select_all = cx.listener(move |panel, _, _, cx| {
                                panel.select_all(select_operation_id.clone(), all_ids.clone(), cx)
                            });
                            let freeze_id = preview.operation_id.clone().unwrap();
                            let freeze = cx.listener(move |panel, _, window, cx| {
                                panel.freeze_selection(freeze_id.clone(), window, cx)
                            });
                            let start_id = preview.operation_id.clone().unwrap();
                            let start = cx.listener(move |panel, _, window, cx| {
                                panel.start_prepared(start_id.clone(), window, cx)
                            });
                            div()
                                .p(px(12.))
                                .rounded(px(10.))
                                .bg(p.surface)
                                .border_1()
                                .border_color(p.border)
                                .child(div().child("Validated manifest"))
                                .child(
                                    Button::new(receive_element_id("select-all", &operation_id))
                                        .label("Select all")
                                        .ghost()
                                        .on_click(select_all),
                                )
                                .children(preview.items.iter().map(|item| {
                                    let item_id = item.id.clone();
                                    let operation_id = preview.operation_id.clone().unwrap();
                                    let item_element_id = receive_element_id(
                                        "receive-item",
                                        &format!("{operation_id}-{}", item.id),
                                    );
                                    let checked = self.selected_operation.as_deref()
                                        == Some(&operation_id)
                                        && self.selected_items.contains(&item_id);
                                    let toggle = cx.listener(move |panel, checked, _, cx| {
                                        panel.select_item(
                                            operation_id.clone(),
                                            item_id.clone(),
                                            *checked,
                                            cx,
                                        )
                                    });
                                    Checkbox::new(item_element_id)
                                        .label(format!("{} ({} bytes)", item.name, item.size))
                                        .checked(checked)
                                        .on_click(toggle)
                                }))
                                .child(
                                    Button::new(receive_element_id(
                                        "confirm-selection",
                                        &operation_id,
                                    ))
                                    .label("Confirm selection")
                                    .primary()
                                    .on_click(freeze),
                                )
                                .child(
                                    Button::new(receive_element_id(
                                        "start-prepared",
                                        &operation_id,
                                    ))
                                    .label("Start download and verify")
                                    .primary()
                                    .on_click(start),
                                )
                                .into_any_element()
                        }
                        ReceiveKind::Note => {
                            let link = preview.link.clone();
                            let validated_id =
                                validated_preview_id(&link, &self.snapshot.instance.url);
                            let burn = preview.burn_on_read;
                            let open = cx.listener(move |panel, _, window, cx| {
                                panel.open_note(link.clone(), burn, window, cx)
                            });
                            div()
                                .p(px(12.))
                                .rounded(px(10.))
                                .bg(p.surface)
                                .border_1()
                                .border_color(p.border)
                                .child(div().child("Validated note preview"))
                                .child(if preview.password_required {
                                    Input::new(&self.secret).into_any_element()
                                } else {
                                    div().into_any_element()
                                })
                                .child(
                                    Button::new(receive_element_id("open-note", &validated_id))
                                        .label("Open note")
                                        .primary()
                                        .on_click(open),
                                )
                                .into_any_element()
                        }
                    }),
            )
    }

    fn note_viewer(&self, cx: &mut Context<Self>, p: Palette) -> impl IntoElement {
        let Some((id, text)) = &self.note_text else {
            return div().into_any_element();
        };
        let copy = cx.listener(|panel, _, _, cx| panel.copy_note(cx));
        let retry_id = id.clone();
        let retry =
            cx.listener(move |panel, _, window, cx| panel.retry_burn(retry_id.clone(), window, cx));
        let pending_burn = self
            .snapshot
            .notes
            .iter()
            .find(|note| note.id == *id)
            .is_some_and(|note| note.retry_burn);
        div()
            .max_w(px(680.))
            .p(px(12.))
            .rounded(px(10.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(div().child("Opened note"))
            .child(
                div()
                    .mt(px(8.))
                    .p(px(10.))
                    .bg(p.raised)
                    .text_size(px(12.))
                    .child(text.clone()),
            )
            .child(
                div()
                    .mt(px(8.))
                    .flex()
                    .gap(px(8.))
                    .child(Button::new("copy-opened-note").label("Copy").on_click(copy))
                    .child(
                        Button::new("save-opened-note")
                            .label("Save note")
                            .on_click(cx.listener(|panel, _, _, cx| panel.save_note(cx))),
                    ),
            )
            .when(pending_burn, |this| {
                this.child(
                    Button::new(receive_element_id("retry-burn", id))
                        .label("Retry burn")
                        .primary()
                        .on_click(retry),
                )
            })
            .into_any_element()
    }

    fn verified(&self, cx: &mut Context<Self>, p: Palette) -> impl IntoElement {
        div()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(self.snapshot.verified_results.iter().map(|result| {
                let transfer_id = result.transfer_id.clone();
                let export = cx.listener(move |panel, _, _, cx| {
                    panel.export_verified(transfer_id.clone(), cx)
                });
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .child(div().child(if result.exported_paths.is_empty() {
                        "Verified on this device"
                    } else {
                        "Saved to folder"
                    }))
                    .child(
                        div().text_size(px(11.)).text_color(p.muted).child(
                            result
                                .export_error
                                .as_ref()
                                .map(|error| error.detail.clone())
                                .unwrap_or_else(|| "Private download is ready to export.".into()),
                        ),
                    )
                    .when(
                        result.exported_paths.is_empty() || result.export_error.is_some(),
                        |this| {
                            this.child(
                                Button::new(SharedString::from(format!(
                                    "export-{}",
                                    result.transfer_id
                                )))
                                .label("Save to folder")
                                .primary()
                                .on_click(export),
                            )
                        },
                    )
            }))
    }

    fn jobs(&self, p: Palette) -> impl IntoElement {
        let received = self
            .snapshot
            .jobs
            .iter()
            .filter(|job| job.direction == TransferDirection::Receive);
        div()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(received.map(|job| {
                let total = job
                    .progress
                    .total_bytes
                    .map(|total| format!("{} / {} bytes", job.progress.completed_bytes, total))
                    .unwrap_or_else(|| format!("{} bytes", job.progress.completed_bytes));
                let state = match job.state {
                    TransferState::Complete => "Verified",
                    TransferState::Failed => "Failed",
                    TransferState::Paused => "Paused",
                    TransferState::PauseRequested => "Pausing",
                    TransferState::Running => "Downloading",
                };
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .child(div().child(format!("{state}: {}", job.progress.name)))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(format!("{} ({total})", job.progress.phase)),
                    )
            }))
    }

    fn prompts(&self, _: &mut Window, cx: &mut Context<Self>, p: Palette) -> impl IntoElement {
        div()
            .max_w(px(680.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(self.snapshot.pending_prompts.iter().map(|prompt| {
                let transfer_id = prompt.transfer_id.clone();
                let prompt_id = prompt.id;
                let kind = prompt.kind;
                let label = match kind {
                    PromptKind::ShareKey => "Share key required",
                    PromptKind::Password => "Password required",
                    PromptKind::PeerConsent => "Peer consent required",
                    PromptKind::Directory => "Directory choice required",
                    PromptKind::ShareReady => "Waiting for sender",
                };
                let answer = cx.listener(move |this, _, window, cx| {
                    this.open_prompt(transfer_id.clone(), prompt_id, kind, window, cx)
                });
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(p.raised)
                    .child(div().text_size(px(12.)).child(label))
                    .child(
                        Button::new(("prompt", prompt_id))
                            .label("Continue")
                            .primary()
                            .on_click(answer),
                    )
            }))
    }
}

impl Render for ReceivePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette_for(Theme::global(cx).mode);
        let content = self.render_content(window, cx);
        crate::views::page::scroller(
            "receive-page-scroll",
            &self.page_scroll,
            window,
            div()
                .bg(p.page)
                .text_color(p.text)
                .child(div().flex_none().child(content)),
        )
    }
}

fn receive_element_id(prefix: &str, non_secret_id: &str) -> SharedString {
    SharedString::from(format!("{prefix}-{non_secret_id}"))
}

fn validated_preview_id(link: &str, instance: &str) -> String {
    // Previews are created only after link validation; use its public transfer ID, never its key.
    filebeam_transfer_native::protocol::parse_link_for_instance(link, instance)
        .expect("receive preview must contain a validated link")
        .id
}

fn snapshot_version(snapshot: &DesktopSnapshot) -> u64 {
    let mut version = 0xcbf29ce484222325_u64;
    for job in &snapshot.jobs {
        fold(&mut version, &job.id);
        fold(&mut version, &job.progress.phase);
        fold(&mut version, &job.progress.name);
        fold(&mut version, job.progress.completed_bytes.to_le_bytes());
        fold(
            &mut version,
            [match job.state {
                TransferState::Running => 0,
                TransferState::PauseRequested => 1,
                TransferState::Paused => 2,
                TransferState::Complete => 3,
                TransferState::Failed => 4,
            }],
        );
        fold(&mut version, [job.error.is_some() as u8]);
    }
    for prompt in &snapshot.pending_prompts {
        fold(&mut version, &prompt.transfer_id);
        fold(&mut version, prompt.id.to_le_bytes());
        fold(
            &mut version,
            [match prompt.kind {
                PromptKind::ShareKey => 0,
                PromptKind::Password => 1,
                PromptKind::PeerConsent => 2,
                PromptKind::Directory => 3,
                PromptKind::ShareReady => 4,
            }],
        );
    }
    for preview in &snapshot.receive_previews {
        fold(&mut version, preview.operation_id.as_deref().unwrap_or(""));
        fold(
            &mut version,
            [
                match preview.kind {
                    ReceiveKind::File => 0,
                    ReceiveKind::Note => 1,
                },
                preview.password_required as u8,
                preview.burn_on_read as u8,
            ],
        );
        fold(&mut version, (preview.items.len() as u64).to_le_bytes());
        for item in &preview.items {
            fold(&mut version, &item.id);
            fold(&mut version, &item.name);
            fold(&mut version, item.size.to_le_bytes());
        }
    }
    for result in &snapshot.verified_results {
        fold(&mut version, &result.transfer_id);
        fold(
            &mut version,
            (result.exported_paths.len() as u64).to_le_bytes(),
        );
        fold(&mut version, [result.export_error.is_some() as u8]);
    }
    for note in &snapshot.notes {
        fold(&mut version, &note.id);
        fold(
            &mut version,
            [
                note.text_available as u8,
                note.retry_burn as u8,
                note.burn_on_read as u8,
            ],
        );
    }
    version
}

fn fold(version: &mut u64, value: impl AsRef<[u8]>) {
    for byte in value.as_ref() {
        *version = version.wrapping_mul(0x100000001b3) ^ *byte as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::snapshot_version;
    use crate::model::{DesktopSnapshot, PendingPrompt, PromptKind};

    #[test]
    fn canonical_parser_keeps_explicit_origin_and_fragment_key() {
        let raw = "https://receive.example/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let parsed = filebeam_transfer_native::protocol::parse_link_for_instance(
            raw,
            "https://default.example",
        )
        .unwrap();
        assert_eq!(parsed.instance, "https://receive.example");
        assert_eq!(parsed.key, Some(vec![0; 32]));
    }

    #[test]
    fn snapshot_fingerprint_observes_prompt_identity_not_only_progress() {
        let mut before = DesktopSnapshot::default();
        before.pending_prompts.push(PendingPrompt {
            transfer_id: "receive-a".into(),
            id: 1,
            kind: PromptKind::Password,
            peer: None,
            directory: None,
        });
        let mut after = before.clone();
        after.pending_prompts[0].id = 2;
        assert_ne!(snapshot_version(&before), snapshot_version(&after));
    }
}
