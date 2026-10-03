use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, mpsc},
    time::Instant,
};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroizing;

use crate::{
    app::{Cancelled, Job, Prompt, PromptKind, Request, TransferEvent, TransferView, subsequence},
    config::Config,
    input::Input,
    presentation::{Theme, clean},
    protocol::{self, Info, SavedTransfer},
    services,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeAction {
    NoteCreate,
    NoteOpen,
    InboxList,
    InboxDownload,
    HistoryList,
    HistoryDelete,
    HistoryExtend,
    Recipient,
    Revoke,
    EndLive,
    Login,
    Register,
    Logout,
    Profile,
    ResendVerification,
    Verify,
    RecoveryRequest,
    RecoveryReset,
    KeySelf,
    KeyPassword,
    KeyImport,
    KeyExport,
    Contacts,
    ContactAction,
    ReceivingDefaults,
    InboxSync,
    InboxStaged,
    InboxSave,
    InboxDismiss,
}

impl NativeAction {
    pub const ALL: [Self; 29] = [
        Self::NoteCreate,
        Self::NoteOpen,
        Self::InboxList,
        Self::InboxDownload,
        Self::HistoryList,
        Self::HistoryDelete,
        Self::HistoryExtend,
        Self::Recipient,
        Self::Revoke,
        Self::EndLive,
        Self::Login,
        Self::Register,
        Self::Logout,
        Self::Profile,
        Self::ResendVerification,
        Self::Verify,
        Self::RecoveryRequest,
        Self::RecoveryReset,
        Self::KeySelf,
        Self::KeyPassword,
        Self::KeyImport,
        Self::KeyExport,
        Self::Contacts,
        Self::ContactAction,
        Self::ReceivingDefaults,
        Self::InboxSync,
        Self::InboxStaged,
        Self::InboxSave,
        Self::InboxDismiss,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Contacts => "Contacts: list friends and requests",
            Self::ContactAction => "Contacts: request, accept, block, or set permissions",
            Self::ReceivingDefaults => "Account: receiving defaults",
            Self::InboxSync => "Inbox: automatically stage eligible deliveries",
            Self::InboxStaged => "Inbox: list privately staged files",
            Self::InboxSave => "Inbox: verify and save staged files",
            Self::InboxDismiss => "Inbox: remove local staging",
            Self::NoteCreate => "Note: create",
            Self::NoteOpen => "Note: open",
            Self::InboxList => "Inbox: list",
            Self::InboxDownload => "Inbox: download",
            Self::HistoryList => "History: list",
            Self::HistoryDelete => "History: delete transfer",
            Self::HistoryExtend => "History: extend retention",
            Self::Recipient => "Recipient: resolve",
            Self::Revoke => "Transfer: revoke",
            Self::EndLive => "Transfer: end live",
            Self::Login => "Account: login",
            Self::Register => "Account: register",
            Self::Logout => "Account: logout",
            Self::Profile => "Account: profile",
            Self::ResendVerification => "Account: resend verification",
            Self::Verify => "Account: verify email",
            Self::RecoveryRequest => "Account: recovery request",
            Self::RecoveryReset => "Account: recovery reset",
            Self::KeySelf => "Custody: create self key",
            Self::KeyPassword => "Custody: create password key",
            Self::KeyImport => "Custody: import key",
            Self::KeyExport => "Custody: export key",
        }
    }
    pub fn fields(self) -> &'static [(&'static str, bool)] {
        match self {
            Self::ContactAction => &[
                ("@username", false),
                (
                    "request / accept / decline / cancel / remove / block / unblock / preferences",
                    false,
                ),
                ("can send: inherit / allow / deny", false),
                ("auto download: inherit / allow / deny", false),
            ],
            Self::ReceivingDefaults => &[
                ("anyone / authenticated / friends / nobody", false),
                ("auto download friends: on / off", false),
            ],
            Self::InboxSave => &[("delivery id", false), ("output folder", false)],
            Self::InboxDismiss => &[("delivery id", false)],
            Self::NoteCreate => &[
                ("title (optional)", false),
                ("language", false),
                ("note text", false),
                ("password (optional)", true),
                ("options: burn / live (space-separated)", false),
            ],
            Self::NoteOpen => &[("link", false), ("password (optional)", true)],
            Self::InboxDownload => &[("delivery id", false), ("output folder", false)],
            Self::HistoryList => &[("cursor (optional)", false)],
            Self::HistoryDelete => &[
                ("server transfer id", false),
                ("type DELETE to remove for everyone", false),
            ],
            Self::HistoryExtend => &[
                ("server transfer id", false),
                ("total retention hours", false),
            ],
            Self::Recipient => &[("username", false)],
            Self::Revoke | Self::EndLive => &[("transfer id", false)],
            Self::Login => &[("email", false), ("password", true)],
            Self::Register => &[
                ("username", false),
                ("email", false),
                ("name (optional)", false),
                ("password", true),
            ],
            Self::Verify => &[("complete verification link", false)],
            Self::RecoveryRequest => &[("email", false)],
            Self::RecoveryReset => &[
                ("email", false),
                ("recovery token", true),
                ("new password", true),
            ],
            Self::KeySelf => &[("type REPLACE to replace", false)],
            Self::KeyPassword => &[
                ("custody password", true),
                ("type REPLACE to replace", false),
            ],
            Self::KeyImport => &[
                ("owner-only key file", false),
                ("type REPLACE to replace", false),
            ],
            Self::KeyExport => &[("type EXPORT to reveal the key", false)],
            _ => &[],
        }
    }

    fn completed(self) -> &'static str {
        match self {
            Self::Contacts => "Contacts loaded",
            Self::ContactAction => "Contact updated",
            Self::ReceivingDefaults => "Receiving defaults saved",
            Self::InboxSync => "Eligible deliveries staged",
            Self::InboxStaged => "Staged files loaded",
            Self::InboxSave => "Staged files verified and saved",
            Self::InboxDismiss => "Local staging removed",
            Self::NoteCreate => "Note created",
            Self::NoteOpen => "Note opened",
            Self::InboxList => "Inbox loaded",
            Self::InboxDownload => "Delivery downloaded",
            Self::HistoryList => "History loaded",
            Self::HistoryDelete => "Deletion scheduled",
            Self::HistoryExtend => "Retention updated",
            Self::Recipient => "Recipient found",
            Self::Revoke => "Transfer revoked",
            Self::EndLive => "Live transfer ended",
            Self::Login => "Signed in",
            Self::Register => "Account created",
            Self::Logout => "Signed out",
            Self::Profile => "Profile loaded",
            Self::ResendVerification => "Verification email requested",
            Self::Verify => "Email verified",
            Self::RecoveryRequest => "Recovery email requested",
            Self::RecoveryReset => "Password reset",
            Self::KeySelf | Self::KeyPassword => "Receiving key created",
            Self::KeyImport => "Receiving key imported",
            Self::KeyExport => "Receiving key exported",
        }
    }
}

