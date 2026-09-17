use crate::{
    DirectoryChoice, DirectoryPrompt, ErrorCategory, JobState, PendingPrompt, PromptType, Result,
    SecretRetryKind, TransferSnapshot, operation,
};
use filebeam_client_core as core;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(uniffi::Object)]
pub struct TransferJob {
    inner: Mutex<core::ManagedJob>,
    end_requested: Option<Arc<AtomicBool>>,
    direction: &'static str,
    kind: &'static str,
    transport: &'static str,
}

impl TransferJob {
    pub(crate) fn new(job: core::Job) -> Self {
        Self {
            inner: Mutex::new(core::ManagedJob::new(job)),
            end_requested: None,
            direction: "unknown",
            kind: "unknown",
            transport: "unknown",
        }
    }
    pub(crate) fn new_upload(job: core::Job, transport: &'static str) -> Self {
        Self {
            inner: Mutex::new(core::ManagedJob::new(job)),
            end_requested: None,
            direction: "upload",
            kind: "files",
            transport,
        }
    }
    pub(crate) fn new_live(job: core::Job, end_requested: Arc<AtomicBool>) -> Self {
        Self {
            inner: Mutex::new(core::ManagedJob::new(job)),
            end_requested: Some(end_requested),
            direction: "upload",
            kind: "note",
            transport: "webrtc",
        }
    }
}

#[uniffi::export]
impl TransferJob {
    pub fn snapshot(&self) -> TransferSnapshot {
        let state = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot();
        TransferSnapshot {
            state: match state.state {
                core::JobState::Running => JobState::Running,
                core::JobState::Pausing => JobState::Pausing,
                core::JobState::Paused => JobState::Paused,
                core::JobState::Complete => JobState::Complete,
                core::JobState::Failed => JobState::Failed,
            },
            checkpoint_id: state.checkpoint_id,
            phase: core::phase_name(state.progress.phase).into(),
            file_name: state.progress.name,
            file_index: state.progress.index as u64,
            file_count: state.progress.files as u64,
            total: state.progress.total,
            done: state.progress.done,
            committed: state.progress.committed,
            wire_bytes: state.progress.wire_bytes,
            prompt: state.prompt.map(|prompt| PendingPrompt {
                id: prompt.id,
                peer: prompt.peer,
                kind: match prompt.kind {
                    core::PromptType::ShareKey => PromptType::ShareKey,
                    core::PromptType::Password => PromptType::Password,
                    core::PromptType::PeerConsent => PromptType::PeerConsent,
                    core::PromptType::Directory => PromptType::Directory,
                    core::PromptType::ShareReady => PromptType::ShareReady,
                },
                directory: prompt.directory.map(|directory| DirectoryPrompt {
                    files: directory.files,
                    bytes: directory.bytes,
                    maximum_files: directory.maximum_files,
                }),
            }),
            share_url: state.share_url,
            results: state.results,
            error: state.error,
            error_category: state.error_kind.map(|kind| match kind {
                core::JobErrorKind::Cancelled => ErrorCategory::Cancelled,
                core::JobErrorKind::ResourceExhausted => ErrorCategory::ResourceExhausted,
                core::JobErrorKind::InvalidInput => ErrorCategory::InvalidInput,
                core::JobErrorKind::Network => ErrorCategory::Network,
                core::JobErrorKind::Remote => ErrorCategory::Remote,
                core::JobErrorKind::Storage => ErrorCategory::Storage,
                core::JobErrorKind::Crypto => ErrorCategory::Crypto,
                core::JobErrorKind::Internal => ErrorCategory::Internal,
            }),
            peer_warning: state.peer_warning,
            secret_retry: state.secret_retry.map(|retry| match retry {
                core::SecretRetryKind::ShareKey => SecretRetryKind::ShareKey,
                core::SecretRetryKind::Password => SecretRetryKind::Password,
                core::SecretRetryKind::Generic => SecretRetryKind::Generic,
            }),
            direction: self.direction.into(),
            kind: self.kind.into(),
            transport: self.transport.into(),
            can_pause: matches!(state.state, core::JobState::Running),
            can_end_live: self.end_requested.is_some()
                && matches!(state.state, core::JobState::Running),
            // Upload delete capabilities are checkpoint-private and are only
            // exposed by authenticated SavedTransferDetails after persistence.
            can_revoke_remote: false,
            expiry_known: false,
        }
    }
    pub fn respond(&self, prompt_id: u64, value: String) -> Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .respond(prompt_id, value)
            .map_err(operation)
    }
    pub fn respond_directory(&self, prompt_id: u64, choice: DirectoryChoice) -> Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .respond_directory(prompt_id, matches!(choice, DirectoryChoice::Zip))
            .map_err(operation)
    }
    pub fn respond_consent(&self, prompt_id: u64, allowed: bool) -> Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .respond_consent(prompt_id, allowed)
            .map_err(operation)
    }
    pub fn pause(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).pause();
    }
    /// Explicitly ends the remote live reservation. `pause` only stops local serving.
    pub fn end_live(&self) {
        if let Some(end) = &self.end_requested {
            end.store(true, Ordering::Relaxed);
        }
        self.pause();
    }
}
