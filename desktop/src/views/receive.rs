//! The Receive flow: open a link, choose what to download, verify, then save.
//! Transfer data remains in the service worker; this view only presents its state.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use gpui::{
    AnyElement, AppContext, ClickEvent, ClipboardItem, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, PathPromptOptions, Render, ScrollHandle, SharedString, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    input::{InputEvent, InputState},
    scroll::ScrollableElement,
};

use crate::{
    assets::icon,
    client::DesktopClient,
    live::Live,
    model::{
        ClientCommand, DesktopSnapshot, DirectoryMode, JobSnapshot, PendingPrompt, PromptAnswer,
        PromptKind, ReceiveKind, ReceivePreview, TransferDirection, TransferState, VerifiedResult,
    },
    theme::{
        self, Palette,
        tokens::{geometry, radius},
    },
    ui::{
        MotionExt, TextExt, Tone, button, dialog,
        feedback::{self, spinner},
        field, format, motion, page,
        progress::{percent_label, progress},
        row, surface, text,
    },
};

/// The request to open a link, until the service answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Inspecting {
    link: String,
    after_message: u64,
    known_jobs: HashSet<String>,
}

/// A download whose selection is being applied before it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Starting {
    operation_id: String,
    after_operation: usize,
    after_message: u64,
}

/// Native receive form and its local presentation state.
pub struct ReceivePanel {
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    home: PathBuf,
    link: Entity<InputState>,
    secret: Entity<InputState>,
    status: Option<String>,
    error: Option<String>,
    inspecting: Option<Inspecting>,
    starting: Option<Starting>,
    /// Chosen items per prepared download. A download starts with everything chosen.
    selections: HashMap<String, HashSet<String>>,
    note_text: Option<(String, String)>,
    consumed_notes: HashSet<String>,
    copied_note: bool,
    page_scroll: ScrollHandle,
}