pub struct NativeForm {
    pub action: NativeAction,
    pub fields: Vec<Input>,
    pub field: usize,
}

impl NativeForm {
    fn note() -> Self {
        let mut fields = NativeAction::NoteCreate
            .fields()
            .iter()
            .map(|_| Input::default())
            .collect::<Vec<_>>();
        fields[2] = Input::multiline();
        Self {
            action: NativeAction::NoteCreate,
            fields,
            field: 2,
        }
    }
}

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub directory: bool,
}

impl Entry {
    pub fn kind(&self) -> &'static str {
        if self.directory {
            return "DIR";
        }
        match self
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" | "jpg" | "jpeg" | "webp" | "svg" | "gif" => "IMG",
            "zip" | "gz" | "tar" | "7z" | "rar" => "ZIP",
            "mp4" | "mov" | "webm" => "VID",
            "mp3" | "wav" | "flac" => "AUD",
            "pdf" => "PDF",
            "rs" | "ts" | "js" | "php" | "json" | "toml" | "sh" => "CODE",
            "txt" | "md" | "log" => "TXT",
            _ => "FILE",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Send,
    Receive,
    Transfers,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Browser,
    Queue,
    Action,
    Link,
    Destination,
    Jobs,
}

pub struct Receipt {
    pub result: Result<Vec<String>, String>,
    pub cancelled: bool,
}

pub struct State {
    pub attachment_mode: bool,
    pub attachment: NativeForm,
    pub received_attachment: Option<protocol::AttachedNote>,
    pub attachment_preview: bool,
    pub attachment_scroll: u16,
    pub send_notes: bool,
    pub note: NativeForm,
    pub note_editing: bool,
    clipboard_result: Option<mpsc::Receiver<Result<crate::clipboard::Content>>>,
    clipboard_files: BTreeMap<PathBuf, tempfile::NamedTempFile>,
    clipboard_directory: tempfile::TempDir,
    pub directory: PathBuf,
    pub entries: Vec<Entry>,
    pub filtered: Vec<usize>,
    pub cursor: usize,
    pub scroll: usize,
    pub queue_cursor: usize,
    pub queue_scroll: usize,
    pub selected: BTreeMap<PathBuf, Entry>,
    pub filter: Input,
    pub searching: bool,
    pub hidden: bool,
    pub mode: Mode,
    pub focus: Focus,
    pub link: Input,
    pub destination: Input,
    pub prompt: Option<Prompt>,
    pub secret: Input,
    pub job: Option<Job>,
    pub transfer: Option<TransferView>,
    pub receipt: Option<Receipt>,
    pub receipt_scroll: u16,
    pub hyperlink: Option<crate::output::Hyperlink>,
    pub help: bool,
    pub notice: Option<(String, Instant)>,
    pub results: Vec<String>,
    pub info: Option<Result<Info, String>>,
    pub saved_transfers: Vec<SavedTransfer>,
    pub instance: String,
    pub config: Config,
    pub now: Instant,
    pub launched: Instant,
    pub palette: bool,
    pub palette_cursor: usize,
    pub native: Option<NativeForm>,
    pub service_result: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
    service_notice: Option<&'static str>,
}

impl State {
    pub fn new(config: &Config, instance: &str, directory: PathBuf) -> Result<Self> {
        let mut state = Self {
            attachment_mode: false,
            attachment: NativeForm::note(),
            received_attachment: None,
            attachment_preview: false,
            attachment_scroll: 0,
            send_notes: false,
            note: NativeForm::note(),
            note_editing: false,
            clipboard_result: None,
            clipboard_files: BTreeMap::new(),
            clipboard_directory: crate::clipboard::directory()?,
            destination: Input::new(directory.display().to_string()),
            directory,
            entries: Vec::new(),
            filtered: Vec::new(),
            cursor: 0,
            scroll: 0,
            queue_cursor: 0,
            queue_scroll: 0,
            selected: BTreeMap::new(),
            filter: Input::default(),
            searching: false,
            hidden: false,
            mode: Mode::Send,
            focus: Focus::Browser,
            link: Input::default(),
            prompt: None,
            secret: Input::default(),
            job: None,
            transfer: None,
            receipt: None,
            receipt_scroll: 0,
            hyperlink: None,
            help: false,
            notice: None,
            results: Vec::new(),
            info: None,
            saved_transfers: protocol::saved_transfers(&config.home.join("transfers"))
                .unwrap_or_default(),
            instance: instance.to_owned(),
            config: config.clone(),
            now: Instant::now(),
            launched: Instant::now(),
            palette: false,
            palette_cursor: 0,
            native: None,
            service_result: None,
            service_notice: None,
        };
        state.refresh()?;
        Ok(state)
    }

    pub fn refresh(&mut self) -> Result<()> {
        let mut entries = Vec::new();
        for value in fs::read_dir(&self.directory)?.flatten() {
            let Ok(metadata) = value.metadata() else {
                continue;
            };
            if !metadata.is_file() && !metadata.is_dir() {
                continue;
            }
            entries.push(Entry {
                path: value.path(),
                name: clean(&value.file_name().to_string_lossy()),
                size: metadata.len(),
                directory: metadata.is_dir(),
            });
        }
        entries.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.entries = entries;
        self.refilter();
        Ok(())
    }

