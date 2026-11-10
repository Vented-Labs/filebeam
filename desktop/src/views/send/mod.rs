//! The Send composer. It owns drafts, picker state and the share in progress;
//! rendering only projects that state.

mod compose;
mod view;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use gpui::{
    AnyWindowHandle, AppContext, ClipboardItem, Context, Entity, ExternalPaths, PathPromptOptions,
    ScrollHandle, Window,
};
use gpui_component::{
    IndexPath,
    input::{EditorState, InputEvent, InputState},
    select::{SelectEvent, SelectState},
};

use self::compose::{
    LifetimeChoice, PreferenceValues, build_command_for_policy, draft_has_content, lifetime_choices,
};
use crate::{
    client::DesktopClient,
    live::Live,
    model::{
        ClientCommand, DesktopSnapshot, PolicyAvailability, PolicySnapshot, SendTransport,
        TransferDirection,
    },
    ui::format,
};

/// Languages offered for notes. `plain` is the wire value for no highlighting.
const LANGUAGES: [&str; 9] = [
    "plain",
    "php",
    "dotenv",
    "javascript",
    "typescript",
    "json",
    "markdown",
    "css",
    "html",
];
/// The attached-note limit enforced by the transfer core, in UTF-8 bytes.
const ATTACHED_NOTE_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposerMode {
    Files,
    Notes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transport {
    Http,
    Live,
}

/// What is known about a chosen path once it has been inspected off the UI thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FileDetail {
    size: Option<u64>,
    directory: bool,
}

