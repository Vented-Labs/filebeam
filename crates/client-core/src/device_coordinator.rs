//! Application-owned lifecycle for authenticated registered-device jobs.
//!
//! This layer owns observations independently of any screen or platform
//! callback. It deliberately delegates all transport, admission, checkpoint,
//! and peer proof work to `DeviceRuntime`.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use anyhow::Result;

use crate::{
    ApplicationJobSnapshot, JobAction, JobId, JobObservation, ShutdownReport,
    device_runtime::DeviceRuntime,
};
use filebeam_transfer_native::{
    device::DeviceSigner,
    protocol::peer::{PeerPreparedJob, PeerReceiver},
};

#[derive(Clone)]
pub struct DeviceCoordinator {
    runtime: DeviceRuntime,
    jobs: std::sync::Arc<Mutex<HashMap<JobId, JobObservation>>>,
}

impl DeviceCoordinator {
    pub fn new(runtime: DeviceRuntime) -> Self {
        Self {
            runtime,
            jobs: Default::default(),
        }
    }

    /// Retain an authenticated device job at app scope. Dropping the returned
    /// observer cannot cancel the job; explicit pause or expiry is required.
    pub fn retain(&self, observation: JobObservation) -> JobId {
        let id = observation.id();
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, observation);
        id
    }

    pub fn start_sender(
        &self,
        session: crate::device_registry::AuthenticatedPeerSession,
        job: PeerPreparedJob,
        grant_id: String,
        expires_at: u64,
    ) -> JobId {
        self.retain(
            self.runtime
                .start_peer_sender(session, job, grant_id, None, expires_at),
        )
    }

    pub fn start_receiver(
        &self,
        session: crate::device_registry::AuthenticatedPeerSession,
        receiver: std::sync::Arc<dyn PeerReceiver>,
        signer: std::sync::Arc<dyn DeviceSigner>,
    ) -> JobId {
        self.retain(self.runtime.start_peer_receiver(session, receiver, signer))
    }

    pub fn snapshots(&self) -> Vec<ApplicationJobSnapshot> {
        self.runtime.snapshots()
    }

    pub fn pause(&self, id: JobId) -> Result<()> {
        self.runtime
            .client_runtime()
            .dispatch(id, JobAction::Pause)?;
        Ok(())
    }

    /// Device jobs have no unsafe forced abort path: cancellation is a normal
    /// checkpointing pause, so a later foreground session can reconnect.
    pub fn cancel(&self, id: JobId) -> Result<()> {
        self.pause(id)
    }

    /// Called from finite platform background-expiration callbacks. This asks
    /// workers to checkpoint and returns without retaining a transport socket.
    pub fn expire(&self, timeout: Duration) -> ShutdownReport {
        self.runtime.shutdown(timeout)
    }

    pub fn is_idle(&self) -> bool {
        self.runtime.is_idle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ApplicationSettings, SchedulerLimits, control::TransferSettings};
    use std::{thread, time::Instant};

    fn runtime() -> DeviceRuntime {
        DeviceRuntime::new(ApplicationSettings::new(
            TransferSettings {
                state_home: std::env::temp_dir().join(format!(
                    "filebeam-device-coordinator-{}",
                    uuid::Uuid::new_v4()
                )),
                max_concurrency: Some(1),
                memory_budget: 64 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            SchedulerLimits {
                workers: 1,
                memory_bytes: 128 * 1024 * 1024,
            },
        ))
        .unwrap()
    }

    #[test]
    fn dropped_observer_does_not_cancel_retained_device_job() {
        let runtime = runtime();
        let coordinator = DeviceCoordinator::new(runtime.clone());
        let job = runtime.client_runtime().start_device_job(|_| {
            thread::sleep(Duration::from_millis(30));
            Ok(vec!["complete".into()])
        });
        let id = coordinator.retain(job.clone());
        drop(job);
        let deadline = Instant::now() + Duration::from_secs(1);
        while runtime.client_runtime().snapshot(id).unwrap().job.state != crate::JobState::Complete
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(coordinator.is_idle());
    }
}