    pub fn refilter(&mut self) {
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (self.hidden || !entry.name.starts_with('.'))
                    && subsequence(&self.filter.value, &entry.name)
            })
            .map(|(index, _)| index)
            .collect();
        self.cursor = self.cursor.min(self.filtered.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.cursor);
    }

    pub fn current(&self) -> Option<&Entry> {
        self.filtered
            .get(self.cursor)
            .map(|index| &self.entries[*index])
    }
    pub fn total(&self) -> u64 {
        self.selected
            .values()
            .fold(0_u64, |sum, entry| sum.saturating_add(entry.size))
    }
    pub fn toast(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    fn open(&mut self, directory: PathBuf) {
        let previous = self.directory.clone();
        self.directory = directory;
        self.filter = Input::default();
        self.cursor = 0;
        self.scroll = 0;
        if let Err(error) = self.refresh() {
            self.directory = previous;
            self.toast(format!("Cannot open folder: {error}"));
        }
    }

    fn toggle(&mut self) {
        if let Some(entry) = self.current().filter(|entry| !entry.directory).cloned()
            && self.selected.remove(&entry.path).is_none()
        {
            self.selected.insert(entry.path.clone(), entry);
        }
    }

    pub fn start(&mut self) {
        self.start_upload(false);
    }

    fn start_upload(&mut self, turbo: bool) {
        if self.clipboard_result.is_some() || self.service_result.is_some() {
            return;
        }
        if self.mode == Mode::Send && self.send_notes {
            if self.attachment_mode {
                std::mem::swap(&mut self.note, &mut self.attachment);
                self.attachment_mode = false;
                self.send_notes = false;
                self.note_editing = false;
                self.toast("Attached note saved; Enter sends files, a edits the note");
                return;
            }
            if self.note.fields[2].value.is_empty() {
                self.toast("Paste or write a note first");
                return;
            }
            let form = NativeForm {
                action: NativeAction::NoteCreate,
                fields: self.note.fields.clone(),
                field: 2,
            };
            if let Err(error) = self.run_native(form) {
                self.toast(error.to_string());
            }
            return;
        }
        self.notice = None;
        let request = match self.mode {
            Mode::Send => {
                if self.selected.is_empty() {
                    self.toggle();
                }
                if self.selected.is_empty() {
                    self.toast("Choose a file with Space to get started");
                    return;
                }
                let authentication = match services::client(&self.config, &self.instance)
                    .and_then(|client| client.upload_authentication())
                {
                    Ok(authentication) => authentication,
                    Err(error) => {
                        self.toast(error.to_string());
                        return;
                    }
                };
                if let Some(Ok(info)) = &self.info {
                    if (!info.anonymous_uploads_enabled
                        && matches!(authentication, protocol::UploadAuthentication::Anonymous))
                        || !info.enabled_drivers.iter().any(|driver| driver == "http")
                    {
                        self.toast("This instance is not accepting anonymous HTTP uploads");
                        return;
                    }
                    if info
                        .maximum_file_count
                        .is_some_and(|maximum| self.selected.len() > maximum)
                    {
                        self.toast(
                            "Too many files for this instance; remove a file from your transfer",
                        );
                        return;
                    }
                }
                Request::Upload(
                    self.selected.keys().cloned().collect(),
                    crate::uploads::DirectoryMode::Individual,
                    protocol::UploadOptions {
                        attached_note: if self.attachment.fields[2].value.is_empty() {
                            None
                        } else {
                            let note = protocol::AttachedNote {
                                text: self.attachment.fields[2].value.clone(),
                                title: Some(self.attachment.fields[0].value.clone())
                                    .filter(|title| !title.is_empty()),
                                language: if self.attachment.fields[1].value.is_empty() {
                                    "plain".into()
                                } else {
                                    self.attachment.fields[1].value.clone()
                                },
                            };
                            if let Err(error) = note.validate() {
                                self.toast(error);
                                return;
                            }
                            Some(note)
                        },
                        snapshot_paths: self
                            .selected
                            .keys()
                            .filter(|path| self.clipboard_files.contains_key(*path))
                            .cloned()
                            .collect(),
                        turbo,
                        authentication,
                        ..Default::default()
                    },
                )
            }
            Mode::Receive => {
                if let Err(error) =
                    protocol::parse_link_for_instance(&self.link.value, &self.instance)
                {
                    self.toast(error.to_string());
                    self.focus = Focus::Link;
                    return;
                }
                if self.destination.value.trim().is_empty() {
                    self.toast("Choose a folder for your downloaded files");
                    self.focus = Focus::Destination;
                    return;
                }
                Request::Download {
                    note_output: None,
                    link: self.link.value.trim().to_owned(),
                    output: expand_path(&self.destination.value),
                }
            }
            Mode::Transfers => {
                self.resume_selected();
                return;
            }
        };
        self.begin(request);
    }

    fn resume_selected(&mut self) {
        let Some(transfer) = self.saved_transfers.get(self.queue_cursor) else {
            self.toast("No saved transfers to resume");
            return;
        };
        let direction = match transfer.direction.as_str() {
            "upload" => crate::app::Direction::Upload,
            "download" => crate::app::Direction::Download,
            _ => {
                self.toast("Saved transfer has an invalid direction");
                return;
            }
        };
        self.begin(Request::Resume {
            note_output: None,
            id: transfer.id.clone(),
            direction,
        });
    }

    fn discard_selected(&mut self) {
        let Some(transfer) = self.saved_transfers.get(self.queue_cursor) else {
            return;
        };
        let id = transfer.id.clone();
        match protocol::discard_transfer(&self.config.home.join("transfers"), &id) {
            Ok(()) => {
                self.saved_transfers.remove(self.queue_cursor);
                self.queue_cursor = self
                    .queue_cursor
                    .min(self.saved_transfers.len().saturating_sub(1));
                self.toast("Saved transfer discarded");
            }
            Err(error) => self.toast(format!("Could not discard saved transfer: {error}")),
        }
    }

    fn begin(&mut self, request: Request) {
        self.received_attachment = None;
        self.attachment_preview = false;
        self.attachment_scroll = 0;
        self.transfer = Some(TransferView::new(request.direction()));
        self.receipt = None;
        self.receipt_scroll = 0;
        self.job = Some(Job::start(
            self.instance.clone(),
            self.config.clone(),
            request,
        ));
    }

    fn cancel(&mut self) {
        if let Some(job) = &self.job {
            job.control.cancel();
        }
        self.prompt = None;
        self.secret = Input::default();
    }

    pub fn receive(&mut self, theme: Theme) {
        if let Some(result) = self
            .clipboard_result
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.clipboard_result = None;
            if self.can_paste() {
                match result {
                    Ok(crate::clipboard::Content::Text(text)) => self.paste_note(&text),
                    Ok(crate::clipboard::Content::Image(file)) => {
                        if self.attachment_mode {
                            std::mem::swap(&mut self.note, &mut self.attachment);
                            self.attachment_mode = false;
                        }
                        self.note_editing = false;
                        let path = file.path().to_owned();
                        let entry = Entry {
                            name: path.file_name().unwrap().to_string_lossy().into_owned(),
                            size: file.as_file().metadata().map(|m| m.len()).unwrap_or(0),
                            path: path.clone(),
                            directory: false,
                        };
                        self.selected.insert(path.clone(), entry);
                        self.clipboard_files.insert(path, file);
                        self.send_notes = false;
                        self.focus = Focus::Queue;
                        self.toast("Clipboard image added");
                    }
                    Err(error) => self.toast(format!("{error:#}")),
                }
            }
        }
        if self.job.is_none() {
            self.clipboard_files
                .retain(|path, _| self.selected.contains_key(path));
        }
        if let Some(result) = self
            .service_result
            .as_ref()
            .and_then(|job| job.try_recv().ok())
        {
            match result {
                Ok(values) => {
                    self.results.extend(values);
                    let notice = self.service_notice.take().unwrap_or("Action completed");
                    self.toast(format!("{notice}. Results will be printed on exit."));
                }
                Err(error) => self.toast(error),
            }
            self.service_result = None;
            self.service_notice = None;
        }
        if let Some(job) = &self.job {
            let result = job.poll();
            if let Some(note) = job.control.attached_note() {
                if self.received_attachment.is_none() {
                    self.attachment_preview = true;
                }
                self.received_attachment = Some(note);
            }
            while let Ok(event) = job.events.try_recv() {
                if let TransferEvent::ShareReady(share) = event
                    && let Some(view) = &mut self.transfer
                {
                    view.share_link = Some(share.share_url);
                }
            }
            if let Some(view) = &mut self.transfer {
                view.tick(job.control.snapshot(), !theme.motion, Instant::now());
                view.cancelling = job.control.cancelled.load(Ordering::Relaxed);
            }
            if let Ok(prompt) = job.prompts.try_recv() {
                self.prompt = Some(prompt);
                self.secret = Input::default();
            }
            if let Some(result) = result {
                if let Some(view) = &mut self.transfer {
                    view.tick(job.control.snapshot(), true, Instant::now());
                    view.finish(result.is_ok());
                }
                let cancelled = result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.is::<Cancelled>());
                if let Ok(values) = &result {
                    self.results.extend(values.iter().cloned());
                }
                self.receipt = Some(Receipt {
                    result: result.map_err(|error| format!("{error:#}")),
                    cancelled,
                });
                self.job = None;
                self.prompt = None;
                self.secret = Input::default();
            }
        }
    }

    pub fn paste(&mut self, value: &str) {
        if self.help || self.palette || self.receipt.is_some() {
            return;
        }
        if let Some(form) = &mut self.native {
            if let Some(field) = form.fields.get_mut(form.field) {
                field.insert(value);
            }
            return;
        }
        if self.prompt.is_some() {
            self.secret.insert(value.trim());
        } else if self.searching {
            self.filter.insert(value);
            self.refilter();
        } else if self.job.is_none() && self.receipt.is_none() && self.mode == Mode::Receive {
            match self.focus {
                Focus::Link => self.link.insert(value.trim()),
                Focus::Destination => self.destination.insert(value),
                _ => {}
            }
        } else if self.can_paste() {
            if self.send_notes && self.note_editing {
                if !self.note.fields[self.note.field].try_insert(value) {
                    self.toast("Note input exceeds 64 KiB; paste was not inserted");
                }
            } else {
                self.paste_note(value);
            }
        }
    }

    fn can_paste(&self) -> bool {
        self.mode == Mode::Send
            && self.job.is_none()
            && self.receipt.is_none()
            && self.prompt.is_none()
            && self.native.is_none()
            && !self.help
            && !self.palette
            && !self.searching
            && self.service_result.is_none()
    }

    fn paste_note(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        self.note.fields[2].end();
        if !self.note.fields[2].try_insert(value) {
            self.toast("Note exceeds 64 KiB; paste was not inserted");
            return;
        }
        self.send_notes = true;
        self.note.field = 2;
        self.note_editing = true;
    }

    fn read_clipboard(&mut self) {
        if !self.can_paste() || self.clipboard_result.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.clipboard_result = Some(receiver);
        let directory = self.clipboard_directory.path().to_owned();
        std::thread::spawn(move || {
            let _ = sender.send(crate::clipboard::read(&directory));
        });
        self.toast("Reading local clipboard…");
    }

    pub fn key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.job.is_some() {
                self.cancel();
                return Ok(false);
            }
            return Ok(true);
        }
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?')) {
                self.help = false;
            }
            return Ok(false);
        }
        if self.palette {
            match key.code {
                KeyCode::Esc => self.palette = false,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.palette_cursor = (self.palette_cursor + 1).min(NativeAction::ALL.len() - 1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.palette_cursor = self.palette_cursor.saturating_sub(1)
                }
                KeyCode::Enter => {
                    let action = NativeAction::ALL[self.palette_cursor];
                    self.palette = false;
                    self.native = Some(NativeForm {
                        action,
                        fields: action.fields().iter().map(|_| Input::default()).collect(),
                        field: 0,
                    });
                }
                _ => {}
            }
            return Ok(false);
        }
        if self.native.is_some() {
            self.native_key(key)?;
            return Ok(false);
        }
        if self.prompt.is_some() {
            if matches!(
                self.prompt.as_ref().map(|prompt| &prompt.kind),
                Some(PromptKind::PeerConsent { .. })
            ) {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if let Some(prompt) = self.prompt.take() {
                            let _ = prompt.reply.send(Zeroizing::new("yes".into()));
                        }
                    }
                    KeyCode::Esc | KeyCode::Enter => self.cancel(),
                    _ => {}
                }
                return Ok(false);
            }
            match key.code {
                KeyCode::Esc => self.cancel(),
                KeyCode::Enter => {
                    if let Some(prompt) = self.prompt.take() {
                        let _ = prompt.reply.send(Zeroizing::new(self.secret.take()));
                    }
                }
                _ => self.secret.handle(key),
            }
            return Ok(false);
        }
        if self.job.is_some() {
            if self.attachment_key(key)? {
                return Ok(false);
            }
            match key.code {
                KeyCode::Char('?') => self.help = true,
                KeyCode::Char('c') => self.copy_result()?,
                _ => {}
            }
            return Ok(false);
        }
        if self.receipt.is_some() {
            if self.attachment_key(key)? {
                return Ok(false);
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Char('c') => self.copy_result()?,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.receipt_scroll = self.receipt_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.receipt_scroll = self.receipt_scroll.saturating_sub(1)
                }
                KeyCode::Enter | KeyCode::Char('n') => {
                    self.receipt = None;
                    self.transfer = None;
                    self.selected.clear();
                    self.switch_mode(self.mode);
                    if let Err(error) = self.refresh() {
                        self.toast(format!("Cannot refresh folder: {error}"));
                    }
                }
                _ => {}
            }
            return Ok(false);
        }
        if self.searching {
            match key.code {
                KeyCode::Enter => self.searching = false,
                KeyCode::Esc => {
                    self.searching = false;
                    self.filter = Input::default();
                    self.refilter();
                }
                _ => {
                    self.filter.handle(key);
                    self.cursor = 0;
                    self.refilter();
                }
            }
            return Ok(false);
        }
        if self.mode == Mode::Send && self.send_notes && self.note_editing {
            if self.service_result.is_some() {
                return Ok(false);
            }
            match key.code {
                KeyCode::Esc => self.note_editing = false,
                KeyCode::BackTab => {
                    let count = if self.attachment_mode { 3 } else { 5 };
                    self.note.field = (self.note.field + count - 1) % count;
                }
                KeyCode::Tab
                    if self.note.field != 2 || key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.note.field =
                        (self.note.field + 1) % if self.attachment_mode { 3 } else { 5 }
                }
                KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => self.start(),
                _ => self.note.fields[self.note.field].handle(key),
            }
            return Ok(false);
        }
        if self.mode == Mode::Receive && matches!(self.focus, Focus::Link | Focus::Destination) {
            match key.code {
                KeyCode::Tab => self.next_focus(false),
                KeyCode::BackTab => self.next_focus(true),
                KeyCode::Esc => self.focus = Focus::Action,
                KeyCode::Enter if self.focus == Focus::Link => self.focus = Focus::Destination,
                KeyCode::Enter => self.start(),
                _ => {
                    if self.focus == Focus::Link {
                        self.link.handle(key)
                    } else {
                        self.destination.handle(key)
                    }
                }
            }
            return Ok(false);
        }
        if self.mode == Mode::Send
            && key.code == KeyCode::Enter
            && key.modifiers.contains(KeyModifiers::SHIFT)
        {
            self.start_upload(true);
            return Ok(false);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            if let Err(error) = self.refresh() {
                self.toast(format!("Cannot refresh folder: {error}"));
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Enter if self.mode == Mode::Send && self.send_notes => self.start(),
            KeyCode::Char('v') if self.mode == Mode::Send => self.read_clipboard(),
            KeyCode::Char('n') if self.mode == Mode::Send => {
                if self.attachment_mode {
                    std::mem::swap(&mut self.note, &mut self.attachment);
                    self.attachment_mode = false;
                }
                self.send_notes = !self.send_notes;
                self.note_editing = false;
            }
            KeyCode::Char('a') if self.mode == Mode::Send && !self.send_notes => {
                std::mem::swap(&mut self.note, &mut self.attachment);
                self.attachment_mode = true;
                self.send_notes = true;
                self.note_editing = true;
                self.note.field = 2;
            }
            KeyCode::Char('e') if self.mode == Mode::Send && self.send_notes => {
                self.note_editing = true
            }
            KeyCode::Tab if self.mode == Mode::Send && self.send_notes => {
                self.note.field = (self.note.field + 1) % if self.attachment_mode { 3 } else { 5 };
                self.note_editing = true;
            }
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('p') => self.palette = true,
            KeyCode::Char('1') | KeyCode::Char('s') => {
                self.switch_mode(Mode::Send);
            }
            KeyCode::Char('2') | KeyCode::Char('d') => {
                self.switch_mode(Mode::Receive);
            }
            KeyCode::Char('3') | KeyCode::Char('r') => {
                self.switch_mode(Mode::Transfers);
            }
            KeyCode::Tab => self.next_focus(false),
            KeyCode::BackTab => self.next_focus(true),
            KeyCode::Char('U') => self.begin(Request::Update),
            KeyCode::Char('u') if self.mode == Mode::Send => self.start(),
            KeyCode::Enter if self.mode == Mode::Transfers => self.resume_selected(),
            KeyCode::Enter if self.focus == Focus::Action => self.start(),
            _ if self.mode == Mode::Transfers => self.transfers_key(key),
            _ if self.mode == Mode::Send && !self.send_notes => self.browser_key(key),
            _ => {}
        }
        Ok(false)
    }

    fn switch_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.focus = match mode {
            Mode::Send => {
                self.queue_cursor = self.queue_cursor.min(self.selected.len().saturating_sub(1));
                Focus::Browser
            }
            Mode::Receive => Focus::Link,
            Mode::Transfers => {
                self.saved_transfers =
                    protocol::saved_transfers(&self.config.home.join("transfers"))
                        .unwrap_or_default();
                self.queue_cursor = self
                    .queue_cursor
                    .min(self.saved_transfers.len().saturating_sub(1));
                Focus::Jobs
            }
        };
        self.queue_scroll = 0;
    }

    fn transfers_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.queue_cursor =
                    (self.queue_cursor + 1).min(self.saved_transfers.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.queue_cursor = self.queue_cursor.saturating_sub(1)
            }
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('x') => self.discard_selected(),
            _ => {}
        }
    }

    fn native_key(&mut self, key: KeyEvent) -> Result<()> {
        let form = self.native.as_mut().expect("native form checked");
        match key.code {
            KeyCode::Esc => {
                self.native = None;
                return Ok(());
            }
            KeyCode::Tab | KeyCode::Down => {
                form.field = (form.field + 1) % form.fields.len().max(1)
            }
            KeyCode::BackTab | KeyCode::Up => {
                form.field = form
                    .field
                    .checked_sub(1)
                    .unwrap_or(form.fields.len().saturating_sub(1))
            }
            KeyCode::Enter if form.field + 1 < form.fields.len() => form.field += 1,
            KeyCode::Enter => {
                let form = self.native.take().expect("form exists");
                self.run_native(form)?;
            }
            _ => {
                if let Some(input) = form.fields.get_mut(form.field) {
                    input.handle(key)
                }
            }
        }
        Ok(())
    }

    fn run_native(&mut self, form: NativeForm) -> Result<()> {
        let values: Vec<String> = form
            .fields
            .into_iter()
            .map(|mut value| value.take())
            .collect();
        match form.action {
            NativeAction::Revoke => {
                self.begin(Request::Revoke {
                    id: values[0].clone(),
                });
                return Ok(());
            }
            NativeAction::EndLive => {
                self.begin(Request::EndLive {
                    id: values[0].clone(),
                });
                return Ok(());
            }
            NativeAction::InboxDownload => {
                let client = services::client(&self.config, &self.instance)?;
                let private = services::stored_private_key(&self.config, &self.instance)?;
                let metadata = client.account().inbox_metadata(&values[0])?;
                let key = filebeam_client_core::services::open_recipient_key(
                    &private,
                    &metadata.recipient_key,
                    &values[0],
                )?;
                self.begin(Request::InboxDownload {
                    note_output: None,
                    id: values[0].clone(),
                    output: expand_path(&values[1]),
                    key: key.to_vec(),
                    cookie: client.cookie_context()?,
                });
                return Ok(());
            }
            NativeAction::NoteCreate
                if values
                    .get(4)
                    .is_some_and(|flags| flags.split_whitespace().any(|v| v == "live")) =>
            {
                self.begin(Request::NoteLive(
                    filebeam_client_core::services::NoteCreate {
                        text: values[2].clone(),
                        title: (!values[0].is_empty()).then(|| values[0].clone()),
                        language: if values[1].is_empty() {
                            "plain".into()
                        } else {
                            values[1].clone()
                        },
                        password: (!values[3].is_empty()).then(|| values[3].clone()),
                        burn_on_read: values[4].split_whitespace().any(|v| v == "burn"),
                        retention_hours: None,
                    },
                ));
                return Ok(());
            }
            _ => {}
        }
        let config = self.config.clone();
        let instance = self.instance.clone();
        let (send, receive) = mpsc::channel();
        self.service_result = Some(receive);
        self.service_notice = Some(form.action.completed());
        std::thread::spawn(move || {
            let result = native_action(form.action, &config, &instance, values)
                .map_err(|error| format!("{error:#}"));
            let _ = send.send(result);
        });
        Ok(())
    }

    fn next_focus(&mut self, reverse: bool) {
        let focuses = if self.mode == Mode::Send {
            [Focus::Browser, Focus::Queue, Focus::Action]
        } else if self.mode == Mode::Receive {
            [Focus::Link, Focus::Destination, Focus::Action]
        } else {
            [Focus::Jobs, Focus::Jobs, Focus::Jobs]
        };
        let position = focuses
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);
        self.focus = focuses[(position + if reverse { 2 } else { 1 }) % 3];
    }

    fn browser_key(&mut self, key: KeyEvent) {
        if self.focus == Focus::Queue {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.queue_cursor =
                        (self.queue_cursor + 1).min(self.selected.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.queue_cursor = self.queue_cursor.saturating_sub(1)
                }
                KeyCode::Delete | KeyCode::Backspace | KeyCode::Char(' ') => {
                    if let Some(path) = self.selected.keys().nth(self.queue_cursor).cloned() {
                        self.selected.remove(&path);
                    }
                    self.queue_cursor =
                        self.queue_cursor.min(self.selected.len().saturating_sub(1));
                }
                KeyCode::Enter => self.start(),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = (self.cursor + 1).min(self.filtered.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::PageDown => {
                self.cursor = (self.cursor + 10).min(self.filtered.len().saturating_sub(1))
            }
            KeyCode::PageUp => self.cursor = self.cursor.saturating_sub(10),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.filtered.len().saturating_sub(1),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::Browser;
            }
            KeyCode::Char('.') => {
                self.hidden = !self.hidden;
                self.refilter();
            }
            KeyCode::Backspace => {
                if let Some(parent) = self.directory.parent() {
                    self.open(parent.to_owned());
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = self.current().cloned() {
                    if entry.directory {
                        self.open(entry.path);
                    } else {
                        self.toggle();
                    }
                }
            }
            _ => {}
        }
    }

    fn copy_result(&mut self) -> Result<()> {
        let value = match &self.receipt {
            Some(Receipt {
                result: Ok(values), ..
            }) => Some(values.join("\n")),
            None => self
                .transfer
                .as_ref()
                .and_then(|view| view.share_link.clone()),
            _ => None,
        };
        if let Some(value) = value {
            let value = STANDARD.encode(value);
            let mut output = io::stdout();
            write!(output, "\x1b]52;c;{value}\x07")?;
            output.flush()?;
            self.toast("Copy requested. Requires terminal clipboard support.");
        }
        Ok(())
    }

    fn attachment_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.received_attachment.is_none() {
            return Ok(false);
        }
        if !self.attachment_preview {
            if key.code == KeyCode::Char('o') {
                self.attachment_preview = true;
                return Ok(true);
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('o') => self.attachment_preview = false,
            KeyCode::Down | KeyCode::Char('j') => {
                self.attachment_scroll = self.attachment_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.attachment_scroll = self.attachment_scroll.saturating_sub(1)
            }
            KeyCode::Char('c') => {
                let value = STANDARD.encode(&self.received_attachment.as_ref().unwrap().text);
                write!(io::stdout(), "\x1b]52;c;{value}\x07")?;
                io::stdout().flush()?;
            }
            _ => {}
        }
        Ok(true)
    }
}

