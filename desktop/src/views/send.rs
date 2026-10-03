use std::{path::PathBuf, sync::Arc};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyWindowHandle, AppContext, Axis, Context, Entity, ExternalPaths, InteractiveElement,
    IntoElement, ParentElement, PathPromptOptions, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px, svg,
};
use gpui_base::{Radio as BaseRadio, RadioGroup as BaseRadioGroup};
use gpui_component::{
    Disableable, Icon, Sizable, Size, ThemeStyled, WindowExt,
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, Input, InputEvent, InputState},
    resizable::{ResizableState, h_resizable, resizable_panel},
    scroll::ScrollableElement,
    searchable_list::SearchableListItem,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    tab::{Tab, TabBar},
};

use crate::{
    client::DesktopClient,
    model::{
        ClientCommand, DirectoryMode, NoteDraft, NoteTransport, PolicyAvailability, PolicySnapshot,
        SendFiles, SendTransport,
    },
    theme::palette_for,
};
use gpui_component::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ComposerMode {
    Files,
    Notes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Transport {
    Http,
    Live,
}

/// The retained composer. It owns drafts and picker state; rendering only projects that state.
pub struct SendPanel {
    attach_note: bool,
    attached_title: Entity<InputState>,
    attached_editor: Entity<EditorState>,
    attached_language: Entity<SelectState<Vec<&'static str>>>,
    client: Arc<DesktopClient>,
    window_handle: AnyWindowHandle,
    #[allow(dead_code)]
    home: PathBuf,
    mode: ComposerMode,
    paths: Vec<PathBuf>,
    files: Preferences,
    notes: Preferences,
    note_title: Entity<InputState>,
    note_editor: Entity<EditorState>,
    language: Entity<SelectState<Vec<&'static str>>>,
    lifetime_availability: Entity<LifetimeAvailability>,
    layout: Entity<ResizableState>,
    page_scroll: ScrollHandle,
    inspector_width: f32,
    status: Option<String>,
    preparing_paths: bool,
}

/// Input entities deliberately belong to a mode, so swapping composers never
/// replaces an editor/input entity or merges two transfers' preferences.
struct Preferences {
    password: Entity<InputState>,
    recipient: Entity<InputState>,
    lifetime: Entity<SelectState<Vec<LifetimeChoice>>>,
    lifetime_choices: Vec<LifetimeChoice>,
    transport: Transport,
    reveal_password: bool,
    include_key: bool,
    turbo: bool,
    zip_directories: bool,
    burn_on_read: bool,
    transport_edited: bool,
}

struct LifetimeAvailability {
    hours: Vec<u64>,
}

impl Preferences {
    fn new(policy: &PolicySnapshot, window: &mut Window, cx: &mut Context<SendPanel>) -> Self {
        let lifetime_choices = lifetime_choices(policy, None);
        let select_choices = lifetime_choices.clone();
        Self {
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Optional transfer password")
                    .masked(true)
            }),
            recipient: cx.new(|cx| InputState::new(window, cx).placeholder("username")),
            lifetime: cx.new(move |cx| {
                SelectState::new(
                    select_choices,
                    Some(gpui_component::IndexPath::default()),
                    window,
                    cx,
                )
            }),
            lifetime_choices,
            transport: Transport::Http,
            reveal_password: false,
            include_key: true,
            turbo: false,
            zip_directories: true,
            burn_on_read: false,
            transport_edited: false,
        }
    }
}

impl SendPanel {
    pub fn new(
        client: Arc<DesktopClient>,
        home: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let policy = client.snapshot().policy;
        let files = Preferences::new(&policy, window, cx);
        let notes = Preferences::new(&policy, window, cx);
        let note_title = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled note"));
        let note_editor = cx.new(|cx| EditorState::new(window, cx).language("text"));
        let attached_title =
            cx.new(|cx| InputState::new(window, cx).placeholder("Attached note title (optional)"));
        let attached_editor = cx.new(|cx| EditorState::new(window, cx).language("text"));
        let attached_language = cx.new(|cx| {
            SelectState::new(
                vec![
                    "plain",
                    "php",
                    "dotenv",
                    "javascript",
                    "typescript",
                    "json",
                    "markdown",
                    "css",
                    "html",
                ],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            )
        });
        let language = cx.new(|cx| {
            SelectState::new(
                vec![
                    "plain",
                    "php",
                    "dotenv",
                    "javascript",
                    "typescript",
                    "json",
                    "markdown",
                    "css",
                    "html",
                ],
                Some(gpui_component::IndexPath::default()),
                window,
                cx,
            )
        });
        let lifetime_availability = cx.new(|_| LifetimeAvailability {
            hours: policy.retention_options_hours.clone(),
        });
        let layout = cx.new(|_| ResizableState::default());
        let inspector_width = crate::platform::preferences::load(&home).inspector_width;

        let mut panel = Self {
            attach_note: false,
            attached_title,
            attached_editor,
            attached_language,
            client,
            window_handle: window.window_handle(),
            home,
            mode: ComposerMode::Files,
            paths: Vec::new(),
            files,
            notes,
            note_title,
            note_editor,
            language,
            lifetime_availability,
            layout,
            page_scroll: ScrollHandle::new(),
            inspector_width,
            status: None,
            preparing_paths: false,
        };
        panel.apply_server_default(&policy);
        panel.subscribe_inputs(cx);
        panel
    }

