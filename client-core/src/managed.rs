use crate::{
    Job,
    control::{Phase, Progress, Prompt, PromptKind, SecretKind, TransferEvent},
};
use anyhow::{Result, bail};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Running,
    Pausing,
    Paused,
    Complete,
    Failed,
}

/// Stable categories for native hosts; the message remains suitable for CLI output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobErrorKind {
    Cancelled,
    ResourceExhausted,
    InvalidInput,
    Network,
    Remote,
    Storage,
    Crypto,
    Internal,
}

impl JobErrorKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::ResourceExhausted => "resource-exhausted",
            Self::InvalidInput => "invalid-input",
            Self::Network => "network",
            Self::Remote => "remote",
            Self::Storage => "storage",
            Self::Crypto => "crypto",
            Self::Internal => "internal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptType {
    ShareKey,
    Password,
    PeerConsent,
    Directory,
    ShareReady,
}

#[derive(Clone, Debug)]
pub struct PendingPrompt {
    pub id: u64,
    pub kind: PromptType,
    pub peer: Option<String>,
}

#[derive(Clone, Debug)]
pub struct JobSnapshot {
    pub state: JobState,
    pub checkpoint_id: Option<String>,
    pub progress: Progress,
    pub prompt: Option<PendingPrompt>,
    pub share_url: Option<String>,
    pub results: Vec<String>,
    pub error: Option<String>,
    pub error_kind: Option<JobErrorKind>,
    pub peer_warning: Option<String>,
}

/// Non-blocking facade over the worker also used by the CLI. The host serializes
/// calls (the FFI object uses a Mutex) and may poll at its own cadence.
pub struct ManagedJob {
    job: Job,
    snapshot: JobSnapshot,
    reply: Option<Prompt>,
    next_prompt: u64,
}

impl ManagedJob {
    pub fn new(job: Job) -> Self {
        Self {
            job,
            snapshot: JobSnapshot {
                state: JobState::Running,
                checkpoint_id: None,
                progress: Progress::default(),
                prompt: None,
                share_url: None,
                results: Vec::new(),
                error: None,
                error_kind: None,
                peer_warning: None,
            },
            reply: None,
            next_prompt: 1,
        }
    }

    pub fn snapshot(&mut self) -> JobSnapshot {
        if matches!(self.snapshot.state, JobState::Running | JobState::Pausing) {
            self.snapshot.progress = self.job.control.snapshot();
            self.snapshot.checkpoint_id = self.job.control.checkpoint_id();
            while let Ok(event) = self.job.events.try_recv() {
                match event {
                    TransferEvent::ShareReady(value) => {
                        self.snapshot.share_url = Some(value.share_url)
                    }
                    TransferEvent::PeerFailed(value) => {
                        self.snapshot.peer_warning = Some(value.message)
                    }
                    TransferEvent::PeerConsent(_) => {} // The reply-bearing prompt is authoritative.
                }
            }
            if self.reply.is_none()
                && self.snapshot.state == JobState::Running
                && let Ok(prompt) = self.job.prompts.try_recv()
            {
                let (kind, peer) = match &prompt.kind {
                    PromptKind::Secret(SecretKind::ShareKey) => (PromptType::ShareKey, None),
                    PromptKind::Secret(SecretKind::Password) => (PromptType::Password, None),
                    PromptKind::PeerConsent { peer_id } => {
                        (PromptType::PeerConsent, Some(peer_id.clone()))
                    }
                    PromptKind::Directory { .. } => (PromptType::Directory, None),
                    PromptKind::ShareReady => (PromptType::ShareReady, None),
                };
                self.snapshot.prompt = Some(PendingPrompt {
                    id: self.next_prompt,
                    kind,
                    peer,
                });
                self.next_prompt += 1;
                self.reply = Some(prompt);
            }
            if let Some(result) = self.job.poll() {
                // The worker may have committed progress after the sample above.
                self.snapshot.progress = self.job.control.snapshot();
                self.snapshot.checkpoint_id = self.job.control.checkpoint_id();
                self.reply = None;
                self.snapshot.prompt = None;
                match result {
                    Ok(results) => {
                        self.snapshot.state = JobState::Complete;
                        self.snapshot.results = results;
                    }
                    Err(error) if error.is::<crate::control::Cancelled>() => {
                        self.snapshot.state = JobState::Paused;
                        self.snapshot.error_kind = Some(JobErrorKind::Cancelled);
                    }
                    Err(error) => {
                        self.snapshot.state = JobState::Failed;
                        self.snapshot.error_kind = Some(error_kind(&error));
                        self.snapshot.error = Some(error.to_string());
                    }
                }
            }
        }
        self.snapshot.clone()
    }