/// The share started from this composer, from dispatch to its link.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Share {
    /// Queued for the service worker, which has not created the job yet.
    Starting {
        known_jobs: HashSet<String>,
        after_message: u64,
        direction: TransferDirection,
    },
    /// Bound to a job in the service snapshot.
    Job(String),
    /// Handed to the local device service for delivery to another device.
    Device(String),
    /// The service refused the request before a job existed.
    Rejected(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copied {
    Link,
    Key,
    Command,
}

/// The retained composer.
pub struct SendPanel {
    attach_note: bool,
    attached_title: Entity<InputState>,
    attached_editor: Entity<EditorState>,
    attached_language: Entity<SelectState<Vec<&'static str>>>,
    client: Arc<DesktopClient>,
    live: Entity<Live>,
    window_handle: AnyWindowHandle,
    mode: ComposerMode,
    paths: Vec<PathBuf>,
    details: HashMap<PathBuf, FileDetail>,
    files: Preferences,
    notes: Preferences,
    note_title: Entity<InputState>,
    note_editor: Entity<EditorState>,
    language: Entity<SelectState<Vec<&'static str>>>,
    lifetime_hours: Vec<u64>,
    page_scroll: ScrollHandle,
    status: Option<String>,
    preparing_paths: bool,
    selected_device: Option<String>,
    share: Option<Share>,
    copied: Option<Copied>,
    copy_generation: u64,
    dropzone_hovered: bool,
}

/// Input entities deliberately belong to a mode, so swapping composers never
/// replaces an editor/input entity or merges two transfers' preferences.
struct Preferences {
    password: Entity<InputState>,
    recipient: Entity<InputState>,
    chosen_contact: Option<(String, u64)>,
    lifetime: Entity<SelectState<Vec<LifetimeChoice>>>,
    lifetime_choices: Vec<LifetimeChoice>,
    transport: Transport,
    include_key: bool,
    turbo: bool,
    zip_directories: bool,
    burn_on_read: bool,
    transport_edited: bool,
}

impl Preferences {
    fn new(policy: &PolicySnapshot, window: &mut Window, cx: &mut Context<SendPanel>) -> Self {
        let lifetime_choices = lifetime_choices(policy, None);
        let select_choices = lifetime_choices.clone();
        Self {
            password: cx.new(|cx| InputState::new(window, cx).placeholder("None").masked(true)),
            recipient: cx.new(|cx| InputState::new(window, cx).placeholder("username")),
            chosen_contact: None,
            lifetime: cx.new(move |cx| {
                SelectState::new(select_choices, Some(IndexPath::default()), window, cx)
            }),
            lifetime_choices,
            transport: Transport::Http,
            include_key: true,
            turbo: false,
            zip_directories: true,
            burn_on_read: false,
            transport_edited: false,
        }
    }
}

impl PreferenceValues {
    fn from_inputs(preferences: &Preferences, cx: &Context<SendPanel>) -> Self {
        let value = |input: &Entity<InputState>| input.read(cx).value().to_string();
        Self {
            password: value(&preferences.password),
            recipient: value(&preferences.recipient),
            expected_recipient_id: preferences
                .chosen_contact
                .as_ref()
                .filter(|(name, _)| name == value(&preferences.recipient).trim())
                .map(|(_, id)| *id),
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

impl SendPanel {
    pub fn new(live: Entity<Live>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client = live.read(cx).client().clone();
        let policy = live.read(cx).snapshot().policy.clone();
        let files = Preferences::new(&policy, window, cx);
        let notes = Preferences::new(&policy, window, cx);
        let note_title = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled note"));
        let note_editor = cx.new(|cx| EditorState::new(window, cx).language("text"));
        let attached_title =
            cx.new(|cx| InputState::new(window, cx).placeholder("Title (optional)"));
        let attached_editor = cx.new(|cx| EditorState::new(window, cx).language("text"));
        let language_select = |window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                SelectState::new(LANGUAGES.to_vec(), Some(IndexPath::default()), window, cx)
            })
        };
        let attached_language = language_select(window, cx);
        let language = language_select(window, cx);

        cx.observe(&live, |this, live, cx| {
            let snapshot = live.read(cx).snapshot();
            this.on_snapshot(&snapshot, cx);
        })
        .detach();

        let mut panel = Self {
            attach_note: false,
            attached_title,
            attached_editor,
            attached_language,
            client,
            live,
            window_handle: window.window_handle(),
            mode: ComposerMode::Files,
            paths: Vec::new(),
            details: HashMap::new(),
            files,
            notes,
            note_title,
            note_editor,
            language,
            lifetime_hours: policy.retention_options_hours.clone(),
            page_scroll: ScrollHandle::new(),
            status: None,
            preparing_paths: false,
            selected_device: None,
            share: None,
            copied: None,
            copy_generation: 0,
            dropzone_hovered: false,
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
        cx.subscribe(&self.attached_editor, |this, _, _: &InputEvent, cx| {
            this.status = None;
            cx.notify();
        })
        .detach();
        for (select, editor) in [
            (&self.attached_language, self.attached_editor.clone()),
            (&self.language, self.note_editor.clone()),
        ] {
            cx.subscribe(
                select,
                move |_, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                    if let SelectEvent::Confirm(Some(language)) = event {
                        // `plain` remains the wire value; the editor registry calls its
                        // neutral highlighter `plaintext`.
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
    }

    /// Follows the share this composer started as the service reports on it.
    fn on_snapshot(&mut self, snapshot: &DesktopSnapshot, cx: &mut Context<Self>) {
        match &self.share {
            Some(Share::Starting {
                known_jobs,
                after_message,
                direction,
            }) => {
                if let Some(job) = snapshot
                    .jobs
                    .iter()
                    .find(|job| job.direction == *direction && !known_jobs.contains(&job.id))
                {
                    self.share = Some(Share::Job(job.id.clone()));
                } else if let Some(message) = snapshot.messages.iter().find(|message| {
                    message.id > *after_message
                        && matches!(message.operation.as_str(), "share" | "note")
                }) {
                    self.share = Some(Share::Rejected(format::readable_error(
                        &message.error.detail,
                    )));
                }
            }
            // A discarded transfer has nothing left to present.
            Some(Share::Job(id)) if !snapshot.jobs.iter().any(|job| &job.id == id) => {
                self.share = None;
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn append_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut added = Vec::new();
        for path in paths {
            if !self.paths.contains(&path) {
                self.paths.push(path.clone());
                added.push(path);
            }
        }
        self.preparing_paths = false;
        self.status = None;
        // A new selection is a new draft: leave any finished share behind.
        if matches!(self.share, Some(Share::Rejected(_))) {
            self.share = None;
        }
        self.inspect_paths(added, cx);
        cx.notify();
    }

    /// Reads sizes off the UI thread so rows can show them without blocking a frame.
    fn inspect_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let details = executor
                .spawn(async move {
                    paths
                        .into_iter()
                        .map(|path| {
                            let detail = std::fs::metadata(&path).map_or_else(
                                |_| FileDetail::default(),
                                |metadata| FileDetail {
                                    size: metadata.is_file().then_some(metadata.len()),
                                    directory: metadata.is_dir(),
                                },
                            );
                            (path, detail)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                for (path, detail) in details {
                    if this.paths.contains(&path) {
                        this.details.insert(path, detail);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn remove_path(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.paths.len() {
            let path = self.paths.remove(index);
            self.details.remove(&path);
            self.status = None;
            cx.notify();
        }
    }

    fn clear_paths(&mut self, cx: &mut Context<Self>) {
        self.paths.clear();
        self.details.clear();
        self.status = None;
        cx.notify();
    }

    /// The combined size of the chosen files, when every one of them is known.
    fn total_size(&self) -> Option<u64> {
        self.paths
            .iter()
            .map(|path| self.details.get(path).and_then(|detail| detail.size))
            .sum()
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
                Ok(Err(error)) => {
                    this.preparing_paths = false;
                    this.status = None;
                    cx.notify();
                    let detail = error.to_string();
                    let _ = window_handle.update(cx, |_, window, cx| {
                        crate::toast::error("Could not open the file picker", detail, window, cx);
                    });
                }
                Ok(Ok(None)) | Err(_) => {
                    this.preparing_paths = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.is_sharing() {
            return;
        }
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
        if self.mode == ComposerMode::Files
            && let Some(target) = self.selected_device.clone()
        {
            match self.client.dispatch(ClientCommand::EnqueueDeviceSend {
                target: target.clone(),
                sources: self.paths.clone(),
            }) {
                Ok(()) => {
                    let name = self
                        .live
                        .read(cx)
                        .snapshot()
                        .device_directory
                        .devices
                        .iter()
                        .find(|device| device.id == target)
                        .map_or(target, |device| device.name.clone());
                    self.status = None;
                    self.share = Some(Share::Device(name));
                }
                Err(error) => self.status = Some(format::readable_error(&error.to_string())),
            }
            cx.notify();
            return;
        }
        let preferences = self.active_preferences();
        let snapshot = self.live.read(cx).snapshot();
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
        let direction = match command {
            ClientCommand::CreateNote(_) => TransferDirection::Note,
            _ => TransferDirection::Send,
        };
        // The worker reads its queue in order, so the job that appears next with this
        // direction is the one created here.
        let current = self.client.snapshot();
        match self.client.dispatch(command) {
            Ok(()) => {
                self.status = None;
                self.share = Some(Share::Starting {
                    known_jobs: current.jobs.iter().map(|job| job.id.clone()).collect(),
                    after_message: current.messages.last().map_or(0, |message| message.id),
                    direction,
                });
            }
            Err(error) => {
                let detail = format::readable_error(&error.to_string());
                let _ = self.window_handle.update(cx, |_, window, cx| {
                    crate::toast::error("Could not share", detail, window, cx);
                });
            }
        }
        cx.notify();
    }

    /// Whether a share is on screen in place of the composer.
    fn is_sharing(&self) -> bool {
        self.share.is_some()
    }

    /// Returns to the composer with the draft exactly as it was.
    fn back_to_draft(&mut self, cx: &mut Context<Self>) {
        self.share = None;
        self.copied = None;
        cx.notify();
    }

    /// Clears the content of the draft that was just shared. Preferences such as the
    /// transport and toggles are kept for the next transfer.
    fn new_transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.share = None;
        self.copied = None;
        self.status = None;
        self.attach_note = false;
        self.selected_device = None;
        self.paths.clear();
        self.details.clear();
        for input in [
            &self.note_title,
            &self.attached_title,
            &self.files.password,
            &self.files.recipient,
            &self.notes.password,
            &self.notes.recipient,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        for editor in [&self.note_editor, &self.attached_editor] {
            editor.update(cx, |editor, cx| editor.set_value("", window, cx));
        }
        self.files.chosen_contact = None;
        cx.notify();
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

    fn optional_text(&self, input: &Entity<InputState>, cx: &Context<Self>) -> Option<String> {
        let value = input.read(cx).value().trim().to_owned();
        (!value.is_empty()).then_some(value)
    }

    fn can_submit(&self, cx: &Context<Self>) -> bool {
        draft_has_content(self.mode, &self.paths, &self.note_editor.read(cx).value())
    }

    /// Chooses a device target, or returns to link sharing with `None`.
    fn select_device(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.selected_device = id;
        self.status = None;
        cx.notify();
    }

    fn set_mode(&mut self, mode: ComposerMode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.status = None;
            cx.notify();
        }
    }

    fn set_transport(&mut self, transport: Transport, cx: &mut Context<Self>) {
        let preferences = self.active_preferences_mut();
        preferences.transport = transport;
        preferences.transport_edited = true;
        self.status = None;
        cx.notify();
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
        self.lifetime_hours = policy.retention_options_hours.clone();
        cx.notify();
    }

    fn receive_drop(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.append_paths(paths.paths().to_vec(), cx);
    }
}

#[cfg(feature = "visual-test")]
impl SendPanel {
    pub fn show_notes_for_capture(&mut self, cx: &mut Context<Self>) {
        self.mode = ComposerMode::Notes;
        cx.notify();
    }

    /// Seeds a visual-only retained note. The shell calls this after switching modes because
    /// native input and editor setters require the active window.
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

    /// Seeds chosen files with known sizes, without touching the filesystem.
    pub fn seed_files_for_capture(&mut self, attach_note: bool, cx: &mut Context<Self>) {
        let files: [(&str, Option<u64>, bool); 5] = [
            ("/fixture/Launch assets.zip", Some(342_884_352), false),
            ("/fixture/Product walkthrough.mp4", Some(482_344_960), false),
            ("/fixture/Brand guidelines.pdf", Some(3_758_080), false),
            ("/fixture/release-notes.md", Some(1_946), false),
            ("/fixture/Photography", None, true),
        ];
        for (path, size, directory) in files {
            let path = PathBuf::from(path);
            self.details
                .insert(path.clone(), FileDetail { size, directory });
            self.paths.push(path);
        }
        self.attach_note = attach_note;
        cx.notify();
    }

    /// Shows the share view for a job that already exists in the fixture snapshot.
    pub fn seed_share_for_capture(&mut self, job_id: &str, cx: &mut Context<Self>) {
        self.share = Some(Share::Job(job_id.into()));
        cx.notify();
    }

    pub fn seed_status_for_capture(&mut self, status: &str, cx: &mut Context<Self>) {
        self.status = Some(status.into());
        cx.notify();
    }

    /// Focuses the native trigger. The capture driver activates it with Space or Enter because
    /// GPUI Kit intentionally keeps SelectState's menu-open operation private.
    pub fn open_lifetime_dropdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lifetime = self.active_preferences().lifetime.clone();
        lifetime.update(cx, |state, cx| state.focus(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{Arc, mpsc::Receiver},
    };

    use filebeam_client_config::{Appearance, Theme as AppearanceTheme};
    use gpui::{AppContext as _, Focusable, TestAppContext, px, size, test::TestWindowExt};

    use super::{ComposerMode, SendPanel, Share, Transport};
    use crate::{
        client::DesktopClient,
        live::Live,
        model::{
            ClientCommand, ClientError, ClientErrorCode, ClientMessage, DesktopSnapshot,
            DriverPolicy, JobSnapshot, PolicyAvailability, PolicySnapshot, TransferCapabilities,
            TransferDirection, TransferProgress, TransferState,
        },
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
        // An available policy keeps the share action enabled, so these tests reach
        // the request-building rules rather than a disabled button.
        let (client, commands) = DesktopClient::test_client(DesktopSnapshot {
            policy: PolicySnapshot {
                availability: PolicyAvailability::Available,
                anonymous_uploads: true,
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
        let handle = cx.open_window(size(px(1280.), px(820.)), move |window, cx| {
            let live = cx.new(|cx| Live::new(Arc::new(client), cx));
            SendPanel::new(live, window, cx)
        });
        (handle, commands)
    }

    fn job(id: &str, direction: TransferDirection) -> JobSnapshot {
        JobSnapshot {
            id: id.into(),
            direction,
            state: TransferState::Running,
            progress: TransferProgress::default(),
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
                window.within("send-mode-tabs").find(1usize).checked(),
                Some(true)
            );
            window.within("send-mode-tabs").click(0usize, cx);
            assert_eq!(window.find("transfer-password").value(), None);
        })
        .unwrap();
        handle
            .update(cx, |panel, _, cx| {
                assert!(panel.mode == ComposerMode::Files);
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
        })
        .unwrap();
        // Editing clears the status line once its change event is delivered, so the
        // share is attempted in its own update, as a later click would be.
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
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
                assert!(panel.share.is_none());
                assert!(panel.status.is_some());
            })
            .unwrap();
        assert!(commands.try_recv().is_err());
    }

    #[gpui::test]
    fn a_share_follows_the_job_it_started_and_keeps_the_draft(cx: &mut TestAppContext) {
        let (handle, commands) = panel(cx);
        let client = handle
            .update(cx, |panel, _, _| panel.client.clone())
            .unwrap();
        handle
            .update(cx, |panel, _, cx| {
                panel.append_paths(vec![PathBuf::from("report.pdf")], cx);
                panel.submit(cx);
                assert!(matches!(panel.share, Some(Share::Starting { .. })));
            })
            .unwrap();
        assert!(matches!(
            commands.try_recv(),
            Ok(ClientCommand::SendFiles(_))
        ));

        // An unrelated receive must not be mistaken for the share.
        let mut snapshot = client.snapshot();
        snapshot.jobs.push(job("other", TransferDirection::Receive));
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert!(matches!(panel.share, Some(Share::Starting { .. })));
            })
            .unwrap();

        snapshot.jobs.push(job("mine", TransferDirection::Send));
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert_eq!(panel.share, Some(Share::Job("mine".into())));
                assert_eq!(panel.paths, vec![PathBuf::from("report.pdf")]);
            })
            .unwrap();

        // Discarding the transfer elsewhere returns to the untouched draft.
        snapshot.jobs.clear();
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert!(panel.share.is_none());
                assert_eq!(panel.paths, vec![PathBuf::from("report.pdf")]);
            })
            .unwrap();
    }

    #[gpui::test]
    fn a_refused_share_is_explained_in_place(cx: &mut TestAppContext) {
        let (handle, _commands) = panel(cx);
        let client = handle
            .update(cx, |panel, _, _| panel.client.clone())
            .unwrap();
        handle
            .update(cx, |panel, _, cx| {
                panel.append_paths(vec![PathBuf::from("report.pdf")], cx);
                panel.submit(cx);
            })
            .unwrap();
        let mut snapshot = client.snapshot();
        snapshot.messages.push(ClientMessage {
            id: 9,
            operation: "share".into(),
            error: ClientError {
                code: ClientErrorCode::Remote,
                detail: "The server returned 503.".into(),
            },
        });
        handle
            .update(cx, |panel, _, cx| {
                panel.on_snapshot(&snapshot, cx);
                assert_eq!(
                    panel.share,
                    Some(Share::Rejected(
                        "The server had a problem. Try again shortly.".into()
                    ))
                );
                panel.back_to_draft(cx);
                assert!(panel.share.is_none());
                assert_eq!(panel.paths, vec![PathBuf::from("report.pdf")]);
            })
            .unwrap();
    }

    #[gpui::test]
    fn removing_files_keeps_details_in_step(cx: &mut TestAppContext) {
        let (handle, _commands) = panel(cx);
        handle
            .update(cx, |panel, _, cx| {
                panel.append_paths(vec!["a.txt".into(), "b.txt".into(), "a.txt".into()], cx);
                assert_eq!(panel.paths.len(), 2, "a path is only added once");
                panel.remove_path(0, cx);
                assert_eq!(panel.paths, vec![PathBuf::from("b.txt")]);
                panel.remove_path(7, cx);
                assert_eq!(panel.paths.len(), 1);
                panel.clear_paths(cx);
                assert!(panel.paths.is_empty() && panel.details.is_empty());
                assert_eq!(panel.total_size(), Some(0));
            })
            .unwrap();
    }
}