impl ReceivePanel {
    pub fn new(
        live: Entity<Live>,
        home: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let client = live.read(cx).client().clone();
        let link =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://… or a transfer ID"));
        cx.subscribe_in(
            &link,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.error = None;
                    this.status = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.open(window, cx),
                _ => {}
            },
        )
        .detach();
        cx.observe(&live, |this, live, cx| {
            let snapshot = live.read(cx).snapshot();
            this.on_snapshot(&snapshot, cx);
        })
        .detach();
        let mut panel = Self {
            client,
            live,
            home,
            link,
            secret: cx.new(|cx| InputState::new(window, cx).masked(true)),
            status: None,
            error: None,
            inspecting: None,
            starting: None,
            selections: HashMap::new(),
            note_text: None,
            consumed_notes: HashSet::new(),
            copied_note: false,
            page_scroll: ScrollHandle::new(),
        };
        let snapshot = panel.live.read(cx).snapshot();
        panel.on_snapshot(&snapshot, cx);
        panel
    }

    /// Replaces the draft without rewriting a full URL to the configured origin.
    pub fn set_link(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        self.link
            .update(cx, |input, cx| input.set_value(link, window, cx));
        self.status = None;
        self.error = None;
        cx.notify();
    }

    /// Reconciles local state with the service: opened notes are collected once,
    /// new downloads start fully selected, and pending requests are resolved.
    fn on_snapshot(&mut self, snapshot: &DesktopSnapshot, cx: &mut Context<Self>) {
        for note in snapshot.notes.iter().filter(|note| note.text_available) {
            if self.consumed_notes.insert(note.id.clone())
                && let Some(text) = self.client.take_opened_note(&note.id)
            {
                self.note_text = Some((note.id.clone(), text));
                self.status = None;
            }
        }
        for preview in &snapshot.receive_previews {
            if let Some(operation) = &preview.operation_id {
                self.selections
                    .entry(operation.clone())
                    .or_insert_with(|| preview.items.iter().map(|item| item.id.clone()).collect());
            }
        }
        self.selections.retain(|operation, _| {
            snapshot
                .receive_previews
                .iter()
                .any(|preview| preview.operation_id.as_deref() == Some(operation))
        });

        if let Some(inspecting) = &self.inspecting {
            let answered = snapshot
                .receive_previews
                .iter()
                .any(|preview| preview.link == inspecting.link);
            let refused = snapshot
                .messages
                .iter()
                .find(|message| {
                    message.id > inspecting.after_message && message.operation == "inspect receive"
                })
                .map(|message| message.error.detail.clone())
                .or_else(|| {
                    // Preparing a file manifest runs as a service job, which reports
                    // its own failure.
                    snapshot
                        .jobs
                        .iter()
                        .find(|job| {
                            job.direction == TransferDirection::Service
                                && job.state == TransferState::Failed
                                && !inspecting.known_jobs.contains(&job.id)
                        })
                        .map(|job| {
                            job.error.as_ref().map_or_else(
                                || "The transfer could not be opened.".to_owned(),
                                |error| error.detail.clone(),
                            )
                        })
                });
            if answered {
                self.inspecting = None;
            } else if let Some(detail) = refused {
                self.inspecting = None;
                self.error = Some(format::readable_error(&detail));
            }
        }

        if let Some(starting) = self.starting.clone() {
            let accepted = snapshot
                .operations
                .iter()
                .skip(starting.after_operation)
                .any(|operation| operation.operation == "select receive items");
            let refused = snapshot.messages.iter().find(|message| {
                message.id > starting.after_message
                    && matches!(
                        message.operation.as_str(),
                        "select receive items" | "start prepared receive"
                    )
            });
            if let Some(message) = refused {
                self.starting = None;
                self.error = Some(format::readable_error(&message.error.detail));
            } else if accepted {
                self.starting = None;
                if let Err(error) = self.client.dispatch(ClientCommand::StartPreparedReceive {
                    operation_id: starting.operation_id,
                }) {
                    self.error = Some(format::readable_error(&error.to_string()));
                }
            }
        }
        cx.notify();
    }

    fn draft(&self, cx: &Context<Self>) -> String {
        self.link.read(cx).value().trim().to_owned()
    }

    /// The instance a draft resolves to, or why it cannot be opened.
    fn validation(&self, cx: &Context<Self>) -> Result<String, String> {
        let raw = self.draft(cx);
        if raw.is_empty() {
            return Err("Enter a transfer link or ID.".into());
        }
        let configured = self.live.read(cx).snapshot().instance.url.clone();
        filebeam_transfer_native::protocol::parse_link_for_instance(&raw, &configured)
            .map(|parsed| parsed.instance)
            .map_err(|error| format::readable_error(&error.to_string()))
    }

    fn paste(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        match cx.read_from_clipboard().and_then(|item| item.text()) {
            Some(text) if !text.trim().is_empty() => {
                self.set_link(text.trim().to_owned(), window, cx);
            }
            _ => {
                self.error = Some("The clipboard does not contain text.".into());
                cx.notify();
            }
        }
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inspecting.is_some() {
            return;
        }
        if let Err(error) = self.validation(cx) {
            self.error = Some(error);
            cx.notify();
            return;
        }
        let link = self.draft(cx);
        let snapshot = self.client.snapshot();
        match self
            .client
            .dispatch(ClientCommand::InspectReceive { link: link.clone() })
        {
            Ok(()) => {
                self.error = None;
                self.inspecting = Some(Inspecting {
                    link,
                    after_message: snapshot.messages.last().map_or(0, |message| message.id),
                    known_jobs: snapshot.jobs.iter().map(|job| job.id.clone()).collect(),
                });
            }
            Err(error) => {
                crate::toast::error(
                    "Could not open the transfer",
                    format::readable_error(&error.to_string()),
                    window,
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn toggle_item(&mut self, operation: &str, item: &str, checked: bool, cx: &mut Context<Self>) {
        let selection = self.selections.entry(operation.to_owned()).or_default();
        if checked {
            selection.insert(item.to_owned());
        } else {
            selection.remove(item);
        }
        self.error = None;
        cx.notify();
    }

    fn select_all(&mut self, operation: &str, items: Vec<String>, cx: &mut Context<Self>) {
        self.selections
            .insert(operation.to_owned(), items.into_iter().collect());
        cx.notify();
    }

    /// Applies the selection, then starts once the service has accepted it.
    fn download(&mut self, operation_id: String, cx: &mut Context<Self>) {
        let Some(selection) = self.selections.get(&operation_id) else {
            return;
        };
        if selection.is_empty() {
            self.error = Some("Choose at least one file to download.".into());
            cx.notify();
            return;
        }
        let snapshot = self.client.snapshot();
        match self.client.dispatch(ClientCommand::SelectReceiveItems {
            operation_id: operation_id.clone(),
            item_ids: selection.iter().cloned().collect(),
        }) {
            Ok(()) => {
                self.error = None;
                self.starting = Some(Starting {
                    operation_id,
                    after_operation: snapshot.operations.len(),
                    after_message: snapshot.messages.last().map_or(0, |message| message.id),
                });
            }
            Err(error) => self.error = Some(format::readable_error(&error.to_string())),
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
            dialog::prompt("Open this note once?")
                .body(
                    "This note burns after reading. Opening it removes it for everyone with \
                     the link, so copy or save what you need before closing it.",
                )
                .confirm("Open note")
                .open(window, cx, move |window, cx| {
                    entity.update(cx, |panel, cx| {
                        panel.dispatch_open_note(link.clone(), true, window, cx)
                    });
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
        let password = self.secret.read(cx).value().to_string();
        match self.client.dispatch(ClientCommand::OpenNote {
            link,
            password: (!password.is_empty()).then_some(password),
            burn_acknowledged,
        }) {
            Ok(()) => self.status = Some("Decrypting the note…".into()),
            Err(error) => {
                self.status = None;
                crate::toast::error(
                    "Could not open the note",
                    format::readable_error(&error.to_string()),
                    window,
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied_note = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copied_note = false;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn save_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_new_path(&self.home, Some("filebeam-note.txt"));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(path, text).map_err(|error| error.to_string()) })
                .await;
            let _ = this.update_in(cx, |panel, window, cx| {
                panel.status = match result {
                    Ok(()) => Some("Note saved.".into()),
                    Err(error) => {
                        crate::toast::error("Could not save the note", error, window, cx);
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
            crate::toast::error(
                "Could not remove the note",
                format::readable_error(&error.to_string()),
                window,
                cx,
            );
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
            crate::toast::error(
                "Could not continue",
                format::readable_error(&error.to_string()),
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Asks for the key or password a transfer needs. Secrets are typed into a
    /// dialog and passed straight to the service.
    fn ask_secret(
        &mut self,
        transfer_id: String,
        prompt_id: u64,
        kind: PromptKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        let secret = self.secret.clone();
        secret.update(cx, |input, cx| input.set_value("", window, cx));
        let (title, detail) = if kind == PromptKind::ShareKey {
            (
                "Enter the decryption key",
                "This link was shared without its key. Enter the key the sender gave you separately.",
            )
        } else {
            (
                "Enter the password",
                "The sender protected this transfer with a password.",
            )
        };
        let (shown, discarded) = (secret.clone(), secret);
        dialog::prompt(title).body(detail).open_with(
            window,
            cx,
            move |p, _, _| Some(field::secret(p, &shown).into_any_element()),
            move |window, cx| {
                entity.update(cx, |panel, cx| {
                    let value = panel.secret.read(cx).value().to_string();
                    if value.is_empty() {
                        return false;
                    }
                    panel.answer_prompt(
                        transfer_id.clone(),
                        prompt_id,
                        PromptAnswer::Secret(value),
                        window,
                        cx,
                    );
                    panel
                        .secret
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    true
                })
            },
            // A dismissed prompt leaves nothing typed behind.
            move |window, cx| discarded.update(cx, |input, cx| input.set_value("", window, cx)),
        );
        self.secret.update(cx, |input, cx| input.focus(window, cx));
    }

    fn export_verified(
        &mut self,
        transfer_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            let _ = this.update_in(cx, |panel, window, cx| {
                if let Err(error) = panel.client.dispatch(ClientCommand::ExportVerified {
                    transfer_id,
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
}

#[cfg(feature = "visual-test")]
impl ReceivePanel {
    /// Seeds the link field and an optional inline failure for a capture.
    pub fn seed_for_capture(
        &mut self,
        link: &str,
        error: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_link(link.to_owned(), window, cx);
        self.error = error.map(str::to_owned);
        cx.notify();
    }

    pub fn ask_secret_for_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_secret(
            "receive-fixture-01".into(),
            7,
            PromptKind::Password,
            window,
            cx,
        );
    }

    pub fn seed_note_for_capture(&mut self, id: &str, text: &str, cx: &mut Context<Self>) {
        self.note_text = Some((id.to_owned(), text.to_owned()));
        cx.notify();
    }
}

impl Render for ReceivePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let snapshot = self.live.read(cx).snapshot();
        let mut sections: Vec<AnyElement> = vec![self.link_card(p, cx)];
        if let Some((id, text)) = self.note_text.clone() {
            sections.push(self.note_viewer(&snapshot, id, text, p, cx));
        }
        for preview in &snapshot.receive_previews {
            sections.push(match preview.kind {
                ReceiveKind::File => self.files_preview(preview, p, cx),
                ReceiveKind::Note => self.note_preview(preview, p, cx),
            });
        }
        for job in snapshot
            .jobs
            .iter()
            .filter(|job| job.direction == TransferDirection::Receive)
        {
            let verified = snapshot
                .verified_results
                .iter()
                .find(|result| result.transfer_id == job.id);
            let prompt = snapshot
                .pending_prompts
                .iter()
                .find(|prompt| prompt.transfer_id == job.id);
            sections.push(self.job_card(job, verified, prompt, p, cx));
        }
        // A prompt can precede its job in the snapshot; never leave one unanswerable.
        for prompt in snapshot.pending_prompts.iter().filter(|prompt| {
            !snapshot.jobs.iter().any(|job| {
                job.direction == TransferDirection::Receive && job.id == prompt.transfer_id
            })
        }) {
            sections.push(
                surface::panel(p)
                    .p(px(20.))
                    .child(self.prompt_block(prompt, p, cx))
                    .into_any_element(),
            );
        }

        let header = page::header(
            p,
            "Receive",
            "Open a transfer from its link or ID. Files are decrypted on this device.",
            window,
            cx,
        );
        let sections = sections
            .into_iter()
            .enumerate()
            .map(|(index, section)| {
                let entrance = motion::entrance(
                    SharedString::from(format!("receive-section-{index}")),
                    index + 1,
                    window,
                    cx,
                );
                div().w_full().entering(entrance).child(section)
            })
            .collect::<Vec<_>>();
        page::scroller(
            "receive-page-scroll",
            &self.page_scroll,
            geometry::CONTENT_READING,
            false,
            window,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(header)
                .children(sections),
        )
    }
}

impl ReceivePanel {
    fn link_card(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let draft_empty = self.draft(cx).is_empty();
        let validation = self.validation(cx);
        let busy = self.inspecting.is_some();
        let hint = match (&self.error, draft_empty, &validation) {
            (Some(error), _, _) => feedback::notice(p, error.clone(), Tone::Danger),
            (None, true, _) => div().child(field::hint(
                p,
                "Paste a complete Filebeam link, or a transfer ID from this instance. \
                 The key in a link never leaves this device.",
                Tone::Neutral,
            )),
            (None, false, Ok(instance)) => div().child(field::hint(
                p,
                format!("Opens from {}", format::host(instance)),
                Tone::Neutral,
            )),
            (None, false, Err(error)) => div().child(field::hint(p, error.clone(), Tone::Danger)),
        };
        surface::panel(p)
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(field::label(p, "Transfer link or ID", false))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div().flex_1().min_w_0().child(
                            field::input(p, &self.link)
                                .id("receive-link")
                                .prefix(icon("link").size(px(16.)).text_color(p.text_subtle))
                                .font_family(text::MONO),
                        ),
                    )
                    .child(
                        button::secondary("paste-link")
                            .icon("copy")
                            .label("Paste")
                            .on_click(cx.listener(Self::paste)),
                    )
                    .child(
                        button::primary("open-transfer")
                            .icon("unlock")
                            .label("Open transfer")
                            .loading(busy)
                            .disabled(draft_empty)
                            .on_click(cx.listener(|this, _, window, cx| this.open(window, cx))),
                    ),
            )
            .child(hint)
            .when(busy, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(spinner(p.accent_text, 16.))
                        .child(
                            text::small("Reading the encrypted manifest…").text_color(p.text_muted),
                        ),
                )
            })
            .when_some(self.status.clone(), |this, status| {
                this.child(text::small(status).text_color(p.text_muted))
            })
            .into_any_element()
    }

    fn files_preview(
        &mut self,
        preview: &ReceivePreview,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(operation) = preview.operation_id.clone() else {
            return surface::panel(p)
                .p(px(20.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(spinner(p.accent_text, 16.))
                .child(text::small("Preparing the file list…").text_color(p.text_muted))
                .into_any_element();
        };
        let selection = self.selections.get(&operation).cloned().unwrap_or_default();
        let all_ids: Vec<String> = preview.items.iter().map(|item| item.id.clone()).collect();
        let chosen: Vec<_> = preview
            .items
            .iter()
            .filter(|item| selection.contains(&item.id))
            .collect();
        let chosen_size: u64 = chosen.iter().map(|item| item.size).sum();
        let everything = chosen.len() == preview.items.len();
        let starting = self
            .starting
            .as_ref()
            .is_some_and(|starting| starting.operation_id == operation);

        let rows = preview
            .items
            .iter()
            .map(|item| {
                let checked = selection.contains(&item.id);
                let (operation, item_id) = (operation.clone(), item.id.clone());
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(radius::CONTROL))
                    .hover(move |style| style.bg(p.hover))
                    .child(
                        field::check(
                            SharedString::from(format!("receive-item-{operation}-{}", item.id)),
                            format!("Download {}", item.name),
                            checked,
                        )
                        .on_change({
                            let panel = cx.entity().downgrade();
                            move |checked, _, cx| {
                                let _ = panel.update(cx, |panel, cx| {
                                    panel.toggle_item(&operation, &item_id, checked, cx);
                                });
                            }
                        }),
                    )
                    .child(row::icon_tile(
                        p,
                        format::file_icon(&item.name),
                        32.,
                        checked,
                    ))
                    .child(
                        text::body_strong(item.name.clone())
                            .flex_1()
                            .single_line()
                            .text_color(if checked { p.text } else { p.text_muted }),
                    )
                    .child(
                        text::small(format::bytes(item.size))
                            .flex_none()
                            .text_color(p.text_muted),
                    )
            })
            .collect::<Vec<_>>();

        let select_operation = operation.clone();
        let download_operation = operation.clone();
        let attached = preview.attached_note.clone().map(|note| {
            self.note_block(
                note.title.unwrap_or_else(|| "Attached note".into()),
                Some(note.language),
                note.text,
                p,
                cx,
            )
        });
        surface::panel(p)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(20.))
                    .pt(px(18.))
                    .pb(px(12.))
                    .child(row::icon_tile(p, "unlock", 40., true))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text::subtitle("Ready to download").text_color(p.text))
                            .child(
                                text::small(format!(
                                    "{} · {} in total",
                                    format::plural(preview.items.len() as u64, "file"),
                                    format::bytes(preview.items.iter().map(|item| item.size).sum())
                                ))
                                .text_color(p.text_muted),
                            ),
                    )
                    .child(
                        button::ghost(SharedString::from(format!("select-all-{operation}")))
                            .small()
                            .label(if everything {
                                "Select none"
                            } else {
                                "Select all"
                            })
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                let items = if everything {
                                    Vec::new()
                                } else {
                                    all_ids.clone()
                                };
                                panel.select_all(&select_operation, items, cx);
                            })),
                    ),
            )
            .children(attached.map(|note| div().px(px(20.)).pb(px(12.)).child(note)))
            .child(
                div()
                    .id(SharedString::from(format!("receive-items-{operation}")))
                    .max_h(px(280.))
                    .overflow_y_scrollbar()
                    .px(px(10.))
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .mt(px(8.))
                    .px(px(20.))
                    .py(px(14.))
                    .border_t_1()
                    .border_color(p.line_soft)
                    .child(
                        text::small(format!(
                            "{} of {} selected · {}",
                            chosen.len(),
                            preview.items.len(),
                            format::bytes(chosen_size)
                        ))
                        .text_color(p.text_muted),
                    )
                    .child(
                        button::primary(SharedString::from(format!("start-prepared-{operation}")))
                            .icon("download")
                            .label("Download and verify")
                            .loading(starting)
                            .disabled(chosen.is_empty())
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                panel.download(download_operation.clone(), cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    fn note_preview(
        &mut self,
        preview: &ReceivePreview,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let link = preview.link.clone();
        let burn = preview.burn_on_read;
        let id = filebeam_transfer_native::protocol::parse_link_for_instance(
            &link,
            &self.live.read(cx).snapshot().instance.url,
        )
        .map_or_else(|_| "note".to_owned(), |parsed| parsed.id);
        surface::panel(p)
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(row::icon_tile(p, "note", 40., true))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text::subtitle("Encrypted note").text_color(p.text))
                            .child(
                                text::small("It is decrypted on this device when you open it.")
                                    .text_color(p.text_muted),
                            ),
                    )
                    .when(burn, |this| {
                        this.child(feedback::pill_with_icon(
                            p,
                            "eye-off",
                            "Burns after reading",
                            Tone::Warning,
                        ))
                    }),
            )
            .when(preview.password_required, |this| {
                this.child(field::field(
                    p,
                    "Password",
                    field::secret(p, &self.secret).id("note-password"),
                ))
            })
            .child(
                div().flex().justify_end().child(
                    button::primary(SharedString::from(format!("open-note-{id}")))
                        .icon("unlock")
                        .label("Open note")
                        .on_click(cx.listener(move |panel, _, window, cx| {
                            panel.open_note(link.clone(), burn, window, cx)
                        })),
                ),
            )
            .into_any_element()
    }

    /// A decrypted note body with copy and save actions.
    fn note_block(
        &mut self,
        title: String,
        language: Option<String>,
        body: String,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (copy, save) = (body.clone(), body.clone());
        surface::well(p)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(p.line_soft)
                    .child(
                        icon("note")
                            .size(px(15.))
                            .flex_none()
                            .text_color(p.text_muted),
                    )
                    .child(
                        text::small_strong(title)
                            .flex_1()
                            .single_line()
                            .text_color(p.text),
                    )
                    .when_some(
                        language.filter(|language| language != "plain"),
                        |this, language| this.child(feedback::pill(p, language, Tone::Neutral)),
                    )
                    .child(
                        button::ghost(SharedString::from(format!("copy-note-{}", body.len())))
                            .small()
                            .icon(if self.copied_note { "check" } else { "copy" })
                            .label(if self.copied_note { "Copied" } else { "Copy" })
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                panel.copy_text(copy.clone(), cx)
                            })),
                    )
                    .child(
                        button::ghost(SharedString::from(format!("save-note-{}", body.len())))
                            .small()
                            .icon("download")
                            .label("Save")
                            .on_click(cx.listener(move |panel, _, window, cx| {
                                panel.save_text(save.clone(), window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("note-body-{}", body.len())))
                    .max_h(px(300.))
                    .overflow_y_scrollbar()
                    .p(px(14.))
                    .child(text::code(body).text_color(p.text)),
            )
            .into_any_element()
    }

    fn note_viewer(
        &mut self,
        snapshot: &DesktopSnapshot,
        id: String,
        body: String,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let note = snapshot.notes.iter().find(|note| note.id == id);
        let burned = note.is_some_and(|note| note.burn_on_read);
        let burn_pending = note.is_some_and(|note| note.retry_burn);
        let retry_id = id.clone();
        let contents = self.note_block("Contents".into(), None, body, p, cx);
        surface::panel(p)
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(row::icon_tile(p, "check", 40., true))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text::subtitle("Note opened").text_color(p.text))
                            .child(
                                text::small(if burned {
                                    "It has been removed for everyone else. Keep what you need."
                                } else {
                                    "Decrypted on this device."
                                })
                                .text_color(p.text_muted),
                            ),
                    ),
            )
            .child(contents)
            .when(burn_pending, |this| {
                this.child(
                    feedback::notice(
                        p,
                        "The note was opened, but removing it from the server did not finish.",
                        Tone::Warning,
                    )
                    .child(
                        button::secondary(SharedString::from(format!("retry-burn-{retry_id}")))
                            .small()
                            .label("Try again")
                            .on_click(cx.listener(move |panel, _, window, cx| {
                                panel.retry_burn(retry_id.clone(), window, cx)
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    fn prompt_block(
        &mut self,
        prompt: &PendingPrompt,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (transfer, id, kind) = (prompt.transfer_id.clone(), prompt.id, prompt.kind);
        let (name, title, detail): (&'static str, &'static str, String) = match kind {
            PromptKind::ShareKey => (
                "key",
                "A decryption key is needed",
                "This link was shared without its key.".into(),
            ),
            PromptKind::Password => (
                "lock",
                "A password is needed",
                "The sender protected this transfer with a password.".into(),
            ),
            PromptKind::PeerConsent => (
                "webrtc-p2p",
                "Connect directly to the sender?",
                "A direct connection shows your network address to the other person. \
                 Both devices need to stay online."
                    .into(),
            ),
            PromptKind::Directory => (
                "archive",
                "This transfer is a folder",
                prompt.directory.as_ref().map_or_else(
                    || "It downloads as one ZIP archive.".to_owned(),
                    |directory| {
                        format!(
                            "{}, {} in total. It downloads as one ZIP archive.",
                            format::plural(directory.files, "file"),
                            format::bytes(directory.bytes)
                        )
                    },
                ),
            ),
            PromptKind::ShareReady => (
                "check",
                "The sender is ready",
                "Start when you are ready to receive.".into(),
            ),
        };
        let answer = |answer: fn() -> PromptAnswer| {
            let transfer = transfer.clone();
            cx.listener(move |panel, _, window, cx| {
                panel.answer_prompt(transfer.clone(), id, answer(), window, cx)
            })
        };
        let actions = match kind {
            PromptKind::ShareKey | PromptKind::Password => {
                let transfer = transfer.clone();
                div().flex().gap(px(8.)).child(
                    button::primary(("prompt", id as usize))
                        .small()
                        .label(if kind == PromptKind::ShareKey {
                            "Enter key"
                        } else {
                            "Enter password"
                        })
                        .on_click(cx.listener(move |panel, _, window, cx| {
                            panel.ask_secret(transfer.clone(), id, kind, window, cx)
                        })),
                )
            }
            PromptKind::PeerConsent => div()
                .flex()
                .gap(px(8.))
                .child(
                    button::ghost(("prompt-decline", id as usize))
                        .small()
                        .label("Decline")
                        .on_click(answer(|| PromptAnswer::AllowPeer(false))),
                )
                .child(
                    button::primary(("prompt", id as usize))
                        .small()
                        .label("Allow")
                        .on_click(answer(|| PromptAnswer::AllowPeer(true))),
                ),
            PromptKind::Directory => div().flex().gap(px(8.)).child(
                button::primary(("prompt", id as usize))
                    .small()
                    .label("Continue")
                    .on_click(answer(|| PromptAnswer::Directory(DirectoryMode::Zip))),
            ),
            PromptKind::ShareReady => div().flex().gap(px(8.)).child(
                button::primary(("prompt", id as usize))
                    .small()
                    .label("Start receiving")
                    .on_click(answer(|| PromptAnswer::AllowPeer(true))),
            ),
        };
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .p(px(12.))
            .rounded(px(radius::CONTROL))
            .bg(p.accent_surface)
            .border_1()
            .border_color(p.choice_border.opacity(0.5))
            .child(
                icon(name)
                    .size(px(18.))
                    .flex_none()
                    .text_color(p.accent_text),
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
            .child(actions.flex_none())
            .into_any_element()
    }

    fn job_card(
        &mut self,
        job: &JobSnapshot,
        verified: Option<&VerifiedResult>,
        prompt: Option<&PendingPrompt>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fraction = job
            .progress
            .total_bytes
            .filter(|total| *total > 0)
            .map(|total| (job.progress.completed_bytes as f32 / total as f32).clamp(0., 1.));
        let saved = verified.is_some_and(|result| !result.exported_paths.is_empty());
        let export_error = verified.and_then(|result| result.export_error.as_ref());
        let (pill, tone, name) = match job.state {
            TransferState::Complete if saved && export_error.is_none() => {
                ("Saved", Tone::Success, "check")
            }
            TransferState::Complete => ("Verified", Tone::Success, "shield"),
            TransferState::Failed => ("Needs attention", Tone::Danger, "alert"),
            TransferState::Paused => ("Paused", Tone::Warning, "clock"),
            TransferState::PauseRequested => ("Pausing", Tone::Warning, "clock"),
            TransferState::Running => ("Downloading", Tone::Accent, "download"),
        };
        let transferred = match job.progress.total_bytes {
            Some(total) => format!(
                "{} of {}",
                format::bytes(job.progress.completed_bytes),
                format::bytes(total)
            ),
            None => format::bytes(job.progress.completed_bytes),
        };
        let title = if job.progress.name.is_empty() {
            "Transfer".to_owned()
        } else {
            job.progress.name.clone()
        };
        let export_id = job.id.clone();
        let export = cx.listener(move |panel, _: &ClickEvent, window, cx| {
            panel.export_verified(export_id.clone(), window, cx);
        });
        let prompt = prompt.map(|prompt| self.prompt_block(prompt, p, cx));

        surface::panel(p)
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(row::icon_tile(
                        p,
                        format::file_icon(&title),
                        40.,
                        job.state != TransferState::Failed,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text::subtitle(title).single_line().text_color(p.text))
                            .child(
                                text::small(if job.state == TransferState::Complete {
                                    format!("{transferred} · checked against the sender's manifest")
                                } else if job.progress.phase.is_empty() {
                                    transferred.clone()
                                } else {
                                    format!("{} · {transferred}", job.progress.phase)
                                })
                                .text_color(p.text_muted),
                            ),
                    )
                    .child(feedback::pill_with_icon(p, name, pill, tone)),
            )
            .when(
                !matches!(job.state, TransferState::Complete | TransferState::Failed),
                |this| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div().flex_1().child(
                                    progress(
                                        SharedString::from(format!("receive-progress-{}", job.id)),
                                        fraction,
                                    )
                                    .height(8.),
                                ),
                            )
                            .when_some(fraction, |this, fraction| {
                                this.child(
                                    text::caption(percent_label(fraction))
                                        .flex_none()
                                        .text_color(p.text_muted),
                                )
                            }),
                    )
                },
            )
            .children(prompt)
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
            .when_some(export_error, |this, error| {
                this.child(feedback::notice(
                    p,
                    format::readable_error(&error.detail),
                    Tone::Danger,
                ))
            })
            .when_some(
                verified.filter(|result| !result.exported_paths.is_empty()),
                |this, result| {
                    this.child(
                        surface::well(p)
                            .px(px(14.))
                            .py(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .children(result.exported_paths.iter().map(|path| {
                                text::code(path.display().to_string())
                                    .single_line()
                                    .text_color(p.text_muted)
                            })),
                    )
                },
            )
            .when(verified.is_some() && job.capabilities.can_export, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.))
                        .child(
                            text::small(if saved {
                                "The verified copy stays on this device until you remove it."
                            } else {
                                "Verified and held privately. Choose where to keep it."
                            })
                            .text_color(p.text_muted),
                        )
                        .child(if saved && export_error.is_none() {
                            button::secondary(SharedString::from(format!("export-{}", job.id)))
                                .icon("folder")
                                .label("Save another copy")
                                .on_click(export)
                        } else {
                            button::primary(SharedString::from(format!("export-{}", job.id)))
                                .icon("folder")
                                .label(if export_error.is_some() {
                                    "Try another folder"
                                } else {
                                    "Save to folder"
                                })
                                .on_click(export)
                        }),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use filebeam_client_config::{Appearance, Theme as AppearanceTheme};
    use gpui::{TestAppContext, px, size, test::TestWindowExt as _};

    use super::*;
    use crate::{
        model::{
            ClientError, ClientErrorCode, ClientMessage, OperationStatus, ReceiveItem,
            TransferCapabilities, TransferProgress,
        },
        testing,
    };

    type Commands = std::sync::mpsc::Receiver<ClientCommand>;

    fn panel_with(
        cx: &mut TestAppContext,
        mut snapshot: DesktopSnapshot,
    ) -> (gpui::WindowHandle<ReceivePanel>, Commands) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::theme::apply(
                &Appearance {
                    theme: AppearanceTheme::Dark,
                    reduced_motion: true,
                },
                cx,
            );
        });
        snapshot.instance.url = "https://default.example".into();
        let (client, commands) = DesktopClient::test_client(snapshot);
        let handle = cx.open_window(size(px(1280.), px(820.)), move |window, cx| {
            let live = cx.new(|cx| Live::new(Arc::new(client), cx));
            ReceivePanel::new(live, "/tmp/filebeam-ui-test".into(), window, cx)
        });
        (handle, commands)
    }

    fn panel(cx: &mut TestAppContext) -> (gpui::WindowHandle<ReceivePanel>, Commands) {
        panel_with(cx, DesktopSnapshot::default())
    }

    fn preview(operation: &str, link: &str) -> ReceivePreview {
        ReceivePreview {
            attached_note: None,
            operation_id: Some(operation.into()),
            link: link.into(),
            kind: ReceiveKind::File,
            password_required: false,
            burn_on_read: false,
            items: vec![
                ReceiveItem {
                    id: "a".into(),
                    name: "a.pdf".into(),
                    size: 10,
                },
                ReceiveItem {
                    id: "b".into(),
                    name: "b.zip".into(),
                    size: 20,
                },
            ],
        }
    }

    const LINK: &str = "https://receive.example/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    #[test]
    fn canonical_parser_keeps_explicit_origin_and_fragment_key() {
        let parsed = filebeam_transfer_native::protocol::parse_link_for_instance(
            LINK,
            "https://default.example",
        )
        .unwrap();
        assert_eq!(parsed.instance, "https://receive.example");
        assert_eq!(parsed.key, Some(vec![0; 32]));
    }

    #[gpui::test]
    fn an_invalid_draft_is_explained_and_never_dispatched(cx: &mut TestAppContext) {
        let (handle, commands) = panel(cx);
        handle
            .update(cx, |panel, window, cx| {
                panel.open(window, cx);
                assert_eq!(panel.error.as_deref(), Some("Enter a transfer link or ID."));
                assert!(panel.inspecting.is_none());
            })
            .unwrap();
        assert!(commands.try_recv().is_err());
    }

    #[gpui::test]
    fn opening_waits_for_the_service_and_new_downloads_start_fully_selected(
        cx: &mut TestAppContext,
    ) {
        let (handle, commands) = panel(cx);
        let client = handle
            .update(cx, |panel, window, cx| {
                panel.set_link(LINK.into(), window, cx);
                panel.open(window, cx);
                assert!(panel.inspecting.is_some());
                panel.client.clone()
            })
            .unwrap();
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::InspectReceive { .. })
        ));

        let mut snapshot = client.snapshot();
        snapshot.receive_previews.push(preview("op-1", LINK));
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert!(panel.inspecting.is_none());
                assert_eq!(panel.selections["op-1"].len(), 2);

                panel.toggle_item("op-1", "a", false, cx);
                // A later snapshot must not undo the user's choice.
                panel.on_snapshot(&snapshot, cx);
                assert_eq!(panel.selections["op-1"], HashSet::from(["b".to_owned()]));
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_refused_link_is_explained_in_place(cx: &mut TestAppContext) {
        let (handle, _commands) = panel(cx);
        let client = handle
            .update(cx, |panel, window, cx| {
                panel.set_link(LINK.into(), window, cx);
                panel.open(window, cx);
                panel.client.clone()
            })
            .unwrap();
        let mut snapshot = client.snapshot();
        snapshot.messages.push(ClientMessage {
            id: 3,
            operation: "inspect receive".into(),
            error: ClientError {
                code: ClientErrorCode::Remote,
                detail: "The server returned 404.".into(),
            },
        });
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert!(panel.inspecting.is_none());
                assert_eq!(
                    panel.error.as_deref(),
                    Some("That could not be found. It may have expired or been removed.")
                );
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_download_starts_only_after_its_selection_is_accepted(cx: &mut TestAppContext) {
        let (handle, commands) = panel(cx);
        let client = handle
            .update(cx, |panel, _, _| panel.client.clone())
            .unwrap();
        let mut snapshot = client.snapshot();
        snapshot.receive_previews.push(preview("op-1", LINK));
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                panel.select_all("op-1", Vec::new(), cx);
                panel.download("op-1".into(), cx);
                assert!(panel.starting.is_none(), "an empty selection never starts");
                assert!(panel.error.is_some());

                panel.select_all("op-1", vec!["b".into()], cx);
                panel.download("op-1".into(), cx);
                assert!(panel.starting.is_some());
            })
            .unwrap();
        let Ok(ClientCommand::SelectReceiveItems { item_ids, .. }) = commands.try_recv() else {
            panic!("the selection is applied first")
        };
        assert_eq!(item_ids, vec!["b".to_owned()]);
        assert!(commands.try_recv().is_err(), "start waits for acceptance");

        snapshot.operations.push(OperationStatus {
            operation: "select receive items".into(),
            detail: "completed".into(),
        });
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert!(panel.starting.is_none());
            })
            .unwrap();
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::StartPreparedReceive { operation_id }) if operation_id == "op-1"
        ));
    }

    #[gpui::test]
    fn a_rejected_selection_does_not_start_the_download(cx: &mut TestAppContext) {
        let (handle, commands) = panel(cx);
        let client = handle
            .update(cx, |panel, _, _| panel.client.clone())
            .unwrap();
        let mut snapshot = client.snapshot();
        snapshot.receive_previews.push(preview("op-1", LINK));
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                panel.download("op-1".into(), cx);
            })
            .unwrap();
        let _ = commands.try_recv();
        snapshot.messages.push(ClientMessage {
            id: 5,
            operation: "select receive items".into(),
            error: ClientError {
                code: ClientErrorCode::Storage,
                detail: "prepared receive operation is unavailable".into(),
            },
        });
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert!(panel.starting.is_none());
                assert!(panel.error.is_some());
            })
            .unwrap();
        assert!(commands.try_recv().is_err());
    }

    #[gpui::test]
    fn a_burning_note_opens_only_after_confirmation(cx: &mut TestAppContext) {
        let (window, panel, commands) =
            testing::rooted(cx, DesktopSnapshot::default(), |live, window, cx| {
                ReceivePanel::new(live, "/tmp/filebeam-ui-test".into(), window, cx)
            });
        testing::in_window(cx, window, |window, cx| {
            panel.update(cx, |panel, cx| {
                panel.open_note("https://default.example/n/abc".into(), true, window, cx)
            });
        });
        assert!(commands.try_recv().is_err(), "burning asks first");
        testing::in_window(cx, window, |window, cx| {
            window.render_frame(cx);
            window.click("dialog-confirm", cx);
        });
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::OpenNote {
                burn_acknowledged: true,
                ..
            })
        ));
    }

    #[gpui::test]
    fn a_secret_is_sent_once_typed_and_never_left_in_the_field(cx: &mut TestAppContext) {
        let (window, panel, commands) =
            testing::rooted(cx, DesktopSnapshot::default(), |live, window, cx| {
                ReceivePanel::new(live, "/tmp/filebeam-ui-test".into(), window, cx)
            });
        let confirm = |cx: &mut TestAppContext| {
            testing::in_window(cx, window, |window, cx| {
                window.render_frame(cx);
                window.click("dialog-confirm", cx);
            });
        };
        testing::in_window(cx, window, |window, cx| {
            panel.update(cx, |panel, cx| {
                panel.ask_secret("7".into(), 3, PromptKind::Password, window, cx)
            });
        });
        confirm(cx);
        assert!(commands.try_recv().is_err(), "an empty secret is not sent");

        testing::in_window(cx, window, |window, cx| {
            let secret = panel.read(cx).secret.clone();
            secret.update(cx, |input, cx| input.set_value("hunter2", window, cx));
        });
        confirm(cx);
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::AnswerPrompt {
                transfer_id,
                prompt_id: 3,
                answer: PromptAnswer::Secret(secret),
            }) if transfer_id == "7" && secret == "hunter2"
        ));
        panel.read_with(cx, |panel, cx| {
            assert!(panel.secret.read(cx).value().is_empty());
        });
    }

    #[gpui::test]
    fn every_receive_state_renders(cx: &mut TestAppContext) {
        let mut snapshot = DesktopSnapshot::default();
        snapshot.receive_previews.push(preview("op-1", LINK));
        snapshot.receive_previews.push(ReceivePreview {
            kind: ReceiveKind::Note,
            operation_id: None,
            password_required: true,
            burn_on_read: true,
            items: Vec::new(),
            ..preview("unused", LINK)
        });
        for (id, state) in [
            ("running", TransferState::Running),
            ("failed", TransferState::Failed),
            ("done", TransferState::Complete),
        ] {
            snapshot.jobs.push(JobSnapshot {
                id: id.into(),
                direction: TransferDirection::Receive,
                state,
                progress: TransferProgress {
                    name: "walkthrough.mp4".into(),
                    total_bytes: Some(100),
                    completed_bytes: 40,
                    ..TransferProgress::default()
                },
                share_url: None,
                separate_key: None,
                cli_command: None,
                checkpoint_id: None,
                origin: None,
                error: None,
                peer_warning: None,
                capabilities: TransferCapabilities {
                    can_export: true,
                    ..TransferCapabilities::default()
                },
            });
        }
        snapshot.verified_results.push(VerifiedResult {
            transfer_id: "done".into(),
            private_path: "/private".into(),
            verified_paths: vec!["/private/walkthrough.mp4".into()],
            exported_paths: vec!["/home/walkthrough.mp4".into()],
            export_error: None,
        });
        for (id, kind) in [
            (1, PromptKind::ShareKey),
            (2, PromptKind::PeerConsent),
            (3, PromptKind::Directory),
            (4, PromptKind::ShareReady),
        ] {
            snapshot.pending_prompts.push(PendingPrompt {
                transfer_id: if id == 1 { "running" } else { "elsewhere" }.into(),
                id,
                kind,
                peer: None,
                directory: None,
            });
        }
        let (handle, _commands) = panel_with(cx, snapshot);
        handle
            .update(cx, |panel, _, cx| {
                panel.note_text = Some(("note".into(), "decrypted".into()));
                cx.notify();
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("receive-link").is_some());
            assert!(window.try_find("start-prepared-op-1").is_some());
            assert!(window.try_find("export-done").is_some());
        })
        .unwrap();
    }
}
