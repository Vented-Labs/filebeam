use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{config::Config, protocol, update, uploads::DirectoryMode};
use filebeam_client_core::services::{NoteCreate, ServiceClient};
#[allow(unused_imports)]
pub use filebeam_transfer_native::control::{
    Cancelled, Control, PeerConsent, PeerFailed, Phase, Progress, Prompt, PromptKind, SecretKind,
    ShareReady, TransferEvent, TransferSettings,
};

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

pub enum Request {
    Upload(Vec<PathBuf>, DirectoryMode, protocol::UploadOptions),
    Download {
        link: String,
        output: PathBuf,
    },
    InboxDownload {
        id: String,
        output: PathBuf,
        key: Vec<u8>,
        cookie: String,
    },
    Revoke {
        id: String,
    },
    EndLive {
        id: String,
    },
    NoteLive(NoteCreate),
    Update,
    Resume {
        id: String,
        direction: Direction,
    },
}

impl Request {
    pub fn direction(&self) -> Direction {
        match self {
            Self::Upload(..) => Direction::Upload,
            Self::Download { .. } => Direction::Download,
            Self::InboxDownload { .. } => Direction::Download,
            Self::Revoke { .. } | Self::EndLive { .. } => Direction::Update,
            Self::NoteLive(..) => Direction::Upload,
            Self::Update => Direction::Update,
            Self::Resume { direction, .. } => *direction,
        }
    }
}

pub struct Job(filebeam_client_core::Job);

impl std::ops::Deref for Job {
    type Target = filebeam_client_core::Job;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Job {
    pub fn start(instance: String, config: Config, request: Request) -> Self {
        Self(filebeam_client_core::Job::spawn(
            TransferSettings {
                state_home: config.home.join("transfers"),
                max_concurrency: config.max_concurrency,
                memory_budget: config.memory_limit_mib * 1024 * 1024,
                client_user_agent: Some(format!("beam/{}", env!("BEAM_VERSION"))),
                webrtc_relay_only: config.webrtc_relay_only,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            move |worker| match request {
                Request::Upload(paths, mode, options) => {
                    protocol::upload(&instance, &paths, mode, options, worker)
                        .map(|link| vec![link])
                }
                Request::Download { link, output } => {
                    protocol::download(&instance, &link, &output, worker).map(|paths| {
                        paths
                            .into_iter()
                            .map(|path| path.display().to_string())
                            .collect()
                    })
                }
                Request::InboxDownload {
                    id,
                    output,
                    key,
                    cookie,
                } => protocol::download_inbox(&instance, &id, &key, &cookie, &output, worker).map(
                    |paths| {
                        paths
                            .into_iter()
                            .map(|path| path.display().to_string())
                            .collect()
                    },
                ),
                Request::Revoke { id } => { let notes = filebeam_client_core::services::note_management::NoteManagementStore::for_root(worker.transfer_home()); if notes.contains(&id) { notes.action(&id, filebeam_client_core::services::note_management::NoteManagementAction::Revoke) } else { protocol::revoke_upload(&id, worker) }.map(|_| vec![id]) },
                Request::EndLive { id } => { let notes = filebeam_client_core::services::note_management::NoteManagementStore::for_root(worker.transfer_home()); if notes.contains(&id) { notes.action(&id, filebeam_client_core::services::note_management::NoteManagementAction::EndLive) } else { protocol::end_live(&id, worker) }.map(|_| vec![id]) },
                Request::NoteLive(request) => ServiceClient::new(&instance)?
                    .notes()
                    .create_live(request, worker, worker.cancelled.clone(), Some(filebeam_client_core::services::note_management::NoteManagementStore::for_root(worker.transfer_home())))
                    .map(|note| vec![note.link]),
                Request::Update => worker
                    .phase(Phase::Updating)
                    .and_then(|_| update::check(&config))
                    .map(|value| vec![value]),
                Request::Resume { id, .. } => protocol::resume(&id, worker),
            },
        ))
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
    generation: u64,
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
            generation: 0,
        }
    }

    pub fn tick(&mut self, progress: Progress, reduced_motion: bool, now: Instant) {
        if self.finished {
            return;
        }
        let generation_changed = progress.generation != self.generation;
        if generation_changed {
            self.generation = progress.generation;
            self.sampled_bytes = progress.wire_bytes;
            self.last_sample = now;
            self.rate = 0.0;
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
        // A checkpoint retry may legitimately restart lower. Within one phase
        // generation, a lower snapshot is stale/uncommitted display progress.
        let target = if generation_changed {
            target
        } else {
            target.max(self.ratio)
        };
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
        Control::test_factory()
    }

    #[test]
    fn control_keeps_transport_and_committed_bytes_separate() {
        let control = control();
        control.totals(10, 1);
        control.item("file".into(), 1, 10);
        control.advance(5, 5, 13);
        assert_eq!(control.snapshot().done, 5);
        control.advance(10, 10, 13);
        let snapshot = control.snapshot();
        assert_eq!(
            (snapshot.done, snapshot.wire_bytes, snapshot.committed),
            (10, 26, 0)
        );
        control.commit(10);
        assert_eq!(control.snapshot().committed, 10);
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

    #[test]
    fn control_exposes_configured_transfer_resources() {
        let control = control();
        assert_eq!(control.transfer_home(), std::path::Path::new("transfers"));
        assert_eq!(control.max_concurrency(), None);
        assert_eq!(control.memory_budget(), 512 * 1024 * 1024);
    }

    #[test]
    fn resumed_jobs_keep_the_saved_direction() {
        assert_eq!(
            Request::Resume {
                id: "job".into(),
                direction: Direction::Download,
            }
            .direction(),
            Direction::Download
        );
    }

    #[test]
    fn recovery_phases_have_safe_terminal_labels() {
        assert_eq!(Phase::Retrying.label(), "Retrying transfer");
        assert_eq!(Phase::Reconnecting.label(), "Reconnecting");
        assert_eq!(Phase::Storing.label(), "Saving encrypted transfer state");
    }

    #[test]
    fn progress_is_monotonic_within_a_phase_and_resets_on_a_new_generation() {
        let mut view = TransferView::new(Direction::Upload);
        let start = view.started;
        let progress = |done, generation| Progress {
            phase: Phase::Sending,
            total: Some(100),
            done,
            generation,
            ..Progress::default()
        };
        view.tick(progress(80, 1), true, start + Duration::from_secs(1));
        view.tick(progress(20, 1), true, start + Duration::from_secs(2));
        assert_eq!(view.ratio, 0.8);
        view.tick(progress(20, 2), true, start + Duration::from_secs(3));
        assert_eq!(view.ratio, 0.2);
    }
}
