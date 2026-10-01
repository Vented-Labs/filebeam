//! Platform-independent ownership of native transfer jobs.
//!
//! A UI observes a job; an application/service owns it. Dropping a UI subscription
//! never cancels a transfer. Dropping the owning job requests a checkpointed stop.

pub mod application;
mod error;
pub mod link_presentation;
mod managed;
mod scheduler;
pub mod services;

pub use application::{
    ActionCapabilities, ApplicationJobSnapshot, ApplicationSettings, ClientRuntime, ExportState,
    JobAction, JobId, JobKind, JobObservation, PromptIdentity, PromptResponse, ShutdownReport,
};
pub use error::JobError;
pub use filebeam_transfer_native::source;
pub use filebeam_transfer_native::webrtc::loopback_self_test as webrtc_self_test;
pub use filebeam_transfer_native::{control, protocol, uploads};
pub use managed::{
    DirectoryPrompt, JobErrorKind, JobSnapshot, JobState, ManagedJob, PendingPrompt, PromptType,
    SecretRetryKind, phase_name,
};
pub use scheduler::{
    RUNTIME_ALLOWANCE_BYTES, Scheduler, SchedulerLimits, TRANSIENT_MEMORY_ALLOWANCE_BYTES,
};

use anyhow::Result;
use control::{Cancelled, Control, Prompt, TransferEvent, TransferSettings};
use std::{
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::Ordering,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
};

pub enum Request {
    Upload {
        instance: String,
        paths: Vec<PathBuf>,
        mode: uploads::DirectoryMode,
        options: protocol::UploadOptions,
    },
    UploadSources {
        instance: String,
        sources: Vec<source::UploadSource>,
        mode: uploads::DirectoryMode,
        options: protocol::UploadOptions,
    },
    Download {
        instance: String,
        link: String,
        output: PathBuf,
    },
    Resume {
        id: String,
    },
}

impl Request {
    pub fn run(self, control: &Control) -> Result<Vec<String>> {
        match self {
            Self::Upload {
                instance,
                paths,
                mode,
                options,
            } => protocol::upload(&instance, &paths, mode, options, control).map(|link| vec![link]),
            Self::UploadSources {
                instance,
                sources,
                mode,
                options,
            } => protocol::upload_sources(&instance, &sources, mode, options, control)
                .map(|link| vec![link]),
            Self::Download {
                instance,
                link,
                output,
            } => protocol::download(&instance, &link, &output, control).map(|paths| {
                paths
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect()
            }),
            Self::Resume { id } => protocol::resume(&id, control),
        }
    }

    fn resume_id(&self) -> Option<&str> {
        match self {
            Self::Resume { id } => Some(id),
            _ => None,
        }
    }
}

/// The CLI uses the channels directly; graphical clients use [`ManagedJob`].
pub struct Job {
    pub control: Control,
    pub prompts: Receiver<Prompt>,
    pub events: Receiver<TransferEvent>,
    result: Receiver<Result<Vec<String>>>,
}

impl Job {
    pub fn start(settings: TransferSettings, request: Request) -> Self {
        Self::start_in(default_scheduler(), settings, request)
    }

    pub fn start_in(scheduler: &Scheduler, settings: TransferSettings, request: Request) -> Self {
        Self::start_in_with_ice_override(scheduler, settings, request, None)
    }

    pub fn start_in_with_ice_override(
        scheduler: &Scheduler,
        settings: TransferSettings,
        request: Request,
        ice_override: Option<filebeam_transfer_native::ice::IceOverride>,
    ) -> Self {
        let resume_id = request.resume_id().map(str::to_owned);
        Self::spawn_in_with_ice_override(
            scheduler,
            settings,
            resume_id,
            ice_override,
            move |control| request.run(control),
        )
    }

    /// Shares worker ownership with platform-specific operations such as the CLI updater.
    pub fn spawn(
        settings: TransferSettings,
        run: impl FnOnce(&Control) -> Result<Vec<String>> + Send + 'static,
    ) -> Self {
        Self::spawn_in(default_scheduler(), settings, None, run)
    }

    pub fn spawn_in(
        scheduler: &Scheduler,
        settings: TransferSettings,
        resume_id: Option<String>,
        run: impl FnOnce(&Control) -> Result<Vec<String>> + Send + 'static,
    ) -> Self {
        Self::spawn_in_with_ice_override(scheduler, settings, resume_id, None, run)
    }

    pub fn spawn_in_with_ice_override(
        scheduler: &Scheduler,
        settings: TransferSettings,
        resume_id: Option<String>,
        ice_override: Option<filebeam_transfer_native::ice::IceOverride>,
        run: impl FnOnce(&Control) -> Result<Vec<String>> + Send + 'static,
    ) -> Self {
        let (prompts, receiver) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        let (sender, result) = mpsc::channel();
        let control = Control::with_events(settings, prompts, event_sender);
        if let Some(policy) = ice_override {
            control.set_ice_override(policy);
        }
        let worker = control.clone();
        let scheduler = scheduler.clone();
        thread::spawn(move || {
            let outcome = match scheduler.admit(&worker, resume_id.as_deref()) {
                Ok(admission) => {
                    let outcome = run(&worker);
                    // A paused snapshot is published only after its scheduler
                    // permit is released, so an immediate resume can be admitted.
                    drop(admission);
                    outcome
                }
                Err(error) => Err(error),
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
            events,
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

fn default_scheduler() -> &'static Scheduler {
    static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();
    SCHEDULER.get_or_init(|| {
        Scheduler::new(SchedulerLimits {
            workers: 4,
            memory_bytes: 4 * 1024 * 1024 * 1024
                + RUNTIME_ALLOWANCE_BYTES
                + TRANSIENT_MEMORY_ALLOWANCE_BYTES,
        })
        .expect("default scheduler limits are valid")
    })
}

impl Drop for Job {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
