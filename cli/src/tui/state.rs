use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Instant,
};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroizing;

use crate::{
    app::{Cancelled, Job, Prompt, Request, TransferView, subsequence},
    config::Config,
    input::Input,
    presentation::{Theme, clean},
    protocol::{self, Info, SavedTransfer},
};

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
        if self.prompt.is_some() {
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
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            if let Err(error) = self.refresh() {
                self.toast(format!("Cannot refresh folder: {error}"));
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => self.help = true,
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
}
