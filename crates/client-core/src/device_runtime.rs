//! Daemon-facing ownership boundary for registered-device work.
//!
//! The daemon supplies platform policy, grants, and registry refreshes, while
//! this owner keeps the work on the same scheduler and checkpoint custody as
//! every other native transfer job.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;

use crate::{
    ApplicationJobSnapshot, ApplicationSettings, ClientRuntime, JobObservation, JobState,
    ShutdownReport, device_registry::AuthenticatedPeerSession,
};
use filebeam_transfer_native::protocol::peer::{PeerPreparedJob, PeerReceiver, load_peer_job};

#[derive(Clone)]
pub struct DeviceRuntime {
    runtime: ClientRuntime,
}

impl DeviceRuntime {
    pub fn new(settings: ApplicationSettings) -> Result<Self> {
        Ok(Self {
            runtime: ClientRuntime::process_global(settings)?,
        })
    }

    /// Delivers through a registry-created authenticated session. Callers cannot
    /// supply proof authority or a disconnected transport identity.
    pub fn start_peer_sender(
        &self,
        session: AuthenticatedPeerSession,
        job: PeerPreparedJob,
        grant_id: String,
        subdirectory: Option<String>,
        expires_at_unix_seconds: u64,
    ) -> JobObservation {
        self.runtime.start_device_job(move |control| {
            let store = control.open_checkpoint_store(job.checkpoint_id())?;
            let job = load_peer_job(&store)?;
            let receipt =
                filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(async {
                    session
                        .send_prepared_peer_job(
                            &store,
                            &job,
                            grant_id,
                            subdirectory,
                            expires_at_unix_seconds,
                            control,
                        )
                        .await
                })?;
            Ok(vec![receipt.commit_id])
        })
    }

    /// Receives one registry-authenticated peer offer. The receiver is wrapped
    /// so registry denial is mandatory at admission and immediately precommit.
    pub fn start_peer_receiver(
        &self,
        session: AuthenticatedPeerSession,
        receiver: Arc<dyn PeerReceiver>,
        receiver_signer: Arc<dyn filebeam_transfer_native::device::DeviceSigner>,
    ) -> JobObservation {
        self.runtime.start_device_job(move |control| {
            let offer = filebeam_transfer_native::runtime::shared_tokio_runtime()
                .block_on(session.receive_peer_offer(unix_now()?))?;
            let checkpoint_id = session.receiver_store_id_for_offer(&offer);
            let store = control
                .open_checkpoint_store(&checkpoint_id)
                .or_else(|_| control.create_checkpoint_store(&checkpoint_id))?;
            let receipt =
                filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(async {
                    session
                        .receive_admitted_peer_offer(
                            &store,
                            &offer,
                            receiver.as_ref(),
                            receiver_signer.as_ref(),
                            control,
                        )
                        .await
                })?;
            Ok(vec![receipt.commit_id])
        })
    }

    pub fn snapshots(&self) -> Vec<ApplicationJobSnapshot> {
        self.runtime.snapshots()
    }

    pub fn is_idle(&self) -> bool {
        self.snapshots().iter().all(|snapshot| {
            matches!(
                snapshot.job.state,
                JobState::Paused | JobState::Complete | JobState::Failed
            )
        })
    }

    pub fn shutdown(&self, timeout: Duration) -> ShutdownReport {
        self.runtime.shutdown(timeout)
    }

    pub fn client_runtime(&self) -> &ClientRuntime {
        &self.runtime
    }
}

fn unix_now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| anyhow::anyhow!("system clock predates Unix epoch"))
}
