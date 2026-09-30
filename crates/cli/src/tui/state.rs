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
    app::{Cancelled, Job, Prompt, PromptKind, Request, TransferView, subsequence},
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
}

impl NativeAction {
    pub const ALL: [Self; 19] = [
        Self::NoteCreate,
        Self::NoteOpen,
        Self::InboxList,
        Self::InboxDownload,
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
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::NoteCreate => "Note: create (native editor)",
            Self::NoteOpen => "Note: open",
            Self::InboxList => "Inbox: list",
            Self::InboxDownload => "Inbox: download",
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
            Self::NoteCreate => &[
                ("title (optional)", false),
                ("language", false),
                ("note text", false),
                ("password (optional)", true),
                ("flags: burn live", false),
            ],
            Self::NoteOpen => &[("link", false), ("password (optional)", true)],
            Self::InboxDownload => &[("delivery id", false), ("output folder", false)],
            Self::Recipient | Self::Revoke | Self::EndLive => &[("username or transfer id", false)],
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
}

pub struct NativeForm {
    pub action: NativeAction,
    pub fields: Vec<Input>,
    pub field: usize,
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
}

impl State {
    pub fn new(config: &Config, instance: &str, directory: PathBuf) -> Result<Self> {
        let mut state = Self {
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
                if let Some(Ok(info)) = &self.info {
                    if !info.anonymous_uploads_enabled
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
                        turbo,
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
            .service_result
            .as_ref()
            .and_then(|job| job.try_recv().ok())
        {
            match result {
                Ok(values) => {
                    self.results.extend(values);
                    self.toast("Native service action completed");
                }
                Err(error) => self.toast(error),
            }
            self.service_result = None;
        }
        if let Some(job) = &self.job {
            if let Some(view) = &mut self.transfer {
                view.tick(job.control.snapshot(), !theme.motion, Instant::now());
                view.cancelling = job.control.cancelled.load(Ordering::Relaxed);
            }
            if let Ok(prompt) = job.prompts.try_recv() {
                self.prompt = Some(prompt);
                self.secret = Input::default();
            }
            if let Some(result) = job.poll() {
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
        if let Some(form) = &mut self.native {
            form.fields[form.field].insert(value);
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
        }
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
            if key.code == KeyCode::Char('?') {
                self.help = true;
            }
            return Ok(false);
        }
        if self.receipt.is_some() {
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
                    self.focus = if self.mode == Mode::Send {
                        Focus::Browser
                    } else {
                        Focus::Link
                    };
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
        if self.mode == Mode::Transfers {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.queue_cursor =
                        (self.queue_cursor + 1).min(self.saved_transfers.len().saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.queue_cursor = self.queue_cursor.saturating_sub(1);
                }
                KeyCode::Enter => self.resume_selected(),
                KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('x') => {
                    self.discard_selected()
                }
                _ => {}
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
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('p') => self.palette = true,
            KeyCode::Char('1') | KeyCode::Char('s') => {
                self.mode = Mode::Send;
                self.focus = Focus::Browser;
            }
            KeyCode::Char('2') | KeyCode::Char('d') => {
                self.mode = Mode::Receive;
                self.focus = Focus::Link;
            }
            KeyCode::Char('3') | KeyCode::Char('r') => {
                self.mode = Mode::Transfers;
                self.focus = Focus::Jobs;
                self.saved_transfers =
                    protocol::saved_transfers(&self.config.home.join("transfers"))
                        .unwrap_or_default();
                self.queue_cursor = self
                    .queue_cursor
                    .min(self.saved_transfers.len().saturating_sub(1));
            }
            KeyCode::Tab => self.next_focus(false),
            KeyCode::BackTab => self.next_focus(true),
            KeyCode::Char('U') => self.begin(Request::Update),
            KeyCode::Char('u') if self.mode == Mode::Send => self.start(),
            KeyCode::Enter if self.mode == Mode::Transfers => self.resume_selected(),
            KeyCode::Enter if self.focus == Focus::Action => self.start(),
            _ if self.mode == Mode::Send => self.browser_key(key),
            _ => {}
        }
        Ok(false)
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
        if let Some(Receipt {
            result: Ok(values), ..
        }) = &self.receipt
        {
            let value = STANDARD.encode(values.join("\n"));
            let mut output = io::stdout();
            write!(output, "\x1b]52;c;{value}\x07")?;
            output.flush()?;
            self.toast("Copy requested · your terminal must support clipboard access. Links are also printed on exit.");
        }
        Ok(())
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
    use super::*;
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
