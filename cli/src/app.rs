use std::{
    collections::VecDeque,
    io::{self, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use zeroize::Zeroizing;

use crate::{config::Config, protocol, update, uploads::DirectoryMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Upload,
    Download,
    Update,
}

impl Direction {
    pub fn completed(self) -> &'static str {
        match self {
            Self::Upload => "Uploaded",
            Self::Download => "Downloaded and verified",
            Self::Update => "Updated",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Connecting,
    Preparing,
    Archiving,
    Encrypting,
    Sending,
    Receiving,
    Verifying,
    Finalizing,
    Waiting,
    Unlocking,
    Updating,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Preparing => "Preparing",
            Self::Archiving => "Creating ZIP archive",
            Self::Encrypting => "Encrypting",
            Self::Sending => "Uploading",
            Self::Receiving => "Downloading",
            Self::Verifying => "Verifying integrity",
            Self::Finalizing => "Creating your encrypted link",
            Self::Waiting => "Waiting for the sender",
            Self::Unlocking => "Unlocking your transfer",
            Self::Updating => "Verifying signed release",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub phase: Phase,
    pub name: String,
    pub index: usize,
    pub files: usize,
    pub total: Option<u64>,
    pub done: u64,
    pub committed: u64,
    pub item_total: u64,
    pub item_done: u64,
    pub wire_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum SecretKind {
    ShareKey,
    Password,
}

impl SecretKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ShareKey => "Decryption key",
            Self::Password => "Transfer password",
        }
    }
}

#[derive(Clone, Copy)]
pub enum PromptKind {
    Secret(SecretKind),
    Directory {
        files: usize,
        bytes: u64,
        maximum_files: Option<usize>,
    },
}

impl PromptKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Secret(kind) => kind.label(),
            Self::Directory { .. } => "Directory upload",
        }
    }
}

pub struct Prompt {
    pub kind: PromptKind,
    pub reply: Sender<Zeroizing<String>>,
}

#[derive(Debug, thiserror::Error)]
#[error("Transfer cancelled")]
pub struct Cancelled;

/// The worker publishes a single latest snapshot. Large transfers never build an event backlog.
#[derive(Clone)]
pub struct Control {
    progress: Arc<Mutex<Progress>>,
    pub cancelled: Arc<AtomicBool>,
    prompts: Sender<Prompt>,
}

impl Control {
    pub fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        Ok(())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Progress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn phase(&self, phase: Phase) -> Result<()> {
        self.check()?;
        self.edit(|progress| progress.phase = phase);
        Ok(())
    }

    pub fn totals(&self, bytes: u64, files: usize) {
        self.edit(|progress| {
            progress.total = Some(bytes);
            progress.files = files;
        });
    }

    pub fn item(&self, name: String, index: usize, size: u64) {
        self.edit(|progress| {
            progress.name = name;
            progress.index = index;
            progress.item_total = size;
            progress.item_done = 0;
        });
    }

    pub fn advance(&self, done: u64, item_done: u64, wire: u64) {
        self.edit(|progress| {
            progress.done = done.min(progress.total.unwrap_or(done));
            progress.item_done = item_done.min(progress.item_total);
            progress.wire_bytes = progress.wire_bytes.saturating_add(wire);
        });
    }

    pub fn commit(&self, bytes: u64) {
        self.edit(|progress| progress.committed = bytes);
    }

    pub fn secret(&self, kind: SecretKind) -> Result<Zeroizing<String>> {
        self.phase(Phase::Unlocking)?;
        self.ask(PromptKind::Secret(kind))
    }

    pub fn ask(&self, kind: PromptKind) -> Result<Zeroizing<String>> {
        let (reply, receiver) = mpsc::channel();
        self.prompts.send(Prompt { kind, reply })?;
        loop {
            self.check()?;
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(value) => return Ok(value),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => bail!("Secret input was closed"),
            }
        }
    }

    fn edit(&self, update: impl FnOnce(&mut Progress)) {
        update(&mut self.progress.lock().unwrap_or_else(|e| e.into_inner()));
    }
}

