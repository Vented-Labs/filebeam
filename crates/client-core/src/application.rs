//! Process-owned application orchestration for native clients.
//!
//! Windows and views hold [`JobObservation`] values. Jobs remain owned by
//! [`ClientRuntime`] until explicitly discarded, so closing a window cannot stop
//! a transfer. Hosts provide persistence and destination selection; this module
//! intentionally has no UI or configuration-file dependency.

use crate::{
    Job, JobSnapshot, JobState, ManagedJob, Request, Scheduler, SchedulerLimits,
    control::TransferSettings,
};
use anyhow::{Result, bail};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Settings supplied by a host configuration layer. Callback-backed secret and
/// source access lives in `transfer`, keeping this crate independent of TOML.
#[derive(Clone)]
pub struct ApplicationSettings {
    pub transfer: TransferSettings,
    pub scheduler: SchedulerLimits,
    ice_override: Option<filebeam_transfer_native::ice::IceOverride>,
}

impl ApplicationSettings {
    pub fn new(transfer: TransferSettings, scheduler: SchedulerLimits) -> Self {
        Self {
            transfer,
            scheduler,
            ice_override: None,
        }
    }
    /// Attaches host configuration to newly created controls without sharing it
    /// outside this runtime.
    pub fn with_ice_override(mut self, policy: filebeam_transfer_native::ice::IceOverride) -> Self {
        self.ice_override = Some(policy);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Upload,
    Download,
    Resume,
    Service,
    Note,
    LiveNote,
    Revoke,
    EndLive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportState {
    NotVerified,
    AwaitingDestination {
        paths: Vec<String>,
    },
    ExportFailed {
        paths: Vec<String>,
        message: String,
    },
    Exported {
        destination: PathBuf,
        paths: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionCapabilities {
    pub pause: bool,
    pub resume: bool,
    pub end_live: bool,
    pub revoke_remote: bool,
    pub discard: bool,
    pub export: bool,
    pub retry_export: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobAction {
    Pause,
    Resume,
    EndLive,
    RevokeRemote,
    Discard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptIdentity {
    pub job: JobId,
    pub prompt: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptResponse {
    Secret(String),
    Directory { zip: bool },
    Consent { allowed: bool },
}

#[derive(Clone, Debug)]
pub struct ApplicationJobSnapshot {
    pub id: JobId,
    pub kind: JobKind,
    pub origin: Option<String>,
    pub job: JobSnapshot,
    pub export: ExportState,
    pub actions: ActionCapabilities,
}

#[derive(Clone, Debug, Default)]
pub struct ShutdownReport {
    pub checkpointed: Vec<JobId>,
    pub still_stopping: Vec<JobId>,
}

struct Entry {
    kind: JobKind,
    origin: Option<String>,
    job: EntryJob,
    export: ExportState,
    recovered_actions: Option<ActionCapabilities>,
}

/// An authenticated checkpoint represented without a worker. This is important
/// after a process restart: a row is not evidence that work is in progress.
struct StoredJob {
    snapshot: JobSnapshot,
}

enum EntryJob {
    Running(Box<ManagedJob>),
    Stored(Box<StoredJob>),
}

impl EntryJob {
    fn snapshot(&mut self) -> JobSnapshot {
        match self {
            Self::Running(job) => job.snapshot(),
            Self::Stored(job) => job.snapshot.clone(),
        }
    }

    fn pause(&mut self) -> Result<()> {
        match self {
            Self::Running(job) => {
                job.pause();
                Ok(())
            }
            Self::Stored(_) => bail!("pause is not available for a recovered transfer"),
        }
    }
}

struct Inner {
    future_transfer_settings: RwLock<FutureTransferSettings>,
    scheduler: Scheduler,
    next_id: AtomicU64,
    jobs: Mutex<HashMap<JobId, Entry>>,
}

#[derive(Clone)]
struct FutureTransferSettings {
    transfer: TransferSettings,
    ice_override: Option<filebeam_transfer_native::ice::IceOverride>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsReload {
    pub restart_required: bool,
}

/// A process/service-owned runtime. Cloning it or creating observations does not
/// create job ownership; call [`ClientRuntime::discard`] to release a job.
#[derive(Clone)]
pub struct ClientRuntime {
    inner: Arc<Inner>,
}

/// Read-only subscription token. It can be dropped whenever a window closes.
#[derive(Clone)]
pub struct JobObservation {
    runtime: ClientRuntime,
    id: JobId,
}

impl ClientRuntime {
    pub fn new(settings: ApplicationSettings) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                scheduler: Scheduler::new(settings.scheduler)?,
                future_transfer_settings: RwLock::new(FutureTransferSettings {
                    transfer: settings.transfer,
                    ice_override: settings.ice_override,
                }),
                next_id: AtomicU64::new(1),
                jobs: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Creates an isolated job registry that shares the process scheduler.
    ///
    /// Transfer settings intentionally are not global: callers may belong to
    /// different profiles and therefore need distinct state homes and callbacks.
    /// The first caller fixes only the process-wide scheduler limits.
    pub fn process_global(settings: ApplicationSettings) -> Result<Self> {
        static SCHEDULER: std::sync::OnceLock<Mutex<Option<Scheduler>>> =
            std::sync::OnceLock::new();
        let mut scheduler = SCHEDULER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if scheduler.is_none() {
            *scheduler = Some(Scheduler::new(settings.scheduler)?);
        }
        let scheduler = scheduler
            .as_ref()
            .expect("scheduler was initialized")
            .clone();
        Ok(Self {
            inner: Arc::new(Inner {
                scheduler,
                future_transfer_settings: RwLock::new(FutureTransferSettings {
                    transfer: settings.transfer,
                    ice_override: settings.ice_override,
                }),
                next_id: AtomicU64::new(1),
                jobs: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn scheduler(&self) -> Scheduler {
        self.inner.scheduler.clone()
    }
    pub fn transfer_settings(&self) -> TransferSettings {
        self.future_settings().transfer
    }

    /// Updates only controls created after this call. Existing controls retain
    /// their original settings, callbacks, ICE policy, and job registry entries.
    pub fn reload_settings(&self, settings: ApplicationSettings) -> Result<SettingsReload> {
        let restart_required = self
            .inner
            .scheduler
            .reconfigure_limits(settings.scheduler)?;
        *self
            .inner
            .future_transfer_settings
            .write()
            .unwrap_or_else(|error| error.into_inner()) = FutureTransferSettings {
            transfer: settings.transfer,
            ice_override: settings.ice_override,
        };
        Ok(SettingsReload { restart_required })
    }

    pub fn start_upload(&self, request: Request) -> Result<JobObservation> {
        if !matches!(
            request,
            Request::Upload { .. } | Request::UploadSources { .. }
        ) {
            bail!("start_upload requires an upload request");
        }
        self.start_request(JobKind::Upload, request)
    }

    pub fn start_download(&self, request: Request) -> Result<JobObservation> {
        if !matches!(request, Request::Download { .. }) {
            bail!("start_download requires a download request");
        }
        self.start_request(JobKind::Download, request)
    }

    pub fn resume(&self, checkpoint_id: String) -> Result<JobObservation> {
        let settings = self.future_settings().transfer;
        let state_home = settings.state_home.clone();
        let details = crate::protocol::saved_transfer_details_with_secret_store(
            &state_home,
            &checkpoint_id,
            checkpoint_secret_store(&settings),
        )?;
        if !details.can_resume {
            bail!("saved transfer is not resumable");
        }
        let kind = match details.direction.as_str() {
            "upload" => JobKind::Upload,
            "download" => JobKind::Download,
            _ => bail!("saved transfer has an invalid direction"),
        };
        self.start_request(kind, Request::Resume { id: details.id })
    }

    /// Registers all authenticated checkpoint records as detached rows. No
    /// scheduler admission or network operation occurs here.
    pub fn restore_saved(&self) -> Result<Vec<JobObservation>> {
        let settings = self.future_settings().transfer;
        let details = crate::protocol::saved_transfers_with_secret_store(
            &settings.state_home,
            checkpoint_secret_store(&settings),
        )?
        .into_iter()
        .filter_map(|saved| {
            crate::protocol::saved_transfer_details_with_secret_store(
                &settings.state_home,
                &saved.id,
                checkpoint_secret_store(&settings),
            )
            .ok()
        })
        .collect::<Vec<_>>();
        Ok(details
            .into_iter()
            .filter(|detail| {
                !self
                    .inner
                    .jobs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .values_mut()
                    .any(|entry| entry.job.snapshot().checkpoint_id.as_deref() == Some(&detail.id))
            })
            .map(|detail| self.insert_recovered(detail))
            .collect())
    }

    pub fn start_service_job(
        &self,
        kind: JobKind,
        run: impl FnOnce(&crate::control::Control) -> Result<Vec<String>> + Send + 'static,
    ) -> JobObservation {
        let settings = self.future_settings();
        let job = Job::spawn_in_with_ice_override(
            &self.inner.scheduler,
            settings.transfer,
            None,
            settings.ice_override,
            run,
        );
        self.insert(kind, job)
    }

    pub fn start_revoke(
        &self,
        run: impl FnOnce(&crate::control::Control) -> Result<Vec<String>> + Send + 'static,
    ) -> JobObservation {
        self.start_service_job(JobKind::Revoke, run)
    }

    pub fn start_end_live(
        &self,
        run: impl FnOnce(&crate::control::Control) -> Result<Vec<String>> + Send + 'static,
    ) -> JobObservation {
        self.start_service_job(JobKind::EndLive, run)
    }

    fn start_request(&self, kind: JobKind, request: Request) -> Result<JobObservation> {
        let settings = self.future_settings();
        Ok(self.insert(
            kind,
            Job::start_in_with_ice_override(
                &self.inner.scheduler,
                settings.transfer,
                request,
                settings.ice_override,
            ),
        ))
    }

    fn future_settings(&self) -> FutureTransferSettings {
        self.inner
            .future_transfer_settings
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn insert(&self, kind: JobKind, job: Job) -> JobObservation {
        let id = JobId(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                Entry {
                    kind,
                    origin: None,
                    job: EntryJob::Running(Box::new(ManagedJob::new(job))),
                    export: ExportState::NotVerified,
                    recovered_actions: None,
                },
            );
        JobObservation {
            runtime: self.clone(),
            id,
        }
    }

    fn insert_recovered(
        &self,
        detail: filebeam_transfer_native::protocol::SavedTransferDetails,
    ) -> JobObservation {
        let id = JobId(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let kind = if detail.direction == "upload" {
            JobKind::Upload
        } else {
            JobKind::Download
        };
        let state = if detail.can_resume {
            JobState::Paused
        } else if detail.verified_privately || detail.exported {
            JobState::Complete
        } else {
            JobState::Failed
        };
        let progress = crate::control::Progress {
            done: detail.done,
            total: Some(detail.total),
            ..Default::default()
        };
        let export = if detail.verified_privately {
            ExportState::AwaitingDestination {
                paths: detail.verified_paths.clone(),
            }
        } else {
            ExportState::NotVerified
        };
        let actions = ActionCapabilities {
            resume: detail.can_resume,
            end_live: detail.can_end_live,
            revoke_remote: detail.can_revoke_remote,
            discard: detail.can_remove_local,
            export: detail.verified_privately,
            retry_export: detail.can_retry_save,
            ..ActionCapabilities::default()
        };
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                Entry {
                    kind,
                    origin: detail.origin,
                    job: EntryJob::Stored(Box::new(StoredJob {
                        snapshot: JobSnapshot {
                            state,
                            checkpoint_id: Some(detail.id),
                            progress,
                            prompt: None,
                            share_url: detail.receipt,
                            results: detail.verified_paths,
                            error: None,
                            error_kind: None,
                            peer_warning: None,
                            secret_retry: None,
                        },
                    })),
                    export,
                    recovered_actions: Some(actions),
                },
            );
        JobObservation {
            runtime: self.clone(),
            id,
        }
    }

    pub fn observe(&self, id: JobId) -> Option<JobObservation> {
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&id)
            .then(|| JobObservation {
                runtime: self.clone(),
                id,
            })
    }

    pub fn snapshots(&self) -> Vec<ApplicationJobSnapshot> {
        let ids = self
            .inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .copied()
            .collect::<Vec<_>>();
        ids.into_iter().filter_map(|id| self.snapshot(id)).collect()
    }

    pub fn snapshot(&self, id: JobId) -> Option<ApplicationJobSnapshot> {
        let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let entry = jobs.get_mut(&id)?;
        let job = entry.job.snapshot();
        if entry.kind == JobKind::Download
            && job.state == JobState::Complete
            && matches!(entry.export, ExportState::NotVerified)
        {
            entry.export = ExportState::AwaitingDestination {
                paths: job.results.clone(),
            };
        }
        let actions = entry
            .recovered_actions
            .unwrap_or_else(|| capabilities(entry.kind, &job, &entry.export));
        Some(ApplicationJobSnapshot {
            id,
            kind: entry.kind,
            origin: entry.origin.clone(),
            job,
            export: entry.export.clone(),
            actions,
        })
    }

    pub fn respond(&self, identity: PromptIdentity, response: PromptResponse) -> Result<()> {
        let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let entry = jobs
            .get_mut(&identity.job)
            .ok_or_else(|| anyhow::anyhow!("transfer is no longer available"))?;
        match response {
            PromptResponse::Secret(value) => match &mut entry.job {
                EntryJob::Running(job) => job.respond(identity.prompt, value),
                EntryJob::Stored(_) => bail!("recovered transfers have no active prompt"),
            },
            PromptResponse::Directory { zip } => match &mut entry.job {
                EntryJob::Running(job) => job.respond_directory(identity.prompt, zip),
                EntryJob::Stored(_) => bail!("recovered transfers have no active prompt"),
            },
            PromptResponse::Consent { allowed } => match &mut entry.job {
                EntryJob::Running(job) => job.respond_consent(identity.prompt, allowed),
                EntryJob::Stored(_) => bail!("recovered transfers have no active prompt"),
            },
        }
    }

    pub fn dispatch(&self, id: JobId, action: JobAction) -> Result<Option<JobObservation>> {
        match action {
            JobAction::Pause => {
                let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
                let entry = jobs
                    .get_mut(&id)
                    .ok_or_else(|| anyhow::anyhow!("transfer is no longer available"))?;
                if !entry
                    .recovered_actions
                    .unwrap_or_else(|| {
                        capabilities(entry.kind, &entry.job.snapshot(), &entry.export)
                    })
                    .pause
                {
                    bail!("pause is not available");
                }
                entry.job.pause()?;
                Ok(None)
            }
            JobAction::Resume => {
                let snapshot = self
                    .snapshot(id)
                    .ok_or_else(|| anyhow::anyhow!("transfer is no longer available"))?;
                if !snapshot.actions.resume {
                    bail!("resume is not available");
                }
                let checkpoint = snapshot
                    .job
                    .checkpoint_id
                    .expect("capability requires checkpoint");
                let settings = self.future_settings().transfer;
                let details = crate::protocol::saved_transfer_details_with_secret_store(
                    &settings.state_home,
                    &checkpoint,
                    checkpoint_secret_store(&settings),
                )?;
                if !details.can_resume {
                    bail!("saved transfer is not resumable");
                }
                let kind = if details.direction == "upload" {
                    JobKind::Upload
                } else {
                    JobKind::Download
                };
                let job = Job::start_in_with_ice_override(
                    &self.inner.scheduler,
                    settings,
                    Request::Resume { id: details.id },
                    self.future_settings().ice_override,
                );
                self.inner
                    .jobs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(
                        id,
                        Entry {
                            kind,
                            origin: snapshot.origin,
                            job: EntryJob::Running(Box::new(ManagedJob::new(job))),
                            export: snapshot.export,
                            recovered_actions: None,
                        },
                    );
                Ok(Some(JobObservation {
                    runtime: self.clone(),
                    id,
                }))
            }
            JobAction::Discard => {
                self.discard(id)?;
                Ok(None)
            }
            JobAction::EndLive | JobAction::RevokeRemote => {
                bail!("start end/revoke through its service job closure")
            }
        }
    }

    /// Drops local ownership only after a terminal state. Call the native
    /// checkpoint discard operation separately when recovery files must be removed.
    pub fn discard(&self, id: JobId) -> Result<()> {
        let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let entry = jobs
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("transfer is no longer available"))?;
        if !entry
            .recovered_actions
            .unwrap_or_else(|| capabilities(entry.kind, &entry.job.snapshot(), &entry.export))
            .discard
        {
            bail!("discard is not available");
        }
        let checkpoint = entry.job.snapshot().checkpoint_id;
        drop(jobs);
        if let Some(checkpoint) = checkpoint {
            crate::protocol::discard_transfer(
                &self.future_settings().transfer.state_home,
                &checkpoint,
            )?;
        }
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        Ok(())
    }

    /// Runs the host's destination commit. A failure leaves verified paths in
    /// `ExportFailed`, allowing a safe retry without re-downloading.
    pub fn commit_export(
        &self,
        id: JobId,
        destination: PathBuf,
        commit: impl FnOnce(&[String], &PathBuf) -> Result<()>,
    ) -> Result<()> {
        let paths = match self
            .snapshot(id)
            .ok_or_else(|| anyhow::anyhow!("transfer is no longer available"))?
            .export
        {
            ExportState::AwaitingDestination { paths }
            | ExportState::ExportFailed { paths, .. } => paths,
            _ => bail!("verified results are not awaiting export"),
        };
        let result = commit(&paths, &destination);
        let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let entry = jobs
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("transfer was discarded during export"))?;
        entry.export = match result {
            Ok(()) => ExportState::Exported { destination, paths },
            Err(error) => ExportState::ExportFailed {
                paths,
                message: format!("{error:#}"),
            },
        };
        match &entry.export {
            ExportState::ExportFailed { message, .. } => bail!("{message}"),
            _ => Ok(()),
        }
    }

    /// Host-owned journal recovery may restore only a previously committed
    /// destination state; it never changes native checkpoint verification.
    pub fn restore_export_state(&self, id: JobId, export: ExportState) -> Result<()> {
        let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let entry = jobs
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("transfer is no longer available"))?;
        if !matches!(entry.job, EntryJob::Stored(_)) {
            bail!("export recovery applies only to detached transfers");
        }
        entry.export = export;
        Ok(())
    }

    /// Explicit quit requests cancellation/checkpointing and waits only for the
    /// supplied bound. Closing a window should not call this method.
    pub fn shutdown(&self, timeout: Duration) -> ShutdownReport {
        let ids = self
            .inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let requested = ids
            .into_iter()
            .filter(|id| self.dispatch(*id, JobAction::Pause).is_ok())
            .collect::<Vec<_>>();
        let until = Instant::now() + timeout;
        loop {
            let (checkpointed, pending) = requested.iter().copied().fold(
                (Vec::new(), Vec::new()),
                |(mut checkpointed, mut pending), id| {
                    match self.snapshot(id).map(|snapshot| snapshot.job.state) {
                        Some(JobState::Paused) => checkpointed.push(id),
                        Some(JobState::Running | JobState::Pausing) => pending.push(id),
                        // A job that completed or failed while cancellation was
                        // in flight did not produce a shutdown checkpoint.
                        Some(JobState::Complete | JobState::Failed) | None => {}
                    }
                    (checkpointed, pending)
                },
            );
            if pending.is_empty() {
                return ShutdownReport {
                    checkpointed,
                    still_stopping: Vec::new(),
                };
            }
            if Instant::now() >= until {
                return ShutdownReport {
                    checkpointed,
                    still_stopping: pending,
                };
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl JobObservation {
    pub fn id(&self) -> JobId {
        self.id
    }
    pub fn snapshot(&self) -> Option<ApplicationJobSnapshot> {
        self.runtime.snapshot(self.id)
    }
}

fn capabilities(kind: JobKind, job: &JobSnapshot, export: &ExportState) -> ActionCapabilities {
    let terminal = matches!(
        job.state,
        JobState::Paused | JobState::Complete | JobState::Failed
    );
    ActionCapabilities {
        pause: job.state == JobState::Running,
        resume: job.state == JobState::Paused && job.checkpoint_id.is_some(),
        end_live: kind == JobKind::LiveNote && job.state == JobState::Running,
        revoke_remote: matches!(kind, JobKind::Note | JobKind::LiveNote)
            && matches!(job.state, JobState::Running | JobState::Complete),
        discard: terminal,
        export: matches!(export, ExportState::AwaitingDestination { .. }),
        retry_export: matches!(export, ExportState::ExportFailed { .. }),
    }
}

fn checkpoint_secret_store(
    settings: &TransferSettings,
) -> std::sync::Arc<dyn filebeam_transfer_native::checkpoint::SecretStore> {
    settings.checkpoint_secret_store.clone().unwrap_or_else(|| {
        filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
            &settings.state_home,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{Control, SecretKind};
    use filebeam_transfer_native::checkpoint::Store;
    struct MockSecretStore;
    impl filebeam_transfer_native::checkpoint::SecretStore for MockSecretStore {
        fn load_or_create(&self, scope: &str) -> Result<[u8; 32]> {
            if scope == "checkpoint-catalog-v1" {
                Ok([9; 32])
            } else {
                bail!("unexpected scope")
            }
        }
        fn remove(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }
    fn runtime() -> ClientRuntime {
        ClientRuntime::new(ApplicationSettings::new(
            TransferSettings {
                state_home: PathBuf::from("transfers"),
                max_concurrency: Some(1),
                memory_budget: 128 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            SchedulerLimits {
                workers: 1,
                memory_bytes: 256 * 1024 * 1024,
            },
        ))
        .unwrap()
    }
    fn wait(
        runtime: &ClientRuntime,
        id: JobId,
        f: impl Fn(&ApplicationJobSnapshot) -> bool,
    ) -> ApplicationJobSnapshot {
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            let s = runtime.snapshot(id).unwrap();
            if f(&s) {
                return s;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn subscriber_drop_does_not_cancel_owned_job() {
        let runtime = runtime();
        let observation =
            runtime.start_service_job(JobKind::Service, |_| Ok(vec!["verified".into()]));
        let id = observation.id();
        drop(observation);
        assert_eq!(
            wait(&runtime, id, |s| s.job.state == JobState::Complete)
                .job
                .results,
            ["verified"]
        );
    }
    #[test]
    fn stale_and_double_prompt_responses_are_rejected() {
        let runtime = runtime();
        let job = runtime.start_service_job(JobKind::Service, |control: &Control| {
            control.secret(SecretKind::Password)?;
            Ok(Vec::new())
        });
        let id = job.id();
        let prompt = wait(&runtime, id, |s| s.job.prompt.is_some())
            .job
            .prompt
            .unwrap()
            .id;
        assert!(
            runtime
                .respond(
                    PromptIdentity {
                        job: id,
                        prompt: prompt + 1
                    },
                    PromptResponse::Secret("bad".into())
                )
                .is_err()
        );
        runtime
            .respond(
                PromptIdentity { job: id, prompt },
                PromptResponse::Secret("ok".into()),
            )
            .unwrap();
        assert!(
            runtime
                .respond(
                    PromptIdentity { job: id, prompt },
                    PromptResponse::Secret("again".into())
                )
                .is_err()
        );
    }
    #[test]
    fn action_guards_and_export_retry_preserve_verified_results() {
        let runtime = runtime();
        let job = runtime.start_service_job(JobKind::Download, |_| Ok(vec!["verified".into()]));
        let id = job.id();
        let done = wait(&runtime, id, |s| s.job.state == JobState::Complete);
        assert!(done.actions.export);
        assert!(!done.actions.pause);
        assert!(runtime.dispatch(id, JobAction::Pause).is_err());
        assert!(
            runtime
                .commit_export(id, PathBuf::from("/dest"), |_, _| bail!("disk full"))
                .is_err()
        );
        let failed = runtime.snapshot(id).unwrap();
        assert!(failed.actions.retry_export);
        assert!(
            matches!(failed.export, ExportState::ExportFailed { ref paths, .. } if paths == &["verified"])
        );
    }
    #[test]
    fn note_action_candidates_do_not_enable_generic_service_jobs() {
        let runtime = runtime();
        let service = runtime.start_service_job(JobKind::Service, |_| Ok(Vec::new()));
        let note = runtime.start_service_job(JobKind::Note, |_| Ok(Vec::new()));
        let service = wait(&runtime, service.id(), |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        let note = wait(&runtime, note.id(), |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        assert!(!service.actions.revoke_remote);
        assert!(note.actions.revoke_remote);
    }
    #[test]
    fn resume_uses_authenticated_checkpoint_direction_and_uploads_cannot_export() {
        let home = std::env::temp_dir().join(format!("filebeam-resume-{}", uuid::Uuid::new_v4()));
        let checkpoint = uuid::Uuid::new_v4().to_string();
        let store = Store::create(&home, &checkpoint).unwrap();
        store
            .save(&serde_json::json!({
                "version": 1, "id": checkpoint, "direction": "upload", "state": "paused",
                "done": 0, "total": 1, "driver": "http", "items": []
            }))
            .unwrap();
        drop(store);
        let runtime = ClientRuntime::new(ApplicationSettings::new(
            TransferSettings {
                state_home: home.clone(),
                max_concurrency: Some(1),
                memory_budget: 128 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            SchedulerLimits {
                workers: 1,
                memory_bytes: 256 * 1024 * 1024,
            },
        ))
        .unwrap();
        let resumed = runtime.resume(checkpoint).unwrap();
        assert_eq!(
            runtime.snapshot(resumed.id()).unwrap().kind,
            JobKind::Upload
        );
        let upload = runtime.start_service_job(JobKind::Upload, |_| {
            Ok(vec!["https://example.test/link".into()])
        });
        let complete = wait(&runtime, upload.id(), |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        assert!(matches!(complete.export, ExportState::NotVerified));
        let _ = std::fs::remove_dir_all(home);
    }
    #[test]
    fn configured_secret_store_restores_detached_checkpoint_without_a_worker() {
        let home = std::env::temp_dir().join(format!("filebeam-restore-{}", uuid::Uuid::new_v4()));
        let checkpoint = uuid::Uuid::new_v4().to_string();
        let secrets: std::sync::Arc<dyn filebeam_transfer_native::checkpoint::SecretStore> =
            std::sync::Arc::new(MockSecretStore);
        let store = Store::create_with_secret_store(&home, &checkpoint, secrets.clone()).unwrap();
        store
            .save(&serde_json::json!({
                "version": 1, "id": checkpoint, "direction": "download", "state": "paused",
                "done": 0, "total": 1, "driver": "http", "items": []
            }))
            .unwrap();
        drop(store);
        let mut settings = runtime().transfer_settings();
        settings.state_home = home.clone();
        settings.checkpoint_secret_store = Some(secrets);
        let runtime = ClientRuntime::new(ApplicationSettings::new(
            settings,
            SchedulerLimits {
                workers: 1,
                memory_bytes: 256 * 1024 * 1024,
            },
        ))
        .unwrap();
        let recovered = runtime.restore_saved().unwrap();
        assert_eq!(recovered.len(), 1);
        let id = recovered[0].id();
        let snapshot = runtime.snapshot(id).unwrap();
        assert_eq!(snapshot.job.state, JobState::Paused);
        assert!(snapshot.actions.resume);
        assert!(!snapshot.actions.pause);
        assert!(runtime.dispatch(id, JobAction::Pause).is_err());
        runtime.dispatch(id, JobAction::Resume).unwrap();
        assert_eq!(
            wait(&runtime, id, |snapshot| snapshot
                .job
                .checkpoint_id
                .is_some())
            .job
            .checkpoint_id
            .as_deref(),
            Some(checkpoint.as_str())
        );
        let _ = std::fs::remove_dir_all(home);
    }
    #[test]
    fn export_retry_reuses_the_complete_verified_path_set_after_a_partial_write() {
        let runtime = runtime();
        let job = runtime.start_service_job(JobKind::Download, |_| {
            Ok(vec!["first-verified".into(), "second-verified".into()])
        });
        let id = job.id();
        wait(&runtime, id, |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        let written = Arc::new(Mutex::new(Vec::new()));
        let partial = written.clone();
        assert!(
            runtime
                .commit_export(id, PathBuf::from("/dest"), move |paths, _| {
                    partial.lock().unwrap().push(paths[0].clone());
                    bail!("destination disconnected")
                })
                .is_err()
        );
        let retry = written.clone();
        runtime
            .commit_export(id, PathBuf::from("/dest"), move |paths, _| {
                assert_eq!(paths, ["first-verified", "second-verified"]);
                retry.lock().unwrap().extend(paths.iter().cloned());
                Ok(())
            })
            .unwrap();
        assert_eq!(
            *written.lock().unwrap(),
            ["first-verified", "first-verified", "second-verified"]
        );
        assert!(matches!(
            runtime.snapshot(id).unwrap().export,
            ExportState::Exported { paths, .. } if paths == ["first-verified", "second-verified"]
        ));
    }
    #[test]
    fn shutdown_waits_for_checkpointed_pause() {
        let runtime = runtime();
        let job = runtime.start_service_job(JobKind::Service, |control| {
            control.secret(SecretKind::ShareKey)?;
            Ok(Vec::new())
        });
        let id = job.id();
        wait(&runtime, id, |s| s.job.prompt.is_some());
        let report = runtime.shutdown(Duration::from_secs(1));
        assert!(report.checkpointed.contains(&id));
        assert!(report.still_stopping.is_empty());
    }
    #[test]
    fn process_runtimes_share_only_the_scheduler_not_profile_settings() {
        let first = ClientRuntime::process_global(ApplicationSettings::new(
            TransferSettings {
                state_home: PathBuf::from("first-profile"),
                max_concurrency: Some(1),
                memory_budget: 64 * 1024 * 1024,
                client_user_agent: Some("first".into()),
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            SchedulerLimits {
                workers: 1,
                memory_bytes: 128 * 1024 * 1024,
            },
        ))
        .unwrap();
        let second = ClientRuntime::process_global(ApplicationSettings::new(
            TransferSettings {
                state_home: PathBuf::from("second-profile"),
                max_concurrency: Some(1),
                memory_budget: 64 * 1024 * 1024,
                client_user_agent: Some("second".into()),
                webrtc_relay_only: true,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            SchedulerLimits {
                workers: 4,
                memory_bytes: 512 * 1024 * 1024,
            },
        ))
        .unwrap();
        assert_eq!(
            first.transfer_settings().state_home,
            PathBuf::from("first-profile")
        );
        assert_eq!(
            second.transfer_settings().state_home,
            PathBuf::from("second-profile")
        );
        assert_eq!(
            second.transfer_settings().client_user_agent.as_deref(),
            Some("second")
        );
        assert!(second.transfer_settings().webrtc_relay_only);
    }
    #[test]
    fn shutdown_does_not_claim_completed_jobs_were_checkpointed() {
        let runtime = runtime();
        let job = runtime.start_service_job(JobKind::Service, |_| Ok(Vec::new()));
        let id = job.id();
        wait(&runtime, id, |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        let report = runtime.shutdown(Duration::ZERO);
        assert!(report.checkpointed.is_empty());
        assert!(report.still_stopping.is_empty());
    }

    #[test]
    fn reload_changes_only_future_controls_and_preserves_running_jobs() {
        let runtime = runtime();
        let (started, wait_for_reload) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let old = runtime.start_service_job(JobKind::Service, move |control| {
            assert!(!control.webrtc_relay_only());
            started.send(()).unwrap();
            released.recv().unwrap();
            assert!(!control.webrtc_relay_only());
            Ok(Vec::new())
        });
        wait_for_reload
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        let old_id = old.id();
        let mut transfer = runtime.transfer_settings();
        transfer.webrtc_relay_only = true;
        runtime
            .reload_settings(ApplicationSettings::new(
                transfer,
                SchedulerLimits {
                    workers: 1,
                    memory_bytes: 256 * 1024 * 1024,
                },
            ))
            .unwrap();
        let new = runtime.start_service_job(JobKind::Service, |control| {
            assert!(control.webrtc_relay_only());
            Ok(Vec::new())
        });
        release.send(()).unwrap();
        wait(&runtime, old_id, |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        wait(&runtime, new.id(), |snapshot| {
            snapshot.job.state == JobState::Complete
        });
        assert!(runtime.observe(old_id).is_some());
        assert_eq!(runtime.snapshots().len(), 2);
    }
}