    pub fn respond(&mut self, id: u64, value: String) -> Result<()> {
        let value = Zeroizing::new(value);
        if self.snapshot.prompt.as_ref().map(|prompt| prompt.id) != Some(id) {
            bail!("This transfer prompt is no longer active");
        }
        let prompt = self
            .reply
            .take()
            .ok_or_else(|| anyhow::anyhow!("Transfer prompt is closed"))?;
        self.snapshot.prompt = None;
        prompt
            .reply
            .send(value)
            .map_err(|_| anyhow::anyhow!("Transfer prompt is closed"))
    }

    pub fn pause(&mut self) {
        if self.snapshot.state == JobState::Running {
            self.snapshot.state = JobState::Pausing;
            self.job.control.cancel();
            self.snapshot.prompt = None;
            self.reply = None;
        }
    }
}

fn error_kind(error: &anyhow::Error) -> JobErrorKind {
    if let Some(error) = error.downcast_ref::<crate::JobError>() {
        return error.kind;
    }
    if error.is::<crate::control::MemoryExhausted>() {
        return JobErrorKind::ResourceExhausted;
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("scheduler") || message.contains("shared scheduler") {
        JobErrorKind::ResourceExhausted
    } else if message.contains("invalid")
        || message.contains("must ")
        || message.contains("provide ")
    {
        JobErrorKind::InvalidInput
    } else if message.contains("network")
        || message.contains("connection")
        || message.contains("timeout")
    {
        JobErrorKind::Network
    } else if message.contains("server returned") || message.contains("http status") {
        JobErrorKind::Remote
    } else if message.contains("checkpoint")
        || message.contains("read ")
        || message.contains("write ")
    {
        JobErrorKind::Storage
    } else if message.contains("decrypt") || message.contains("encrypt") || message.contains("key")
    {
        JobErrorKind::Crypto
    } else {
        JobErrorKind::Internal
    }
}

/// Stable presentation identifiers; translated labels belong to each platform.
pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Connecting => "connecting",
        Phase::Preparing => "preparing",
        Phase::Archiving => "archiving",
        Phase::Encrypting => "encrypting",
        Phase::Sending => "sending",
        Phase::Receiving => "receiving",
        Phase::Verifying => "verifying",
        Phase::Finalizing => "finalizing",
        Phase::Waiting => "waiting",
        Phase::Unlocking => "unlocking",
        Phase::Updating => "updating",
        Phase::Retrying => "retrying",
        Phase::Reconnecting => "reconnecting",
        Phase::Storing => "storing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{Control, TransferSettings};
    use std::time::{Duration, Instant};

    fn job(run: impl FnOnce(&Control) -> Result<Vec<String>> + Send + 'static) -> ManagedJob {
        ManagedJob::new(Job::spawn(
            TransferSettings {
                state_home: "transfers".into(),
                max_concurrency: Some(1),
                memory_budget: 128 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            run,
        ))
    }

    fn wait(job: &mut ManagedJob, ready: impl Fn(&JobSnapshot) -> bool) -> JobSnapshot {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let state = job.snapshot();
            if ready(&state) {
                return state;
            }
            assert!(Instant::now() < until, "job timed out: {state:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn prompts_survive_observer_replacement_and_reject_stale_answers() {
        let mut job = job(|control| {
            assert_eq!(control.secret(SecretKind::Password)?.as_str(), "password");
            Ok(vec!["verified-file".into()])
        });
        let id = wait(&mut job, |s| s.prompt.is_some()).prompt.unwrap().id;
        assert_eq!(job.snapshot().prompt.unwrap().id, id);
        assert!(job.respond(id + 1, "wrong".into()).is_err());
        job.respond(id, "password".into()).unwrap();
        let done = wait(&mut job, |s| s.state == JobState::Complete);
        assert_eq!(done.results, ["verified-file"]);
        assert_eq!(job.snapshot().results, done.results);
        assert!(job.respond(id, "password".into()).is_err());
    }

    #[test]
    fn pausing_a_job_waiting_for_a_secret_releases_the_worker() {
        let mut job = job(|control| {
            control.secret(SecretKind::ShareKey)?;
            Ok(Vec::new())
        });
        wait(&mut job, |s| s.prompt.is_some());
        job.pause();
        let paused = wait(&mut job, |s| s.state == JobState::Paused);
        assert!(paused.prompt.is_none());
        assert!(paused.error.is_none());
    }

    #[test]
    fn progress_keeps_large_sizes_and_committed_bytes_separate() {
        let bytes = 5_u64 * 1024 * 1024 * 1024;
        let mut job = job(move |control| {
            control.set_checkpoint_id("saved-job".into());
            control.totals(bytes, 1);
            control.advance(bytes, bytes, bytes + 100);
            control.commit(bytes - 1);
            Ok(Vec::new())
        });
        let done = wait(&mut job, |s| s.state == JobState::Complete);
        assert_eq!(done.progress.total, Some(bytes));
        assert_eq!(done.progress.committed, bytes - 1);
        assert_eq!(done.checkpoint_id.as_deref(), Some("saved-job"));
    }
}