/// Count bytes handed to/from HTTP, separately from server acknowledgement and verification.
pub struct TransferReader<R> {
    inner: R,
    control: Control,
    payload_size: u64,
    read: u64,
    total_base: u64,
    item_base: u64,
}

impl<R: Read> TransferReader<R> {
    pub fn new(
        inner: R,
        control: &Control,
        payload_size: u64,
        total_base: u64,
        item_base: u64,
    ) -> Self {
        Self {
            inner,
            control: control.clone(),
            payload_size,
            read: 0,
            total_base,
            item_base,
        }
    }
}

impl<R: Read> Read for TransferReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.control.check().map_err(io::Error::other)?;
        let length = buffer.len().min(64 * 1024);
        let count = self.inner.read(&mut buffer[..length])?;
        self.read += count as u64;
        let payload = self.read.min(self.payload_size);
        self.control.advance(
            self.total_base + payload,
            self.item_base + payload,
            count as u64,
        );
        Ok(count)
    }
}

pub enum Request {
    Upload(Vec<PathBuf>, DirectoryMode),
    Download { link: String, output: PathBuf },
    Update,
}

impl Request {
    pub fn direction(&self) -> Direction {
        match self {
            Self::Upload(..) => Direction::Upload,
            Self::Download { .. } => Direction::Download,
            Self::Update => Direction::Update,
        }
    }
}

pub struct Job {
    pub control: Control,
    pub prompts: Receiver<Prompt>,
    result: Receiver<Result<Vec<String>>>,
}

impl Job {
    pub fn start(instance: String, config: Config, request: Request) -> Self {
        let (prompts, receiver) = mpsc::channel();
        let (sender, result) = mpsc::channel();
        let control = Control {
            progress: Arc::new(Mutex::new(Progress::default())),
            cancelled: Arc::new(AtomicBool::new(false)),
            prompts,
        };
        let worker = control.clone();
        thread::spawn(move || {
            let outcome = match request {
                Request::Upload(paths, mode) => {
                    protocol::upload(&instance, &paths, mode, &worker).map(|link| vec![link])
                }
                Request::Download { link, output } => {
                    protocol::download(&instance, &link, &output, &worker).map(|paths| {
                        paths
                            .into_iter()
                            .map(|path| path.display().to_string())
                            .collect()
                    })
                }
                Request::Update => worker
                    .phase(Phase::Updating)
                    .and_then(|_| update::check(&config))
                    .map(|value| vec![value]),
            };
            let outcome = if worker.cancelled.load(Ordering::Relaxed) && outcome.is_err() {
                Err(Cancelled.into())
            } else {
                outcome
            };
            let _ = sender.send(outcome);
        });
        Self {
            control,
            prompts: receiver,
            result,
        }
    }

    pub fn poll(&self) -> Option<Result<Vec<String>>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err(anyhow::anyhow!("Transfer worker stopped unexpectedly")))
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

pub struct TransferView {
    pub direction: Direction,
    pub progress: Progress,
    pub started: Instant,
    pub elapsed: Duration,
    pub ratio: f64,
    pub rate: f64,
    pub history: VecDeque<u64>,
    pub finished: bool,
    pub cancelling: bool,
    last_tick: Instant,
    last_sample: Instant,
    last_movement: Instant,
    sampled_bytes: u64,
}

impl TransferView {
    pub fn new(direction: Direction) -> Self {
        let now = Instant::now();
        Self {
            direction,
            progress: Progress::default(),
            started: now,
            elapsed: Duration::ZERO,
            ratio: 0.0,
            rate: 0.0,
            history: VecDeque::from(vec![0; 48]),
            finished: false,
            cancelling: false,
            last_tick: now,
            last_sample: now,
            last_movement: now,
            sampled_bytes: 0,
        }
    }

