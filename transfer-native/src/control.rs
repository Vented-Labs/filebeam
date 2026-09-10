use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
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

#[derive(Clone, Copy)]
pub enum PromptKind {
    Secret(SecretKind),
    Directory {
        files: usize,
        bytes: u64,
        maximum_files: Option<usize>,
    },
    ShareReady,
    PeerConsent,
}
impl PromptKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Secret(kind) => kind.label(),
            Self::Directory { .. } => "Directory upload",
            Self::ShareReady => "Share ready",
            Self::PeerConsent => "Peer consent",
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
pub enum TransferEvent {
    ShareReady(ShareReady),
    PeerConsent(PeerConsent),
}

#[derive(Debug, thiserror::Error)]
#[error("Transfer cancelled")]
pub struct Cancelled;

#[derive(Clone, Debug)]
pub struct TransferSettings {
    pub state_home: PathBuf,
    pub max_concurrency: Option<u32>,
    pub memory_budget: u64,
    pub client_user_agent: Option<String>,
}

#[derive(Clone)]
pub struct Control {
    progress: Arc<Mutex<Progress>>,
    pub cancelled: Arc<AtomicBool>,
    prompts: Sender<Prompt>,
    settings: TransferSettings,
}

impl Control {
    pub fn new(settings: TransferSettings, prompts: Sender<Prompt>) -> Self {
        Self {
            progress: Arc::new(Mutex::new(Progress::default())),
            cancelled: Arc::new(AtomicBool::new(false)),
            prompts,
            settings,
        }
    }
    pub fn test_factory() -> Self {
        Self::new(
            TransferSettings {
                state_home: PathBuf::from("transfers"),
                max_concurrency: None,
                memory_budget: 512 * 1024 * 1024,
                client_user_agent: None,
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
    pub fn max_concurrency(&self) -> Option<u32> {
        self.settings.max_concurrency
    }
    pub fn memory_budget(&self) -> u64 {
        self.settings.memory_budget
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_has_safe_transfer_defaults() {
        let control = Control::test_factory();
        assert_eq!(control.transfer_home(), PathBuf::from("transfers"));
        assert_eq!(control.memory_budget(), 512 * 1024 * 1024);
    }
}