fn expand_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        home.join(rest)
    } else if value == "~" {
        dirs::home_dir().unwrap_or_else(|| Path::new(".").to_owned())
    } else {
        PathBuf::from(value)
    }
}

fn native_action(
    action: NativeAction,
    config: &Config,
    instance: &str,
    v: Vec<String>,
) -> Result<Vec<String>> {
    let client = services::client(config, instance)?;
    let account = client.account();
    match action {
        NativeAction::HistoryList => {
            let page = client.history().list(
                &Default::default(),
                (!v[0].is_empty()).then_some(v[0].as_str()),
                25,
            )?;
            let mut rows = page
                .data
                .iter()
                .map(services::history_row)
                .collect::<Vec<_>>();
            if let Some(cursor) = page.next_cursor {
                rows.push(format!("Older entries cursor: {cursor}"));
            }
            Ok(rows)
        }
        NativeAction::HistoryDelete => {
            anyhow::ensure!(v[1] == "DELETE", "deletion requires typing DELETE");
            client.history().delete(&v[0])?;
            Ok(vec!["Deletion scheduled".into()])
        }
        NativeAction::HistoryExtend => {
            let update = client.history().extend(&v[0], v[1].parse()?)?;
            Ok(vec![format!("Expires {}", update.expires_at)])
        }
        NativeAction::NoteCreate => Ok(vec![
            client
                .notes()
                .create(filebeam_client_core::services::NoteCreate {
                    text: v[2].clone(),
                    title: (!v[0].is_empty()).then(|| v[0].clone()),
                    language: if v[1].is_empty() {
                        "plain".into()
                    } else {
                        v[1].clone()
                    },
                    password: (!v[3].is_empty()).then(|| v[3].clone()),
                    burn_on_read: v[4].split_whitespace().any(|flag| flag == "burn"),
                    retention_hours: None,
                })?
                .link,
        ]),
        // The TUI secret has already been collected in a masked field, so avoid
        // routing it through stdin or a process argument.
        NativeAction::NoteOpen => {
            let password = (!v[1].is_empty()).then(|| v[1].as_str());
            let note = services::client(config, instance)?
                .notes()
                .open(&v[0], password)?;
            Ok(vec![
                note.title.unwrap_or_default(),
                format!("language: {}", note.language),
                note.text,
            ])
        }
        NativeAction::InboxSync
        | NativeAction::InboxStaged
        | NativeAction::InboxSave
        | NativeAction::InboxDismiss => {
            let receiver = services::receiver(config, instance, account.session()?.id)?;
            let control = services::receiver_control(config);
            match action {
                NativeAction::InboxSync => {
                    receiver.set_enabled(true)?;
                    receiver.sweep(&client, &control)?;
                }
                NativeAction::InboxSave => {
                    return receiver.export(
                        &v[0],
                        &services::stored_private_key(config, instance)?,
                        &expand_path(&v[1]),
                        &control,
                    );
                }
                NativeAction::InboxDismiss => {
                    receiver.dismiss(&v[0])?;
                }
                _ => {}
            }
            Ok(receiver
                .state()?
                .entries
                .into_iter()
                .map(|entry| {
                    format!(
                        "{} {}: {} encrypted bytes",
                        entry.id, entry.state, entry.bytes
                    )
                })
                .collect())
        }
        NativeAction::Contacts => Ok(account
            .contacts()?
            .contacts
            .into_iter()
            .map(|contact| {
                format!(
                    "@{} {}: can send {}, auto-download {}",
                    contact.username,
                    contact.status,
                    contact.effective.can_send,
                    contact.effective.auto_download
                )
            })
            .collect()),
        NativeAction::ContactAction => {
            let parse = |value: &str| -> Result<Option<bool>> {
                match value {
                    "" | "inherit" => Ok(None),
                    "allow" => Ok(Some(true)),
                    "deny" => Ok(Some(false)),
                    _ => anyhow::bail!("Use inherit, allow, or deny"),
                }
            };
            account.contact_action(&v[0], &v[1], parse(&v[2])?, parse(&v[3])?)?;
            Ok(vec!["Contact updated".into()])
        }
        NativeAction::ReceivingDefaults => {
            account.set_receiving_defaults(&v[0], v[1] == "on")?;
            Ok(vec!["Receiving defaults saved".into()])
        }
        NativeAction::InboxList => Ok(account
            .inbox()?
            .into_iter()
            .map(|item| format!("{}\t{}\t{}", item.id, item.item_count, item.expires_at))
            .collect()),
        NativeAction::Recipient => {
            let recipient = account.recipient(&v[0])?;
            Ok(vec![format!(
                "{}\t{}\t{}",
                recipient.username, recipient.id, recipient.fingerprint
            )])
        }
        NativeAction::Login => {
            let session = account.login(&v[0], &v[1], true)?;
            services::save_session(config, instance, &client)?;
            Ok(vec![session.email])
        }
        NativeAction::Register => {
            let session = account.register(
                &v[0],
                (!v[2].is_empty()).then(|| v[2].as_str()),
                &v[1],
                &v[3],
            )?;
            services::save_session(config, instance, &client)?;
            Ok(vec![session.email])
        }
        NativeAction::Logout => {
            account.logout()?;
            services::clear_session(config, instance)?;
            Ok(vec!["logged out".into()])
        }
        NativeAction::Profile => {
            let session = account.session()?;
            Ok(vec![format!(
                "{}\t{}\t{}",
                session.email,
                session.username.unwrap_or_default(),
                session.inbox_enabled
            )])
        }
        NativeAction::ResendVerification => {
            account.resend_verification()?;
            Ok(vec!["verification email requested".into()])
        }
        NativeAction::Verify => {
            account.verify_email_link(&v[0])?;
            Ok(vec!["email verified".into()])
        }
        NativeAction::RecoveryRequest => {
            account.request_password_reset(&v[0])?;
            Ok(vec!["recovery email requested".into()])
        }
        NativeAction::RecoveryReset => {
            account.reset_password(&v[0], &v[1], &v[2])?;
            Ok(vec!["password reset".into()])
        }
        NativeAction::KeySelf => {
            services::setup_self_key(config, instance, v[0] == "REPLACE")?;
            Ok(vec!["self-custody key created".into()])
        }
        NativeAction::KeyPassword => {
            services::setup_password_key(config, instance, &v[0], v[1] == "REPLACE")?;
            Ok(vec!["password custody key created".into()])
        }
        NativeAction::KeyImport => {
            let value = services::private_text_file(Path::new(&v[0]))?;
            services::import_self_key_for_account(
                config,
                instance,
                value.trim(),
                v[1] == "REPLACE",
            )?;
            Ok(vec!["self-custody key imported".into()])
        }
        NativeAction::KeyExport => {
            anyhow::ensure!(v[0] == "EXPORT", "key export requires typing EXPORT");
            Ok(vec![services::export_stored_key(config, instance)?])
        }
        NativeAction::InboxDownload | NativeAction::Revoke | NativeAction::EndLive => {
            unreachable!("handled by shared transfer job")
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn attachment_editor_preserves_standalone_draft_and_file_selection() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            State::new(&Config::default(), "http://127.0.0.1:1", dir.path().into()).unwrap();
        state.note.fields[2].value = "Standalone note".into();
        state.key(KeyCode::Char('a').into()).unwrap();
        assert!(state.attachment_mode);
        state.paste("Attached note 🦀\n");
        state.key(KeyCode::Esc.into()).unwrap();
        state.start();
        assert!(!state.send_notes && !state.attachment_mode);
        assert_eq!(state.note.fields[2].value, "Standalone note");
        assert_eq!(state.attachment.fields[2].value, "Attached note 🦀\n");
        assert!(state.job.is_none());
        state.key(KeyCode::Char('a').into()).unwrap();
        state.key(KeyCode::BackTab.into()).unwrap();
        assert_eq!(state.note.field, 1);
    }
    #[test]
    fn send_paste_keeps_both_drafts_and_respects_editing_and_overlays() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            State::new(&Config::default(), "http://127.0.0.1:1", dir.path().into()).unwrap();
        let path = dir.path().join("original.txt");
        state.selected.insert(
            path.clone(),
            Entry {
                path,
                name: "original.txt".into(),
                size: 3,
                directory: false,
            },
        );
        state.paste("one\n\ttwo 日本");
        assert!(state.send_notes && state.note_editing);
        assert_eq!(state.note.fields[2].value, "one\n\ttwo 日本");
        assert_eq!(state.selected.len(), 1);
        state.key(KeyCode::Esc.into()).unwrap();
        state.key(KeyCode::Char('n').into()).unwrap();
        state.paste("\nthree");
        assert_eq!(state.note.fields[2].value, "one\n\ttwo 日本\nthree");
        state.note.field = 0;
        state.paste("title");
        assert_eq!(state.note.fields[0].value, "title");
        state.help = true;
        state.paste("ignored");
        assert_eq!(state.note.fields[0].value, "title");
        state.help = false;
        state.note_editing = false;
        state.switch_mode(Mode::Receive);
        state.paste(" https://example.com/link ");
        assert_eq!(state.link.value, "https://example.com/link");
        assert_eq!(state.note.fields[2].value, "one\n\ttwo 日本\nthree");
    }

    #[test]
    fn queued_clipboard_source_is_removed_only_after_queue_removal() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            State::new(&Config::default(), "http://127.0.0.1:1", dir.path().into()).unwrap();
        let file = tempfile::NamedTempFile::new_in(state.clipboard_directory.path()).unwrap();
        let path = file.path().to_owned();
        let (sender, receiver) = mpsc::channel();
        state.clipboard_result = Some(receiver);
        sender
            .send(Ok(crate::clipboard::Content::Image(file)))
            .unwrap();
        state.receive(Theme::new(&Config::default()));
        assert!(state.selected.contains_key(&path));
        assert!(path.exists());
        state.selected.clear();
        state.receive(Theme::new(&Config::default()));
        assert!(!path.exists());
    }
    use super::*;

    #[test]
    fn transfers_page_allows_navigation_help_and_exit() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.home = dir.path().join("home");
        let mut state = State::new(&config, "http://localhost:8000", dir.path().into()).unwrap();
        for populated in [false, true] {
            state.key(KeyCode::Char('3').into()).unwrap();
            if populated {
                state.saved_transfers.push(SavedTransfer {
                    id: "job".into(),
                    direction: "upload".into(),
                    state: "sending".into(),
                    done: 1,
                    total: 10,
                });
            }
            state.key(KeyCode::Char('?').into()).unwrap();
            assert!(state.help);
            state.key(KeyCode::Esc.into()).unwrap();
            state.key(KeyCode::Char('1').into()).unwrap();
            assert!(state.mode == Mode::Send && state.focus == Focus::Browser);
            state.key(KeyCode::Char('3').into()).unwrap();
            state.key(KeyCode::Char('2').into()).unwrap();
            assert!(state.mode == Mode::Receive && state.focus == Focus::Link);
            for digit in "123".chars() {
                state.key(KeyCode::Char(digit).into()).unwrap();
            }
            assert_eq!(state.link.take(), "123");
            state.key(KeyCode::Esc.into()).unwrap();
            state.key(KeyCode::Char('3').into()).unwrap();
            assert!(state.mode == Mode::Transfers && state.focus == Focus::Jobs);
            assert!(state.key(KeyCode::Char('q').into()).unwrap());
            assert!(state.key(KeyCode::Esc.into()).unwrap());
        }
    }

    #[test]
    fn new_transfer_after_resume_restores_the_jobs_focus() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.home = dir.path().join("home");
        let mut state = State::new(&config, "http://localhost:8000", dir.path().into()).unwrap();
        state.mode = Mode::Transfers;
        state.transfer = Some(TransferView::new(crate::app::Direction::Upload));
        state.receipt = Some(Receipt {
            result: Ok(vec![]),
            cancelled: false,
        });
        state.key(KeyCode::Enter.into()).unwrap();
        assert!(state.focus == Focus::Jobs);
        assert!(state.transfer.is_none());
    }
    #[test]
    fn search_input_does_not_trigger_transfer_shortcuts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("upload-key.txt"), b"test").unwrap();
        let mut state = State::new(
            &Config::default(),
            "http://localhost:8000",
            dir.path().into(),
        )
        .unwrap();
        state.key(KeyCode::Char('/').into()).unwrap();
        for key in "upload-key".chars() {
            state.key(KeyCode::Char(key).into()).unwrap();
        }
        assert_eq!(state.filtered.len(), 1);
        assert!(state.job.is_none());
        state.key(KeyCode::Enter.into()).unwrap();
        state.key(KeyCode::Char(' ').into()).unwrap();
        assert_eq!(state.selected.len(), 1);
    }

    #[test]
    fn transfer_tab_cursor_is_safe_when_the_store_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            &Config::default(),
            "http://localhost:8000",
            dir.path().into(),
        )
        .unwrap();
        state.mode = Mode::Transfers;
        state.key(KeyCode::Down.into()).unwrap();
        assert_eq!(state.queue_cursor, 0);
        assert!(state.job.is_none());
    }

    #[test]
    fn send_actions_select_their_upload_mode_once() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("report.pdf"), b"test").unwrap();
        let mut state = State::new(
            &Config::default(),
            "http://localhost:8000",
            dir.path().into(),
        )
        .unwrap();
        state.key(KeyCode::Char(' ').into()).unwrap();
        state.focus = Focus::Action;
        state
            .key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT))
            .unwrap();
        assert!(state.job.is_some());
        state.cancel();
        assert!(state.job.is_some());
    }

    #[test]
    fn native_palette_opens_a_masked_account_form_without_starting_a_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            &Config::default(),
            "http://localhost:8000",
            dir.path().into(),
        )
        .unwrap();
        state.key(KeyCode::Char('p').into()).unwrap();
        assert!(state.palette);
        while NativeAction::ALL[state.palette_cursor] != NativeAction::Login {
            state.key(KeyCode::Down.into()).unwrap();
        }
        state.key(KeyCode::Enter.into()).unwrap();
        assert!(state.native.is_some());
        assert!(state.job.is_none());
        assert_eq!(
            state.native.as_ref().unwrap().action.fields()[1],
            ("password", true)
        );
    }
}
