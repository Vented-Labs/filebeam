use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    time::Duration,
};

use anyhow::{Result, bail};
use zeroize::Zeroizing;

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
    Retrying,
    Reconnecting,
    Storing,
}

impl Phase {
    pub fn label(&self) -> &'static str {
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
            Self::Retrying => "Retrying transfer",
            Self::Reconnecting => "Reconnecting",
            Self::Storing => "Saving encrypted transfer state",
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

#[derive(Clone)]
pub enum PromptKind {
    Secret(SecretKind),
    Directory {
        files: usize,
        bytes: u64,
        maximum_files: Option<usize>,
    },
    ShareReady,
    PeerConsent {
        peer_id: String,
    },
}
impl PromptKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Secret(kind) => kind.label(),
            Self::Directory { .. } => "Directory upload",
            Self::ShareReady => "Share ready",
            Self::PeerConsent { .. } => "Peer consent",
        }
    }
}

pub struct Prompt {
    pub kind: PromptKind,
    pub reply: Sender<Zeroizing<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareReady {
    pub share_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerConsent {
    pub peer_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerFailed {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferEvent {
    ShareReady(ShareReady),
    PeerConsent(PeerConsent),
    PeerFailed(PeerFailed),
}

#[derive(Debug, thiserror::Error)]
#[error("Transfer cancelled")]
pub struct Cancelled;

#[derive(Debug, thiserror::Error)]
#[error("transfer memory budget is exhausted")]
pub struct MemoryExhausted;

#[derive(Clone)]
pub struct MemoryBudget {
    state: Arc<(Mutex<MemoryState>, Condvar)>,
}

struct MemoryState {
    limit: u64,
    used: u64,
}

impl MemoryBudget {
    pub fn new(limit: u64) -> Self {
        Self {
            state: Arc::new((Mutex::new(MemoryState { limit, used: 0 }), Condvar::new())),
        }
    }

    pub fn reserve(&self, bytes: u64, cancelled: &AtomicBool) -> Result<MemoryPermit> {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if bytes > state.limit {
            return Err(MemoryExhausted.into());
        }
        while state.used > state.limit - bytes {
            if cancelled.load(Ordering::Relaxed) {
                return Err(Cancelled.into());
            }
            let (next, _) = wake
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|error| error.into_inner());
            state = next;
        }
        state.used += bytes;
        Ok(MemoryPermit {
            budget: self.clone(),
            bytes,
        })
    }

    /// Non-job service calls have no cancellation handle, so never wait behind
    /// a transfer: report admission pressure synchronously instead.
    pub fn try_reserve(&self, bytes: u64) -> Result<MemoryPermit> {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if bytes > state.limit || state.used > state.limit - bytes {
            return Err(MemoryExhausted.into());
        }
        state.used += bytes;
        Ok(MemoryPermit {
            budget: self.clone(),
            bytes,
        })
    }
}

pub struct MemoryPermit {
    budget: MemoryBudget,
    bytes: u64,
}

impl Drop for MemoryPermit {
    fn drop(&mut self) {
        let (lock, wake) = &*self.budget.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.used -= self.bytes;
        wake.notify_all();
    }
}

#[derive(Clone)]
pub struct TransferSettings {
    pub state_home: PathBuf,
    pub max_concurrency: Option<u32>,
    pub memory_budget: u64,
    pub client_user_agent: Option<String>,
    pub webrtc_relay_only: bool,
    pub checkpoint_secret_store: Option<Arc<dyn crate::checkpoint::SecretStore>>,
    pub source_resolver: Option<Arc<dyn crate::source::SourceResolver>>,
}

#[derive(Clone)]
pub struct Control {
    progress: Arc<Mutex<Progress>>,
    checkpoint_id: Arc<Mutex<Option<String>>>,
    pub cancelled: Arc<AtomicBool>,
    prompts: Sender<Prompt>,
    events: Option<Sender<TransferEvent>>,
    memory: Arc<Mutex<MemoryBudget>>,
    settings: TransferSettings,
}

impl Control {
    pub fn new(settings: TransferSettings, prompts: Sender<Prompt>) -> Self {
        let memory = MemoryBudget::new(settings.memory_budget);
        Self {
            progress: Arc::new(Mutex::new(Progress::default())),
            checkpoint_id: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(AtomicBool::new(false)),
            prompts,
            events: None,
            memory: Arc::new(Mutex::new(memory)),
            settings,
        }
    }
    /// Attaches an advisory event stream. A disconnected consumer must never
    /// affect a running transfer.
    pub fn with_events(
        settings: TransferSettings,
        prompts: Sender<Prompt>,
        events: Sender<TransferEvent>,
    ) -> Self {
        let mut control = Self::new(settings, prompts);
        control.events = Some(events);
        control
    }
    pub fn test_factory() -> Self {
        Self::new(
            TransferSettings {
                state_home: PathBuf::from("transfers"),
                max_concurrency: None,
                memory_budget: 512 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            mpsc::channel().0,
        )
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        Ok(())
    }
    pub fn transfer_home(&self) -> PathBuf {
        self.settings.state_home.clone()
    }
    pub fn create_checkpoint_store(&self, id: &str) -> Result<crate::checkpoint::Store> {
        let root = self.transfer_home();
        let secrets = self
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| crate::checkpoint::FilesystemSecretStore::for_state_root(&root));
        crate::checkpoint::Store::create_with_secret_store(&root, id, secrets)
    }
    pub fn open_checkpoint_store(&self, id: &str) -> Result<crate::checkpoint::Store> {
        let root = self.transfer_home();
        let secrets = self
            .settings
            .checkpoint_secret_store
            .clone()
            .unwrap_or_else(|| crate::checkpoint::FilesystemSecretStore::for_state_root(&root));
        crate::checkpoint::Store::open_with_secret_store(&root, id, secrets)
    }
    /// Advertise only a durable checkpoint that the host can actually resume.
    pub fn set_checkpoint_id(&self, id: String) {
        *self.checkpoint_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
    }
    pub fn checkpoint_id(&self) -> Option<String> {
        self.checkpoint_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn max_concurrency(&self) -> Option<u32> {
        self.settings.max_concurrency
    }
    pub fn memory_budget(&self) -> u64 {
        self.settings.memory_budget
    }
    /// Replaces the local test/default pool with the application-owned scheduler pool.
    pub fn set_memory_budget(&self, memory: MemoryBudget) {
        *self
            .memory
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = memory;
    }
    pub fn reserve_memory(&self, bytes: u64) -> Result<MemoryPermit> {
        self.memory
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .reserve(bytes, &self.cancelled)
    }
    pub fn client_user_agent(&self) -> &str {
        self.settings
            .client_user_agent
            .as_deref()
            .unwrap_or(concat!(
                "filebeam-transfer-native/",
                env!("CARGO_PKG_VERSION")
            ))
    }
    pub fn webrtc_relay_only(&self) -> bool {
        self.settings.webrtc_relay_only
    }
    pub fn source_resolver(&self) -> Option<&dyn crate::source::SourceResolver> {
        self.settings.source_resolver.as_deref()
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn emit(&self, event: TransferEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
    pub fn request_peer_consent(&self, peer_id: String) -> Result<()> {
        self.emit(TransferEvent::PeerConsent(PeerConsent {
            peer_id: peer_id.clone(),
        }));
        let consent = self.ask(PromptKind::PeerConsent { peer_id })?;
        if matches!(
            consent.trim().to_ascii_lowercase().as_str(),
            "allow" | "yes"
        ) {
            Ok(())
        } else {
            bail!("peer address exposure was not accepted")
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_has_safe_transfer_defaults() {
        let control = Control::test_factory();
        assert_eq!(control.transfer_home(), PathBuf::from("transfers"));
        assert_eq!(control.memory_budget(), 512 * 1024 * 1024);
    }

    #[test]
    fn memory_permits_are_bounded_and_cancellation_aware() {
        let budget = MemoryBudget::new(10);
        let cancelled = AtomicBool::new(false);
        let held = budget.reserve(10, &cancelled).unwrap();
        let waiting = {
            let budget = budget.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(25));
                budget
            })
        };
        let waiting = waiting.join().unwrap();
        cancelled.store(true, Ordering::Relaxed);
        assert!(waiting.reserve(1, &cancelled).is_err());
        drop(held);
        let active = AtomicBool::new(false);
        assert!(budget.reserve(10, &active).is_ok());
    }

    #[test]
    fn events_are_advisory() {
        let (prompts, _) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let control = Control::with_events(
            TransferSettings {
                state_home: PathBuf::from("transfers"),
                max_concurrency: None,
                memory_budget: 512 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            prompts,
            events,
        );
        control.emit(TransferEvent::ShareReady(ShareReady {
            share_url: "https://example.test/share".into(),
        }));
        assert!(matches!(
            receiver.recv().unwrap(),
            TransferEvent::ShareReady(_)
        ));
    }

    #[test]
    fn peer_failure_is_an_advisory_event() {
        let (prompts, _) = mpsc::channel();
        let (events, receiver) = mpsc::channel();
        let control = Control::with_events(
            TransferSettings {
                state_home: PathBuf::from("transfers"),
                max_concurrency: None,
                memory_budget: 512 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            prompts,
            events,
        );
        control.emit(TransferEvent::PeerFailed(PeerFailed {
            message: "A live receiver disconnected or failed.".into(),
        }));
        assert!(matches!(
            receiver.recv().unwrap(),
            TransferEvent::PeerFailed(PeerFailed { message }) if message == "A live receiver disconnected or failed."
        ));
    }
}
