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
    pub directory: Option<DirectoryPrompt>,
}

#[derive(Clone, Debug)]
pub struct DirectoryPrompt {
    pub files: u64,
    pub bytes: u64,
    pub maximum_files: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretRetryKind {
    ShareKey,
    Password,
    Generic,
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
    pub secret_retry: Option<SecretRetryKind>,
}

/// Non-blocking facade over the worker also used by the CLI. The host serializes
/// calls (the FFI object uses a Mutex) and may poll at its own cadence.
pub struct ManagedJob {
    job: Job,
    snapshot: JobSnapshot,
    reply: Option<Prompt>,
    next_prompt: u64,
    last_secret_kind: Option<SecretRetryKind>,
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
                secret_retry: None,
            },
            reply: None,
            next_prompt: 1,
            last_secret_kind: None,
        }
    }

    pub fn snapshot(&mut self) -> JobSnapshot {
        if matches!(self.snapshot.state, JobState::Running | JobState::Pausing) {
            self.snapshot.progress = self.job.control.snapshot();
            self.snapshot.checkpoint_id = self.job.control.checkpoint_id();
            self.drain_events();
            if self.reply.is_none()
                && self.snapshot.state == JobState::Running
                && let Ok(prompt) = self.job.prompts.try_recv()
            {
                let (kind, peer, directory) = match &prompt.kind {
                    PromptKind::Secret(SecretKind::ShareKey) => (PromptType::ShareKey, None, None),
                    PromptKind::Secret(SecretKind::Password) => (PromptType::Password, None, None),
                    PromptKind::PeerConsent { peer_id } => {
                        (PromptType::PeerConsent, Some(peer_id.clone()), None)
                    }
                    PromptKind::Directory {
                        files,
                        bytes,
                        maximum_files,
                    } => (
                        PromptType::Directory,
                        None,
                        Some(DirectoryPrompt {
                            files: *files as u64,
                            bytes: *bytes,
                            maximum_files: maximum_files.map(|value| value as u64),
                        }),
                    ),
                    PromptKind::ShareReady => (PromptType::ShareReady, None, None),
                };
                self.snapshot.prompt = Some(PendingPrompt {
                    id: self.next_prompt,
                    kind,
                    peer,
                    directory,
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
                // Completion may race the final publication event on the worker.
                self.drain_events();
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
                        // Android acceptance needs the OS error beneath storage
                        // context (for example a hard-link errno), not only the
                        // outer transfer message.
                        self.snapshot.error = Some(format!("{error:#}"));
                        if self.snapshot.error_kind == Some(JobErrorKind::Crypto) {
                            self.snapshot.secret_retry =
                                Some(self.last_secret_kind.unwrap_or(SecretRetryKind::Generic));
                        }
                    }
                }
            }
        }
        self.snapshot.clone()
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.job.events.try_recv() {
            match event {
                TransferEvent::ShareReady(value) => self.snapshot.share_url = Some(value.share_url),
                TransferEvent::PeerFailed(value) => {
                    self.snapshot.peer_warning = Some(value.message)
                }
                TransferEvent::PeerConsent(_) => {} // The reply-bearing prompt is authoritative.
            }
        }
    }

    pub fn respond(&mut self, id: u64, value: String) -> Result<()> {
        let value = Zeroizing::new(value);
        if self.snapshot.prompt.as_ref().map(|prompt| prompt.id) != Some(id) {
            bail!("This transfer prompt is no longer active");
        }
        if self
            .reply
            .as_ref()
            .is_some_and(|prompt| matches!(&prompt.kind, PromptKind::Directory { .. }))
        {
            bail!("Directory prompts require an explicit ZIP or individual-files choice");
        }
        let prompt = self
            .reply
            .take()
            .ok_or_else(|| anyhow::anyhow!("Transfer prompt is closed"))?;
        self.last_secret_kind = match &prompt.kind {
            PromptKind::Secret(SecretKind::ShareKey) => Some(SecretRetryKind::ShareKey),
            PromptKind::Secret(SecretKind::Password) => Some(SecretRetryKind::Password),
            _ => None,
        };
        self.snapshot.prompt = None;
        prompt
            .reply
            .send(value)
            .map_err(|_| anyhow::anyhow!("Transfer prompt is closed"))
    }

    pub fn respond_directory(&mut self, id: u64, zip: bool) -> Result<()> {
        if self.snapshot.prompt.as_ref().map(|prompt| prompt.id) != Some(id) {
            bail!("This transfer prompt is no longer active");
        }
        let prompt = self
            .reply
            .take()
            .ok_or_else(|| anyhow::anyhow!("Transfer prompt is closed"))?;
        if !matches!(&prompt.kind, PromptKind::Directory { .. }) {
            bail!("This transfer prompt does not accept a directory choice");
        }
        self.snapshot.prompt = None;
        self.last_secret_kind = None;
        prompt
            .reply
            .send(Zeroizing::new(if zip {
                "zip".into()
            } else {
                "individual".into()
            }))
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
    fn directory_prompts_require_a_bound_explicit_choice() {
        let mut job = job(|control| {
            let answer = control.ask(PromptKind::Directory {
                files: 3,
                bytes: 42,
                maximum_files: Some(5),
            })?;
            assert_eq!(answer.as_str(), "individual");
            Ok(Vec::new())
        });
        let prompt = wait(&mut job, |snapshot| snapshot.prompt.is_some())
            .prompt
            .unwrap();
        assert_eq!(prompt.directory.as_ref().unwrap().files, 3);
        assert!(job.respond(prompt.id, "yes".into()).is_err());
        // The rejected generic response must leave the same prompt actionable.
        job.respond_directory(prompt.id, false).unwrap();
        assert_eq!(
            wait(&mut job, |snapshot| snapshot.state == JobState::Complete).state,
            JobState::Complete
        );
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

    #[test]
    fn terminal_snapshot_drains_a_final_share_ready_event() {
        let mut job = job(|control| {
            control.emit(TransferEvent::ShareReady(crate::control::ShareReady {
                share_url: "https://example.test/HTTP".into(),
            }));
            Ok(vec!["https://example.test/HTTP".into()])
        });
        let done = wait(&mut job, |snapshot| snapshot.state == JobState::Complete);
        assert_eq!(done.share_url.as_deref(), Some("https://example.test/HTTP"));
    }
}