    pub fn tick(&mut self, progress: Progress, reduced_motion: bool, now: Instant) {
        if self.finished {
            return;
        }
        if progress.wire_bytes > self.progress.wire_bytes {
            self.last_movement = now;
        }
        self.progress = progress;
        self.elapsed = now.duration_since(self.started);
        let dt = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        let target = self
            .progress
            .total
            .filter(|total| *total > 0)
            .map(|total| self.progress.done as f64 / total as f64)
            .unwrap_or(0.0)
            .min(0.995);
        self.ratio = if reduced_motion {
            target
        } else {
            self.ratio + (target - self.ratio) * (1.0 - (-dt / 0.14).exp())
        };
        let period = now.duration_since(self.last_sample).as_secs_f64();
        if period >= 0.25 {
            let rate = self.progress.wire_bytes.saturating_sub(self.sampled_bytes) as f64 / period;
            self.rate = if self.rate == 0.0 {
                rate
            } else {
                self.rate * 0.6 + rate * 0.4
            };
            self.sampled_bytes = self.progress.wire_bytes;
            self.last_sample = now;
            self.history.pop_front();
            self.history.push_back(rate as u64);
        }
    }

    pub fn stalled(&self) -> bool {
        matches!(self.progress.phase, Phase::Sending | Phase::Receiving)
            && self.last_tick.duration_since(self.last_movement) >= Duration::from_secs(3)
    }

    pub fn eta(&self) -> Option<Duration> {
        if self.rate < 1.0
            || self.stalled()
            || self.elapsed < Duration::from_secs(1)
            || !matches!(self.progress.phase, Phase::Sending | Phase::Receiving)
        {
            return None;
        }
        let remaining = self.progress.total?.saturating_sub(self.progress.done);
        if remaining == 0 {
            return None;
        }
        Some(Duration::from_secs_f64(
            (remaining as f64 / self.rate).min(86400.0 * 365.0),
        ))
    }

    pub fn phase_label(&self) -> &'static str {
        if self.cancelling {
            "Cancelling · waiting for the active request"
        } else if self.stalled() {
            "Waiting for the connection"
        } else {
            self.progress.phase.label()
        }
    }

    pub fn finish(&mut self, success: bool) {
        self.finished = true;
        self.elapsed = self.started.elapsed();
        if success {
            self.ratio = 1.0;
        }
    }
}

pub fn subsequence(needle: &str, haystack: &str) -> bool {
    let mut needle = needle.chars().flat_map(char::to_lowercase);
    let mut wanted = needle.next();
    for got in haystack.chars().flat_map(char::to_lowercase) {
        if Some(got) == wanted {
            wanted = needle.next();
        }
    }
    wanted.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control() -> Control {
        Control {
            progress: Arc::new(Mutex::new(Progress::default())),
            cancelled: Arc::new(AtomicBool::new(false)),
            prompts: mpsc::channel().0,
        }
    }

    #[test]
    fn transport_counts_exclude_authentication_tags_and_require_commit() {
        let control = control();
        control.totals(10, 1);
        control.item("file".into(), 1, 10);
        let mut reader = TransferReader::new(io::Cursor::new(vec![0; 26]), &control, 10, 0, 0);
        reader.read_exact(&mut [0; 5]).unwrap();
        assert_eq!(control.snapshot().done, 5);
        reader.read_to_end(&mut Vec::new()).unwrap();
        let snapshot = control.snapshot();
        assert_eq!(
            (snapshot.done, snapshot.wire_bytes, snapshot.committed),
            (10, 26, 0)
        );
        control.commit(10);
        assert_eq!(control.snapshot().committed, 10);
        control.cancel();
        assert!(reader.read(&mut [0; 1]).is_err());
    }

    #[test]
    fn completion_and_stalls_are_not_invented_by_animation() {
        let mut view = TransferView::new(Direction::Upload);
        let start = view.started;
        let progress = Progress {
            total: Some(100),
            done: 100,
            wire_bytes: 116,
            phase: Phase::Sending,
            ..Progress::default()
        };
        view.tick(progress.clone(), false, start + Duration::from_secs(1));
        assert!(view.ratio < 1.0);
        view.tick(progress, false, start + Duration::from_secs(5));
        assert!(view.stalled());
        assert!(view.eta().is_none());
        view.finish(true);
        assert_eq!(view.ratio, 1.0);
    }
}
