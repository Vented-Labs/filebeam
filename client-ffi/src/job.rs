use crate::{
    ErrorCategory, JobState, PendingPrompt, PromptType, Result, TransferSnapshot, operation,
};
use filebeam_client_core as core;
use std::sync::Mutex;

#[derive(uniffi::Object)]
pub struct TransferJob {
    inner: Mutex<core::ManagedJob>,
}

impl TransferJob {
    pub(crate) fn new(job: core::Job) -> Self {
        Self {
            inner: Mutex::new(core::ManagedJob::new(job)),
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
        }
    }
    pub fn respond(&self, prompt_id: u64, value: String) -> Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .respond(prompt_id, value)
            .map_err(operation)
    }
    pub fn pause(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).pause();
    }
}