    fn subscribe_inputs(&mut self, cx: &mut Context<Self>) {
        for input in [
            &self.files.password,
            &self.files.recipient,
            &self.notes.password,
            &self.notes.recipient,
            &self.note_title,
            &self.attached_title,
        ] {
            cx.subscribe(input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.status = None;
                    cx.notify();
                }
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.submit(cx),
                _ => {}
            })
            .detach();
        }
        for lifetime in [&self.files.lifetime, &self.notes.lifetime] {
            cx.subscribe(
                lifetime,
                |this, _, event: &SelectEvent<Vec<LifetimeChoice>>, cx| {
                    if matches!(event, SelectEvent::Confirm(_)) {
                        this.status = None;
                        cx.notify();
                    }
                },
            )
            .detach();
        }
        cx.subscribe(
            &self.note_editor,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.status = None;
                    cx.notify();
                }
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.submit(cx),
                _ => {}
            },
        )
        .detach();
        let editor = self.note_editor.clone();
        cx.subscribe(&self.attached_editor, |this, _, _: &InputEvent, cx| {
            this.status = None;
            cx.notify();
        })
        .detach();
        let attached_editor = self.attached_editor.clone();
        cx.subscribe(
            &self.attached_language,
            move |_, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if let SelectEvent::Confirm(Some(language)) = event {
                    attached_editor.update(cx, |editor, cx| {
                        editor.set_highlighter(
                            if *language == "plain" {
                                "plaintext"
                            } else {
                                language
                            },
                            cx,
                        )
                    });
                }
            },
        )
        .detach();
        cx.subscribe(
            &self.language,
            move |_, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if let SelectEvent::Confirm(Some(language)) = event {
                    // `plain` remains the wire value; the editor registry calls its neutral
                    // highlighter `plaintext`.
                    let highlighter = if *language == "plain" {
                        "plaintext"
                    } else {
                        language
                    };
                    editor.update(cx, |editor, cx| editor.set_highlighter(highlighter, cx));
                }
            },
        )
        .detach();
    }

    pub fn append_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            if !self.paths.contains(&path) {
                self.paths.push(path);
            }
        }
        self.preparing_paths = false;
        self.status = None;
        cx.notify();
    }

    fn choose_paths(&mut self, directories: bool, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: !directories,
            prompt: Some(
                if directories {
                    "Choose folder"
                } else {
                    "Choose files"
                }
                .into(),
            ),
        });
        let window_handle = window.window_handle();
        self.preparing_paths = true;
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => this.append_paths(paths, cx),
                Ok(Ok(None)) => {
                    this.preparing_paths = false;
                    cx.notify();
                }
                Ok(Err(error)) => {
                    this.preparing_paths = false;
                    this.status = None;
                    cx.notify();
                    let detail = error.to_string();
                    let _ = window_handle.update(cx, |_, window, cx| {
                        crate::toast::error("File selection failed", detail, window, cx);
                    });
                }
                Err(_) => {
                    this.preparing_paths = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            self.status = Some(
                if self.mode == ComposerMode::Files {
                    "Choose files or a folder before sharing."
                } else {
                    "Write a note before sharing."
                }
                .into(),
            );
            cx.notify();
            return;
        }
        let preferences = self.active_preferences();
        let snapshot = self.client.snapshot();
        let mut command = match build_command_for_policy(
            self.mode,
            self.paths.clone(),
            self.note_editor.read(cx).value().to_string(),
            self.optional_text(&self.note_title, cx),
            self.language
                .read(cx)
                .selected_value()
                .copied()
                .unwrap_or("plain")
                .into(),
            PreferenceValues::from_inputs(preferences, cx),
            &snapshot.policy,
            snapshot.account.authenticated,
        ) {
            Ok(command) => command,
            Err(error) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        if let ClientCommand::SendFiles(files) = &mut command
            && self.attach_note
        {
            let note = filebeam_client_core::protocol::AttachedNote {
                text: self.attached_editor.read(cx).value().to_string(),
                title: self.optional_text(&self.attached_title, cx),
                language: self
                    .attached_language
                    .read(cx)
                    .selected_value()
                    .copied()
                    .unwrap_or("plain")
                    .into(),
            };
            if let Err(error) = note.validate() {
                self.status = Some(error);
                cx.notify();
                return;
            }
            files.attached_note = Some(note);
        }
        self.status = match self.client.dispatch(command) {
            Ok(()) => Some("Queued for local encryption. Your draft remains available.".into()),
            Err(error) => {
                let detail = error.to_string();
                let _ = self.window_handle.update(cx, |_, window, cx| {
                    crate::toast::error("Share failed", detail, window, cx);
                });
                None
            }
        };
        cx.notify();
    }

    fn optional_text(&self, input: &Entity<InputState>, cx: &Context<Self>) -> Option<String> {
        let value = input.read(cx).value().trim().to_owned();
        (!value.is_empty()).then_some(value)
    }

    fn can_submit(&self, cx: &Context<Self>) -> bool {
        draft_has_content(self.mode, &self.paths, &self.note_editor.read(cx).value())
    }

    fn active_preferences(&self) -> &Preferences {
        if self.mode == ComposerMode::Files {
            &self.files
        } else {
            &self.notes
        }
    }

    fn active_preferences_mut(&mut self) -> &mut Preferences {
        if self.mode == ComposerMode::Files {
            &mut self.files
        } else {
            &mut self.notes
        }
    }

    fn set_active_password_revealed(
        &mut self,
        revealed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preferences = self.active_preferences_mut();
        preferences.reveal_password = revealed;
        preferences.password.update(cx, |input, cx| {
            input.set_masked(!revealed, window, cx);
        });
    }

    fn apply_server_default(&mut self, policy: &PolicySnapshot) {
        if policy.availability != PolicyAvailability::Unavailable {
            for preferences in [&mut self.files, &mut self.notes] {
                if !preferences.transport_edited {
                    preferences.transport = if policy.default_transport == SendTransport::Http {
                        Transport::Http
                    } else {
                        Transport::Live
                    };
                }
            }
        }
    }

    fn sync_lifetime_options(
        &mut self,
        policy: &PolicySnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for preferences in [&mut self.files, &mut self.notes] {
            let selected = preferences.lifetime.read(cx).selected_value().cloned();
            let choices = lifetime_choices(policy, selected);
            if preferences.lifetime_choices == choices {
                continue;
            }
            let selected = selected.unwrap_or(None);
            preferences.lifetime.update(cx, |state, cx| {
                state.set_items(choices.clone(), window, cx);
                state.set_selected_value(&selected, window, cx);
            });
            preferences.lifetime_choices = choices;
        }
    }

    /// Called by the app shell after it applies a refreshed desktop snapshot.
    /// Policy refreshes occur outside render so retained drafts can be reconciled safely.
    pub fn refresh_policy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let policy = self.client.snapshot().policy;
        self.apply_server_default(&policy);
        self.sync_lifetime_options(&policy, window, cx);
        self.lifetime_availability.update(cx, |availability, cx| {
            availability.hours = policy.retention_options_hours.clone();
            cx.notify();
        });
        cx.notify();
    }

    #[cfg(feature = "visual-test")]
    pub fn show_notes_for_capture(&mut self, cx: &mut Context<Self>) {
        self.mode = ComposerMode::Notes;
        cx.notify();
    }

    /// Seeds a visual-only retained note. The shell calls this after switching modes because
    /// native input and editor setters require the active window.
    #[cfg(feature = "visual-test")]
    pub fn seed_notes_for_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.note_editor.read(cx).value().is_empty() {
            self.note_title.update(cx, |title, cx| {
                title.set_value("deployment.md", window, cx);
            });
            self.note_editor.update(cx, |editor, cx| {
                editor.set_value(
                    "# Production handoff\n\n## Before the release\n- Review the build and migration plan\n- Confirm the rollback point\n- Share the release notes\n\n```sh\nbeam up ./release-notes.md\n```\n\nKeep the decryption key with the intended recipient.",
                    window,
                    cx,
                );
                editor.set_highlighter("markdown", cx);
            });
            self.language.update(cx, |language, cx| {
                language.set_selected_value(&"markdown", window, cx);
            });
        }
        cx.notify();
    }

    /// Focuses the native trigger. The capture driver activates it with Space or Enter because
    /// GPUI Kit intentionally keeps SelectState's menu-open operation private.
    #[cfg(feature = "visual-test")]
    pub fn open_lifetime_dropdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lifetime = self.active_preferences().lifetime.clone();
        lifetime.update(cx, |state, cx| state.focus(window, cx));
    }

    fn receive_drop(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.append_paths(paths.paths().to_vec(), cx);
    }

    fn open_preferences_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preferences = self.active_preferences();
        let lifetime_availability = self.lifetime_availability.clone();
        let password = preferences.password.clone();
        let lifetime = preferences.lifetime.clone();
        let lifetime_error =
            cx.new(|cx| LifetimeErrorHint::new(lifetime.clone(), lifetime_availability, cx));
        let recipient = preferences.recipient.clone();
        let reveal_password = preferences.reveal_password;
        let include_key = preferences.include_key;
        let turbo = preferences.turbo;
        let zip_or_burn = if self.mode == ComposerMode::Files {
            preferences.zip_directories
        } else {
            preferences.burn_on_read
        };
        let mode = self.mode;
        let panel = cx.entity().downgrade();
        window.open_sheet(cx, move |sheet, _, _| {
            let reveal_panel = panel.clone();
            let include_panel = panel.clone();
            let turbo_panel = panel.clone();
            let zip_panel = panel.clone();
            sheet
                .title("Sharing preferences")
                .size(px(360.))
                .resizable(false)
                .child(
                    div().flex_1().min_h_0().overflow_y_scrollbar().child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child("Preferences remain attached to this transfer.")
                            .child(div().mt(px(14.)).child("Password"))
                            .child(
                                Input::new(&password)
                                    .id("sheet-transfer-password")
                                    .with_size(Size::Large)
                                    .h(px(38.)),
                            )
                            .child(
                                Switch::new("sheet-reveal-password")
                                    .label("Reveal password")
                                    .checked(reveal_password)
                                    .on_click(move |checked, window, cx| {
                                        let _ = reveal_panel.update(cx, |this, cx| {
                                            this.set_active_password_revealed(*checked, window, cx);
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(div().mt(px(14.)).child("Link lifetime"))
                            .child(
                                Select::new(&lifetime)
                                    .id("sheet-transfer-lifetime")
                                    .with_size(Size::Large)
                                    .h(px(38.)),
                            )
                            .child(
                                div()
                                    .mt(px(6.))
                                    .text_size(px(10.))
                                    .child("The server may shorten this lifetime."),
                            )
                            .child(lifetime_error.clone())
                            .child(
                                Switch::new("sheet-include-key")
                                    .label("Include key")
                                    .checked(include_key)
                                    .on_click(move |checked, _, cx| {
                                        let _ = include_panel.update(cx, |this, cx| {
                                            this.active_preferences_mut().include_key = *checked;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Switch::new("sheet-turbo")
                                    .label("Turbo")
                                    .checked(turbo)
                                    .disabled(mode == ComposerMode::Notes)
                                    .on_click(move |checked, _, cx| {
                                        let _ = turbo_panel.update(cx, |this, cx| {
                                            this.active_preferences_mut().turbo = *checked;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Switch::new("sheet-zip-or-burn")
                                    .label(if mode == ComposerMode::Files {
                                        "ZIP folders"
                                    } else {
                                        "Burn on read"
                                    })
                                    .checked(zip_or_burn)
                                    .on_click(move |checked, _, cx| {
                                        let _ = zip_panel.update(cx, |this, cx| {
                                            if mode == ComposerMode::Files {
                                                this.files.zip_directories = *checked;
                                            } else {
                                                this.notes.burn_on_read = *checked;
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(div().mt(px(14.)).child("Recipient"))
                            .child(
                                Input::new(&recipient)
                                    .id("sheet-transfer-recipient")
                                    .with_size(Size::Large)
                                    .h(px(38.)),
                            ),
                    ),
                )
        });
    }
}

fn draft_has_content(mode: ComposerMode, paths: &[PathBuf], note: &str) -> bool {
    match mode {
        ComposerMode::Files => !paths.is_empty(),
        ComposerMode::Notes => !note.trim().is_empty(),
    }
}

fn drop_side_art_svg(left: &str, right: &str) -> String {
    // GPUI Kit transforms SVG but not Div. Keep the reviewed Iconsax assets embedded so each
    // side card and its icon rotate together while the center card stays native and upright.
    // The 108px viewport leaves room for the rotated cards' 104px lower edge and stroke.
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 168 108">
  <g transform="translate(15 24) rotate(-16 31.5 36)">
    {left}
  </g>
  <g transform="translate(89 20) rotate(17 31.5 36)">
    {right}
  </g>
</svg>"##,
        left = left,
        right = right,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LifetimeChoice {
    hours: Option<u64>,
    label: String,
}

struct LifetimeErrorHint {
    lifetime: Entity<SelectState<Vec<LifetimeChoice>>>,
    availability: Entity<LifetimeAvailability>,
    _lifetime_subscription: Subscription,
    _availability_subscription: Subscription,
}

impl LifetimeErrorHint {
    fn new(
        lifetime: Entity<SelectState<Vec<LifetimeChoice>>>,
        availability: Entity<LifetimeAvailability>,
        cx: &mut Context<Self>,
    ) -> Self {
        let lifetime_subscription = cx.subscribe(
            &lifetime,
            |_, _, _: &SelectEvent<Vec<LifetimeChoice>>, cx| cx.notify(),
        );
        let availability_subscription = cx.observe(&availability, |_, _, cx| cx.notify());
        Self {
            lifetime,
            availability,
            _lifetime_subscription: lifetime_subscription,
            _availability_subscription: availability_subscription,
        }
    }
}

impl Render for LifetimeErrorHint {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette_for(Theme::global(cx).mode);
        let selected = self.lifetime.read(cx).selected_value().copied();
        let available_hours = &self.availability.read(cx).hours;
        div().when_some(
            unavailable_lifetime_error(selected, available_hours),
            |this, error| this.child(div().text_size(px(10.)).text_color(p.danger).child(error)),
        )
    }
}

impl LifetimeChoice {
    fn default_choice(hours: Option<u64>) -> Self {
        Self {
            hours: None,
            label: match hours {
                Some(hours) => format!("Default ({})", duration_label(hours)),
                None => "Default lifetime".into(),
            },
        }
    }
}

impl SearchableListItem for LifetimeChoice {
    type Value = Option<u64>;

    fn title(&self) -> SharedString {
        self.label.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.hours
    }
}

fn duration_label(hours: u64) -> String {
    match hours {
        1 => "1 hour".into(),
        hours if hours % (24 * 7) == 0 => {
            let weeks = hours / (24 * 7);
            format!("{weeks} {}", if weeks == 1 { "week" } else { "weeks" })
        }
        hours if hours % 24 == 0 => {
            let days = hours / 24;
            format!("{days} {}", if days == 1 { "day" } else { "days" })
        }
        hours => format!("{hours} hours"),
    }
}

fn lifetime_choices(policy: &PolicySnapshot, selected: Option<Option<u64>>) -> Vec<LifetimeChoice> {
    let mut hours = policy.retention_options_hours.clone();
    hours.sort_unstable();
    hours.dedup();

    let mut choices = vec![LifetimeChoice::default_choice(
        policy.default_retention_hours,
    )];
    choices.extend(hours.iter().copied().map(|hours| LifetimeChoice {
        hours: Some(hours),
        label: duration_label(hours),
    }));

    if let Some(Some(selected)) = selected
        && !hours.contains(&selected)
    {
        choices.push(LifetimeChoice {
            hours: Some(selected),
            label: format!("{} (unavailable)", duration_label(selected)),
        });
    }
    choices
}

fn unavailable_lifetime_error(
    selected: Option<Option<u64>>,
    available_hours: &[u64],
) -> Option<String> {
    selected.flatten().and_then(|hours| {
        (!available_hours.contains(&hours))
            .then_some(String::from("Lifetime is not available on this instance."))
    })
}

#[derive(Clone)]
struct PreferenceValues {
    password: String,
    recipient: String,
    lifetime: String,
    transport: Transport,
    include_key: bool,
    turbo: bool,
    zip_directories: bool,
    burn_on_read: bool,
}

impl PreferenceValues {
    fn from_inputs(preferences: &Preferences, cx: &Context<SendPanel>) -> Self {
        let value = |input: &Entity<InputState>| input.read(cx).value().to_string();
        Self {
            password: value(&preferences.password),
            recipient: value(&preferences.recipient),
            lifetime: preferences
                .lifetime
                .read(cx)
                .selected_value()
                .copied()
                .flatten()
                .map(|hours| hours.to_string())
                .unwrap_or_default(),
            transport: preferences.transport,
            include_key: preferences.include_key,
            turbo: preferences.turbo,
            zip_directories: preferences.zip_directories,
            burn_on_read: preferences.burn_on_read,
        }
    }
}

fn retention_hours(value: &str) -> Result<Option<u64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let hours = value
        .parse::<u64>()
        .map_err(|_| "Lifetime must be a positive whole number of hours.".to_string())?;
    if hours == 0 {
        return Err("Lifetime must be a positive whole number of hours.".into());
    }
    Ok(Some(hours))
}

fn driver_policy_label(policy: &crate::model::DriverPolicy) -> String {
    let size = policy
        .maximum_ciphertext_bytes
        .map(|bytes| {
            if bytes >= 1024 * 1024 * 1024 {
                format!(
                    "{:.1} GiB per transfer",
                    bytes as f64 / (1024 * 1024 * 1024) as f64
                )
            } else {
                format!("{} MiB per transfer", bytes / (1024 * 1024))
            }
        })
        .unwrap_or_else(|| "Transfer limit not advertised".into());
    policy
        .maximum_file_count
        .map(|count| format!("{size} · Up to {count} files"))
        .unwrap_or(size)
}

fn retention_label(_: &PolicySnapshot) -> String {
    "The server may shorten this lifetime.".into()
}

#[cfg(test)]
fn build_command(
    mode: ComposerMode,
    paths: Vec<PathBuf>,
    text: String,
    title: Option<String>,
    language: String,
    preferences: PreferenceValues,
) -> Result<ClientCommand, String> {
    build_command_for_policy(
        mode,
        paths,
        text,
        title,
        language,
        preferences,
        &PolicySnapshot {
            availability: PolicyAvailability::Available,
            anonymous_uploads: true,
            default_retention_hours: Some(12),
            retention_options_hours: vec![12],
            http: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            webrtc: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            ..PolicySnapshot::default()
        },
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_command_for_policy(
    mode: ComposerMode,
    paths: Vec<PathBuf>,
    text: String,
    title: Option<String>,
    language: String,
    preferences: PreferenceValues,
    policy: &PolicySnapshot,
    authenticated: bool,
) -> Result<ClientCommand, String> {
    let retention_hours = retention_hours(&preferences.lifetime)?;
    if policy.availability == PolicyAvailability::Unavailable {
        return Err(policy
            .reason
            .clone()
            .unwrap_or_else(|| "Instance policy is unavailable; refresh before sharing.".into()));
    }
    let driver = if preferences.transport == Transport::Http {
        &policy.http
    } else {
        &policy.webrtc
    };
    if !driver.enabled {
        return Err("This transport is not enabled by the current instance.".into());
    }
    if let Some(hours) = retention_hours
        && !policy.retention_options_hours.contains(&hours)
    {
        return Err("Lifetime is not one of this instance's available retention choices.".into());
    }
    // Empty alone means no password. Whitespace is a valid, intentional secret.
    let password = (!preferences.password.is_empty()).then_some(preferences.password);
    let recipient = (!preferences.recipient.trim().is_empty())
        .then_some(preferences.recipient.trim().to_owned());
    match mode {
        ComposerMode::Files => {
            if recipient.is_some() && !authenticated {
                return Err("Recipient delivery requires an authenticated account.".into());
            }
            if recipient.is_some() && preferences.transport != Transport::Http {
                return Err("Recipient delivery is available with HTTP only; choose HTTP or clear the recipient before sharing.".into());
            }
            if recipient.is_some() && password.is_some() {
                return Err("Recipient delivery cannot be combined with a transfer password; clear one before sharing.".into());
            }
            if recipient.is_none() && !policy.anonymous_uploads {
                return Err("This instance requires an authenticated account for sharing.".into());
            }
            Ok(ClientCommand::SendFiles(SendFiles {
                attached_note: None,
                paths,
                directory_mode: if preferences.zip_directories {
                    DirectoryMode::Zip
                } else {
                    DirectoryMode::Individual
                },
                transport: if preferences.transport == Transport::Http {
                    SendTransport::Http
                } else {
                    SendTransport::Live
                },
                retention_hours,
                turbo: preferences.turbo,
                include_key: preferences.include_key,
                password,
                recipient,
            }))
        }
        ComposerMode::Notes => {
            if recipient.is_some() {
                return Err("Recipient delivery is not available for notes; clear the recipient before sharing.".into());
            }
            Ok(ClientCommand::CreateNote(NoteDraft {
                text,
                title,
                language,
                password,
                burn_on_read: preferences.burn_on_read,
                include_key: preferences.include_key,
                retention_hours,
                transport: if preferences.transport == Transport::Http {
                    NoteTransport::Http
                } else {
                    NoteTransport::Live
                },
            }))
        }
    }
}

impl Render for SendPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let policy = self.client.snapshot().policy;
        let p = palette_for(Theme::global(cx).mode);
        let files_ready = !self.paths.is_empty();
        let http_selected = self.active_preferences().transport == Transport::Http;
        let live_selected = !http_selected;
        let panel = cx.entity().downgrade();
        let http_focus = window
            .use_keyed_state(0usize, cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let live_focus = window
            .use_keyed_state(1usize, cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let composer_height = px(660.);
        let drop_side_cards = drop_side_art_svg(
            r#"<rect width="63" height="72" rx="10" fill="currentColor"/>"#,
            r#"<rect width="63" height="72" rx="10" fill="currentColor"/>"#,
        );
        let drop_side_borders = drop_side_art_svg(
            r#"<rect width="63" height="72" rx="10" fill="none" stroke="currentColor"/>"#,
            r#"<rect width="63" height="72" rx="10" fill="none" stroke="currentColor"/>"#,
        );
        let drop_side_icons = drop_side_art_svg(
            &format!(
                r#"<svg x="22" y="26" width="19" height="19">{}</svg>"#,
                include_str!("../../assets/icons/image.svg"),
            ),
            &format!(
                r#"<svg x="22" y="26" width="19" height="19">{}</svg>"#,
                include_str!("../../assets/icons/code.svg"),
            ),
        );
        let footer = div()
            .h(px(58.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .justify_between()
            .bg(p.raised)
            .rounded_b(px(18.))
            .border_t_1()
            .border_color(p.border)
            .child(div().text_size(px(11.)).text_color(p.muted).child(
                self.status.clone().unwrap_or_else(|| {
                    policy
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Encrypt locally before sharing.".into())
                }),
            ))
            .child(
                Button::new("encrypt-share")
                    .icon(Icon::default().path("icons/lock.svg").size(px(16.)))
                    .label("Encrypt and share")
                    .primary()
                    .with_size(Size::Large)
                    .h(px(43.))
                    .disabled(
                        !self.can_submit(cx)
                            || self.preparing_paths
                            || policy.availability == PolicyAvailability::Unavailable,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
            );
        let composer = div()
            .w_full()
            .h(composer_height)
            .min_h(composer_height)
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(18.))
            .overflow_hidden()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .h(px(66.))
                    .flex_none()
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        TabBar::new("send-mode-tabs")
                            .segmented()
                            .with_size(Size::Large)
                            .selected_index(if self.mode == ComposerMode::Files {
                                0
                            } else {
                                1
                            })
                            .children([
                                Tab::new()
                                    .aria_label("Send files")
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(7.))
                                            .child(
                                                Icon::default()
                                                    .path("icons/folder.svg")
                                                    .size(px(14.)),
                                            )
                                            .child("Files"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mode = ComposerMode::Files;
                                        cx.notify();
                                    })),
                                Tab::new()
                                    .aria_label("Send notes")
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(7.))
                                            .child(
                                                Icon::default()
                                                    .path("icons/note.svg")
                                                    .size(px(14.)),
                                            )
                                            .child("Notes"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mode = ComposerMode::Notes;
                                        cx.notify();
                                    })),
                            ]),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.subtle)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(crate::assets::icon("lock").size(px(14.)))
                            .child("End-to-end encrypted"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div().flex_none().px(px(21.)).pt(px(18.)).child(
                            BaseRadioGroup::new("send-transport")
                                .axis(Axis::Horizontal)
                                .w_full()
                                .flex()
                                .gap(px(8.))
                                .children([
                                    BaseRadio::new(0usize)
                                        .accessibility_label("HTTP")
                                        .disabled(!policy.http.enabled)
                                        .checked(http_selected)
                                        .set_position(1, 2)
                                        .track_focus(&http_focus)
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(59.))
                                        .p(px(12.))
                                        .flex()
                                        .items_start()
                                        .gap(px(10.))
                                        .rounded(px(10.))
                                        .border_1()
                                        .text_color(if http_selected { p.text } else { p.muted })
                                        .border_color(if http_selected {
                                            p.accent
                                        } else {
                                            p.border
                                        })
                                        .bg(if http_selected { p.raised } else { p.surface })
                                        .hover(|this| {
                                            this.bg(p.hover).border_color(if http_selected {
                                                p.accent
                                            } else {
                                                p.focus
                                            })
                                        })
                                        .active(|this| this.bg(p.hover).border_color(p.accent))
                                        .when(http_focus.is_focused(window), |this| {
                                            this.focus_ring_style(window, cx)
                                        })
                                        .child(
                                            div()
                                                .size(px(24.))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .flex_shrink_0()
                                                .rounded(px(6.))
                                                .bg(if http_selected {
                                                    p.selected
                                                } else {
                                                    p.raised
                                                })
                                                .child(
                                                    crate::assets::icon("http-server")
                                                        .size(px(18.))
                                                        .text_color(if http_selected {
                                                            p.accent
                                                        } else {
                                                            p.muted
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_start()
                                                .gap(px(0.))
                                                .text_size(px(13.))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(2.))
                                                        .child("HTTP")
                                                        .child(
                                                            div()
                                                                .text_size(px(10.))
                                                                .text_color(p.muted)
                                                                .child("Download later."),
                                                        ),
                                                ),
                                        )
                                        .on_change({
                                            let panel = panel.clone();
                                            move |_, _, _, cx| {
                                                let _ = panel.update(cx, |this, cx| {
                                                    let preferences = this.active_preferences_mut();
                                                    preferences.transport = Transport::Http;
                                                    preferences.transport_edited = true;
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                    BaseRadio::new(1usize)
                                        .accessibility_label("WebRTC")
                                        .disabled(!policy.webrtc.enabled)
                                        .checked(live_selected)
                                        .set_position(2, 2)
                                        .track_focus(&live_focus)
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(59.))
                                        .p(px(12.))
                                        .flex()
                                        .items_start()
                                        .gap(px(10.))
                                        .rounded(px(10.))
                                        .border_1()
                                        .text_color(if live_selected { p.text } else { p.muted })
                                        .border_color(if live_selected {
                                            p.accent
                                        } else {
                                            p.border
                                        })
                                        .bg(if live_selected { p.raised } else { p.surface })
                                        .hover(|this| {
                                            this.bg(p.hover).border_color(if live_selected {
                                                p.accent
                                            } else {
                                                p.focus
                                            })
                                        })
                                        .active(|this| this.bg(p.hover).border_color(p.accent))
                                        .when(live_focus.is_focused(window), |this| {
                                            this.focus_ring_style(window, cx)
                                        })
                                        .child(
                                            div()
                                                .size(px(24.))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .flex_shrink_0()
                                                .rounded(px(6.))
                                                .bg(if live_selected {
                                                    p.selected
                                                } else {
                                                    p.raised
                                                })
                                                .child(
                                                    crate::assets::icon("webrtc-p2p")
                                                        .size(px(18.))
                                                        .text_color(if live_selected {
                                                            p.accent
                                                        } else {
                                                            p.muted
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_start()
                                                .gap(px(0.))
                                                .text_size(px(13.))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(2.))
                                                        .child("WebRTC")
                                                        .child(
                                                            div()
                                                                .text_size(px(10.))
                                                                .text_color(p.muted)
                                                                .child("Keep Filebeam open."),
                                                        ),
                                                ),
                                        )
                                        .on_change({
                                            let panel = panel.clone();
                                            move |_, _, _, cx| {
                                                let _ = panel.update(cx, |this, cx| {
                                                    let preferences = this.active_preferences_mut();
                                                    preferences.transport = Transport::Live;
                                                    preferences.transport_edited = true;
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                ]),
                        ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .mx(px(21.))
                            .mt(px(10.))
                            .mb(px(7.))
                            .flex()
                            .justify_between()
                            .text_size(px(10.))
                            .text_color(p.subtle)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        crate::assets::icon(
                                            if self.active_preferences().transport
                                                == Transport::Http
                                            {
                                                "http-server"
                                            } else {
                                                "webrtc-p2p"
                                            },
                                        )
                                        .size(px(12.)),
                                    )
                                    .child(driver_policy_label(
                                        if self.active_preferences().transport == Transport::Http {
                                            &policy.http
                                        } else {
                                            &policy.webrtc
                                        },
                                    )),
                            )
                            .child("Instance policy"),
                    )
                    .child(if self.mode == ComposerMode::Files {
                        div()
                            .mx(px(20.))
                            .mt(px(4.))
                            .mb(px(20.))
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .size_full()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_start()
                                    .py(px(24.))
                                    .rounded(px(12.))
                                    .border_1()
                                    .border_dashed()
                                    .border_color(p.border)
                                    .drag_over::<ExternalPaths>(move |this, _, _, _| {
                                        this.bg(p.selected).border_color(p.accent)
                                    })
                                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                                        this.receive_drop(paths, cx)
                                    }))
                                    .when(!files_ready, |this| {
                                this.child(
                                        div()
                                            .relative()
                                            .w(px(168.))
                                            .h(px(108.))
                                        .flex_none()
                                        .mb(px(15.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            svg()
                                                .data(drop_side_cards.as_bytes())
                                                .absolute()
                                                .inset_0()
                                                .size_full()
                                                .text_color(p.raised),
                                        )
                                        .child(
                                            svg()
                                                .data(drop_side_borders.as_bytes())
                                                .absolute()
                                                .inset_0()
                                                .size_full()
                                                .text_color(p.border),
                                        )
                                        .child(
                                            svg()
                                                .data(drop_side_icons.as_bytes())
                                                .absolute()
                                                .inset_0()
                                                .size_full()
                                                .text_color(p.accent),
                                        )
                                        .child(
                                            div()
                                                .relative()
                                                .w(px(68.))
                                                .h(px(85.))
                                                .rounded(px(10.))
                                                .border_1()
                                                .border_color(p.border)
                                                .bg(p.selected)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(crate::assets::mark().size(px(39.))),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .bottom(px(4.))
                                                .w(px(20.))
                                                .h(px(20.))
                                                .rounded_full()
                                                .border_1()
                                                .border_color(p.border)
                                                .bg(p.inset)
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(crate::assets::icon("lock").size(px(11.))),
                                        ),
                                )
                            })
                            .child(if files_ready {
                                div()
                                    .flex_none()
                                    .text_size(px(28.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(format!("{} item(s) ready", self.paths.len()))
                                    .into_any_element()
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_none()
                                    .text_size(px(28.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Drop your files ")
                                    .child(div().text_color(p.accent).child("here"))
                                    .into_any_element()
                            })
                            .child(
                                div()
                                    .mt(px(7.))
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child(if self.preparing_paths {
                                        "Adding selection..."
                                    } else {
                                        "They are encrypted on your device before they leave it."
                                    }),
                            )
                            .child(
                                div()
                                    .mt(px(19.))
                                    .flex_none()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        Button::new("choose-files")
                                            .label("Choose files")
                                            .icon(Icon::default().path("icons/folder.svg"))
                                            .primary()
                                            .with_size(Size::Large)
                                            .h(px(43.))
                                            .min_w(px(143.))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_paths(false, window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("choose-folder")
                                            .label("Choose folder")
                                            .outline()
                                            .with_size(Size::Large)
                                            .h(px(43.))
                                            .min_w(px(127.))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_paths(true, window, cx)
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(12.))
                                    .flex_none()
                                    .text_size(px(10.))
                                    .text_color(p.subtle)
                                    .child("Or drag and drop anywhere"),
                            )
                            .children(self.paths.iter().enumerate().map(|(index, path)| {
                                Button::new(("remove-path", index))
                                    .label(format!(
                                        "Remove {}",
                                        path.file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("item")
                                    ))
                                    .ghost()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.paths.remove(index);
                                        cx.notify();
                                    }))
                            }))
                            .when(files_ready, |this| {
                                this.child(
                                    Button::new("remove-all-paths")
                                        .label("Remove all")
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.paths.clear();
                                            cx.notify();
                                        })),
                                )
                            })
                                    .into_any_element(),
                            )
                            .child(Button::new("attach-note").label(if self.attach_note { "Remove attached note" } else { "Attach note" }).ghost().on_click(cx.listener(|this, _, _, cx| { this.attach_note = !this.attach_note; cx.notify(); })))
                            .when(self.attach_note, |this| this.child(
                                div().p(px(16.)).flex().flex_col().gap(px(8.))
                                    .child("The note uses this transfer's encryption and expiry.")
                                    .child(Input::new(&self.attached_title).id("attached-note-title"))
                                    .child(Select::new(&self.attached_language).id("attached-note-language"))
                                    .child(Editor::new(&self.attached_editor).aria_label("Attached note contents").h(px(180.)))
                                    .child(format!("{} / 65536 UTF-8 bytes", self.attached_editor.read(cx).value().len()))
                            ))
                            .into_any_element()
                    } else {
                        let note_characters = self.note_editor.read(cx).value().chars().count();
                        div()
                            .mx(px(20.))
                            .mt(px(8.))
                            .mb(px(20.))
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(p.border)
                            .overflow_hidden()
                            .bg(p.inset)
                            .child(
                                div()
                                    .w_full()
                                    .h(px(51.))
                                    .flex_none()
                                    .relative()
                                    .px(px(13.))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .bg(p.raised)
                                    .border_b_1()
                                    .border_color(p.border)
                                    .child(crate::assets::icon("note").size(px(16.)))
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px(40.))
                                            .right(px(130.))
                                            .top(px(0.))
                                            .bottom(px(0.))
                                            .flex()
                                            .items_center()
                                            .child(
                                                Input::new(&self.note_title)
                                                    .id("note-title")
                                                    .with_size(Size::Small)
                                                    .appearance(false)
                                                    .bordered(false)
                                                    .w_full(),
                                            ),
                                    )
                                    .child(
                                        Select::new(&self.language)
                                            .id("note-language")
                                            .with_size(Size::Small)
                                            .absolute()
                                            .right(px(13.))
                                            .top(px(11.))
                                            .w(px(116.)),
                                    ),
                            )
                            .child(
                                div().flex_1().min_h_0().child(
                                    Editor::new(&self.note_editor)
                                        .aria_label("Note contents")
                                        .bg(p.inset)
                                        .h_full(),
                                ),
                            )
                            .child(
                                div()
                                    .h(px(31.))
                                    .flex_none()
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .border_t_1()
                                    .border_color(p.border_soft)
                                    .text_size(px(10.))
                                    .text_color(p.subtle)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.))
                                            .child(crate::assets::icon("lock").size(px(13.)))
                                            .child("Encrypted on your device before sharing."),
                                    )
                                    .child(format!("{note_characters} characters · UTF-8")),
                            )
                            .into_any_element()
                    }),
            )
            .child(footer);
        let heading =
            crate::views::page::heading(p, "Send", "Files and notes, shared on your terms.");
        let viewport_width = window.viewport_size().width;
        let sidebar_width = if viewport_width < px(1100.) {
            px(78.)
        } else {
            px(crate::theme::tokens::geometry::STUDIO_SIDEBAR)
        };
        let narrow = viewport_width - sidebar_width < px(920.);
        if narrow {
            crate::views::page::scroller(
                "send-page-scroll",
                &self.page_scroll,
                window,
                div()
                    .w_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(crate::views::page::HEADER_GAP))
                    .child(heading)
                    .child(
                        div().flex_none().flex().justify_end().child(
                            Button::new("open-sharing-preferences")
                                .label("Sharing preferences")
                                .outline()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_preferences_sheet(window, cx)
                                })),
                        ),
                    )
                    .child(composer),
            )
            .into_any_element()
        } else {
            let home = self.home.clone();
            crate::views::page::scroller(
                "send-page-scroll",
                &self.page_scroll,
                window,
                div()
                    .w_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(crate::views::page::HEADER_GAP))
                    .child(heading)
                    .child(
                        div().w_full().h(composer_height).flex_none().flex().child(
                            h_resizable("send-layout")
                                .with_state(&self.layout)
                                .with_handle_appearance(std::rc::Rc::new(|_, _, _| {
                                    Some(div().into_any_element())
                                }))
                                .on_resize(move |state, _, cx| {
                                    let Some(width) =
                                        state.read(cx).sizes().last().map(|size| size.as_f32())
                                    else {
                                        return;
                                    };
                                    let home = home.clone();
                                    cx.background_executor()
                                        .spawn(async move {
                                            let mut saved =
                                                crate::platform::preferences::load(&home);
                                            saved.inspector_width = width;
                                            let _ =
                                                crate::platform::preferences::save(&home, &saved);
                                        })
                                        .detach();
                                })
                                .child(resizable_panel().mr(px(10.)).child(composer))
                                .child(
                                    resizable_panel()
                                        .size(px(self.inspector_width))
                                        .size_range(px(260.)..px(420.))
                                        .ml(px(10.))
                                        .child(self.preferences(cx)),
                                ),
                        ),
                    ),
            )
            .into_any_element()
        }
    }
}

impl SendPanel {
    fn preferences(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette_for(Theme::global(cx).mode);
        let preferences = self.active_preferences();
        let policy = self.client.snapshot().policy;
        let selected_lifetime = preferences.lifetime.read(cx).selected_value().copied();
        let available_hours = self.lifetime_availability.read(cx).hours.clone();
        let lifetime_error = unavailable_lifetime_error(selected_lifetime, &available_hours);
        div()
            .size_full()
            .min_h_0()
            .overflow_y_scrollbar()
            .rounded(px(18.))
            .border_1()
            .border_color(p.border)
            .flex()
            .flex_col()
            .bg(p.surface)
            .child(
                div()
                    .px(px(18.))
                    .pt(px(20.))
                    .text_size(px(10.))
                    .text_color(p.subtle)
                    .child("FOR THIS TRANSFER"),
            )
            .child(
                div()
                    .px(px(18.))
                    .pt(px(5.))
                    .pb(px(15.))
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Sharing preferences"),
            )
            .child(
                div()
                    .px(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(11.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .text_size(px(12.))
                            .child("Password")
                            .child(div().text_color(p.subtle).child("optional")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div().flex_1().child(
                                    Input::new(&preferences.password)
                                        .id("transfer-password")
                                        .with_size(Size::Large)
                                        .h(px(38.)),
                                ),
                            )
                            .child(
                                Button::new("reveal-password")
                                    .icon(Icon::default().path(if preferences.reveal_password {
                                        "icons/eye-off.svg"
                                    } else {
                                        "icons/eye.svg"
                                    }))
                                    .outline()
                                    .with_size(Size::Medium)
                                    .tooltip(if preferences.reveal_password {
                                        "Hide password"
                                    } else {
                                        "Reveal password"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let revealed = !this.active_preferences().reveal_password;
                                        this.set_active_password_revealed(revealed, window, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(p.subtle)
                            .child("Share the password separately."),
                    )
                    .child(div().mt(px(4.)).text_size(px(12.)).child("Link lifetime"))
                    .child(
                        Select::new(&preferences.lifetime)
                            .id("transfer-lifetime")
                            .with_size(Size::Large)
                            .h(px(38.)),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(p.subtle)
                            .child(retention_label(&policy)),
                    )
                    .when_some(lifetime_error, |this, error| {
                        this.child(div().text_size(px(10.)).text_color(p.danger).child(error))
                    }),
            )
            .child(
                div()
                    .mx(px(18.))
                    .my(px(15.))
                    .border_t_1()
                    .border_color(p.border_soft),
            )
            .child(
                div()
                    .px(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(crate::assets::icon("key").size(px(14.)))
                                    .child(div().text_size(px(12.)).child("Include key in link")),
                            )
                            .child(
                                Switch::new("include-key")
                                    .checked(preferences.include_key)
                                    .on_click(cx.listener(|this, checked, _, cx| {
                                        this.active_preferences_mut().include_key = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(8.))
                                    .child(crate::assets::icon("bolt").size(px(14.)))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(div().text_size(px(12.)).child("Turbo Transfer"))
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(p.subtle)
                                                    .child("Share while the upload continues."),
                                            ),
                                    ),
                            )
                            .child(
                                Switch::new("turbo")
                                    .checked(preferences.turbo)
                                    .disabled(self.mode == ComposerMode::Notes)
                                    .on_click(cx.listener(|this, checked, _, cx| {
                                        this.active_preferences_mut().turbo = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        crate::assets::icon(if self.mode == ComposerMode::Files {
                                            "archive"
                                        } else {
                                            "lock"
                                        })
                                        .size(px(14.)),
                                    )
                                    .child(div().text_size(px(12.)).child(
                                        if self.mode == ComposerMode::Files {
                                            "Combine files into a ZIP"
                                        } else {
                                            "Burn on read"
                                        },
                                    )),
                            )
                            .child(
                                Switch::new("zip-or-burn")
                                    .checked(if self.mode == ComposerMode::Files {
                                        preferences.zip_directories
                                    } else {
                                        preferences.burn_on_read
                                    })
                                    .on_click(cx.listener(|this, checked, _, cx| {
                                        if this.mode == ComposerMode::Files {
                                            this.files.zip_directories = *checked;
                                        } else {
                                            this.notes.burn_on_read = *checked;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .mx(px(18.))
                    .my(px(15.))
                    .border_t_1()
                    .border_color(p.border_soft),
            )
            .child(
                div()
                    .px(px(18.))
                    .pb(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .text_size(px(12.))
                            .child("Deliver to username")
                            .child(div().text_color(p.subtle).child("optional")),
                    )
                    .child(
                        Input::new(&preferences.recipient)
                            .id("transfer-recipient")
                            .with_size(Size::Large)
                            .h(px(38.)),
                    )
                    .child(div().text_size(px(10.)).text_color(p.subtle).child(
                        "Account-protected delivery. Your draft is preserved if it is unavailable.",
                    )),
            )
            .child(
                div()
                    .mt_auto()
                    .px(px(18.))
                    .py(px(14.))
                    .rounded_b(px(18.))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(p.border)
                    .bg(p.raised)
                    .text_size(px(10.))
                    .text_color(p.muted)
                    .child(crate::assets::icon("shield").size(px(14.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .line_height(px(15.))
                            .child("Your settings stay with this draft when you switch views."),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComposerMode, PreferenceValues, SendPanel, Transport, build_command,
        build_command_for_policy, draft_has_content, duration_label, lifetime_choices,
        retention_hours, unavailable_lifetime_error,
    };
    use crate::{
        client::DesktopClient,
        model::{ClientCommand, DesktopSnapshot, DriverPolicy, PolicySnapshot, SendTransport},
    };
    use filebeam_client_config::{Appearance, Theme as AppearanceTheme};
    use gpui::test::TestWindowExt;
    use gpui::{AppContext as _, Focusable, TestAppContext, px, size};
    use std::{
        path::PathBuf,
        sync::{Arc, mpsc::Receiver},
    };

    fn panel(cx: &mut TestAppContext) -> (gpui::WindowHandle<SendPanel>, Receiver<ClientCommand>) {
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
        let (client, commands) = DesktopClient::test_client(DesktopSnapshot {
            policy: PolicySnapshot {
                http: DriverPolicy {
                    enabled: true,
                    ..DriverPolicy::default()
                },
                webrtc: DriverPolicy {
                    enabled: true,
                    ..DriverPolicy::default()
                },
                ..PolicySnapshot::default()
            },
            ..DesktopSnapshot::default()
        });
        let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
            SendPanel::new(
                Arc::new(client),
                PathBuf::from("/tmp/filebeam-ui-test"),
                window,
                cx,
            )
        });
        (handle, commands)
    }

    #[gpui::test]
    fn retained_file_and_note_drafts_survive_navigation_theme_changes_and_editor_undo(
        cx: &mut TestAppContext,
    ) {
        let (handle, commands) = panel(cx);
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("transfer-password", cx);
            window.input("file secret", cx);
            window.within("send-mode-tabs").click(1usize, cx);
            window.click("transfer-password", cx);
            window.input("note secret", cx);
        })
        .unwrap();
        // Kit 0.6.1's Editor exposes an accessible name, but no element ID for
        // pointer targeting. Focus its real InputState before native text input.
        let editor_focus = handle
            .update(cx, |panel, _, cx| {
                panel.note_editor.read(cx).focus_handle(cx)
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.focus(&editor_focus, cx);
            window.input("first line", cx);
            window.press("enter", cx);
            window.input("second line", cx);
            window.press(
                if cfg!(target_os = "macos") {
                    "cmd-z"
                } else {
                    "ctrl-z"
                },
                cx,
            );
        })
        .unwrap();
        handle
            .update(cx, |panel, _, cx| {
                assert_eq!(panel.files.password.read(cx).value(), "file secret");
                assert_eq!(panel.notes.password.read(cx).value(), "note secret");
                assert_eq!(panel.note_editor.read(cx).value(), "first line\n");
            })
            .unwrap();

        cx.update(|cx| {
            crate::theme::apply(
                &Appearance {
                    theme: AppearanceTheme::Light,
                    reduced_motion: true,
                },
                cx,
            );
        });
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.within("send-mode-tabs").find(1usize).selected(),
                Some(true)
            );
            window.within("send-mode-tabs").click(0usize, cx);
            assert_eq!(window.find("transfer-password").value(), None);
        })
        .unwrap();
        handle
            .update(cx, |panel, _, cx| {
                assert_eq!(panel.note_editor.read(cx).value(), "first line\n");
                assert_eq!(panel.files.password.read(cx).value(), "file secret");
            })
            .unwrap();
        assert!(commands.try_recv().is_err());
    }

    #[gpui::test]
    fn transport_radio_click_selects_the_controlled_value(cx: &mut TestAppContext) {
        let (handle, commands) = panel(cx);
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.within("send-transport").find(0usize).checked(),
                Some(true)
            );
            window.within("send-transport").click(1usize, cx);
            window.render_frame(cx);
            assert_eq!(
                window.within("send-transport").find(1usize).checked(),
                Some(true)
            );
            window.within("send-transport").click(0usize, cx);
            window.render_frame(cx);
            assert_eq!(
                window.within("send-transport").find(0usize).checked(),
                Some(true)
            );
        })
        .unwrap();
        handle
            .update(cx, |panel, _, _| {
                assert!(panel.files.transport == Transport::Http)
            })
            .unwrap();
        assert!(commands.try_recv().is_err());
    }

    #[gpui::test]
    fn empty_and_incompatible_send_controls_never_enqueue_and_keep_the_draft(
        cx: &mut TestAppContext,
    ) {
        let (handle, commands) = panel(cx);
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("encrypt-share", cx);
        })
        .unwrap();
        assert!(commands.try_recv().is_err());

        handle
            .update(cx, |panel, _, cx| {
                panel.append_paths(vec![PathBuf::from("report.pdf")], cx)
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("transfer-recipient", cx);
            window.input("alice", cx);
            window.within("send-transport").click(1usize, cx);
            window.click("encrypt-share", cx);
        })
        .unwrap();
        handle
            .update(cx, |panel, _, cx| {
                assert_eq!(panel.paths, vec![PathBuf::from("report.pdf")]);
                assert_eq!(
                    panel.files.lifetime.read(cx).selected_value().copied(),
                    Some(None)
                );
                assert_eq!(panel.files.recipient.read(cx).value(), "alice");
            })
            .unwrap();
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn empty_drafts_cannot_be_dispatched() {
        assert!(!draft_has_content(ComposerMode::Files, &[], ""));
        assert!(!draft_has_content(ComposerMode::Notes, &[], "  \n"));
        assert!(draft_has_content(
            ComposerMode::Files,
            &[PathBuf::from("report.pdf")],
            ""
        ));
        assert!(draft_has_content(ComposerMode::Notes, &[], "private note"));
    }

    #[test]
    fn invalid_lifetime_is_rejected_instead_of_becoming_default() {
        assert_eq!(retention_hours("24"), Ok(Some(24)));
        assert_eq!(retention_hours(""), Ok(None));
        assert!(retention_hours("0").is_err());
        assert!(retention_hours("tomorrow").is_err());
    }

    #[test]
    fn unavailable_lifetime_error_tracks_current_policy_choices() {
        assert_eq!(unavailable_lifetime_error(Some(None), &[1, 24]), None);
        assert_eq!(unavailable_lifetime_error(Some(Some(24)), &[1, 24]), None);
        assert_eq!(
            unavailable_lifetime_error(Some(Some(24)), &[1, 168]),
            Some("Lifetime is not available on this instance.".into())
        );
    }

    #[test]
    fn removed_default_lifetime_is_rejected_like_the_transfer_worker() {
        let policy = crate::model::PolicySnapshot {
            availability: crate::model::PolicyAvailability::Available,
            anonymous_uploads: true,
            default_retention_hours: Some(24),
            retention_options_hours: vec![1, 168],
            http: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            webrtc: crate::model::DriverPolicy {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut values = preferences("", "");
        values.lifetime = "24".into();
        assert!(
            build_command_for_policy(
                ComposerMode::Files,
                vec!["a.txt".into()],
                String::new(),
                None,
                "plain".into(),
                values,
                &policy,
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn lifetime_choices_are_human_readable_and_preserve_removed_selection() {
        let policy = crate::model::PolicySnapshot {
            default_retention_hours: Some(24),
            retention_options_hours: vec![168, 1, 24, 24],
            ..Default::default()
        };
        let choices = lifetime_choices(&policy, Some(Some(6)));
        assert_eq!(duration_label(1), "1 hour");
        assert_eq!(duration_label(24), "1 day");
        assert_eq!(duration_label(168), "1 week");
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Default (1 day)",
                "1 hour",
                "1 day",
                "1 week",
                "6 hours (unavailable)",
            ]
        );
        assert_eq!(choices.last().unwrap().hours, Some(6));
    }

    #[test]
    fn lifetime_choices_do_not_invent_custom_durations_without_policy_values() {
        let policy = crate::model::PolicySnapshot {
            default_retention_hours: Some(48),
            ..Default::default()
        };
        let choices = lifetime_choices(&policy, None);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].label, "Default (2 days)");
        assert_eq!(choices[0].hours, None);
    }

    fn preferences(password: &str, recipient: &str) -> PreferenceValues {
        PreferenceValues {
            password: password.into(),
            recipient: recipient.into(),
            lifetime: "12".into(),
            transport: Transport::Http,
            include_key: true,
            turbo: false,
            zip_directories: true,
            burn_on_read: false,
        }
    }

    #[test]
    fn password_whitespace_is_an_intentional_password() {
        let command = build_command(
            ComposerMode::Files,
            vec!["a.txt".into()],
            String::new(),
            None,
            "plain".into(),
            preferences("        ", ""),
        )
        .unwrap();
        let ClientCommand::SendFiles(request) = command else {
            panic!("expected file request")
        };
        assert_eq!(request.password.as_deref(), Some("        "));
    }

    #[test]
    fn recipient_drafts_are_not_silently_dropped() {
        assert!(
            build_command(
                ComposerMode::Notes,
                vec![],
                "note".into(),
                None,
                "plain".into(),
                preferences("", "alice")
            )
            .is_err()
        );
        let mut incompatible = preferences("", "alice");
        incompatible.transport = Transport::Live;
        assert!(
            build_command(
                ComposerMode::Files,
                vec!["a.txt".into()],
                String::new(),
                None,
                "plain".into(),
                incompatible
            )
            .is_err()
        );
        assert!(
            build_command(
                ComposerMode::Files,
                vec!["a.txt".into()],
                String::new(),
                None,
                "plain".into(),
                preferences("secret", "alice"),
            )
            .is_err()
        );
    }

    #[test]
    fn mode_values_remain_independent_when_building_requests() {
        let files = preferences("file secret", "");
        let mut notes = preferences("note secret", "");
        notes.transport = Transport::Live;
        let ClientCommand::SendFiles(file) = build_command(
            ComposerMode::Files,
            vec!["a.txt".into()],
            String::new(),
            None,
            "plain".into(),
            files,
        )
        .unwrap() else {
            panic!("expected file request")
        };
        let ClientCommand::CreateNote(note) = build_command(
            ComposerMode::Notes,
            vec![],
            "note".into(),
            None,
            "plain".into(),
            notes,
        )
        .unwrap() else {
            panic!("expected note request")
        };
        assert_eq!(file.password.as_deref(), Some("file secret"));
        assert!(matches!(file.transport, SendTransport::Http));
        assert_eq!(note.password.as_deref(), Some("note secret"));
    }
}
