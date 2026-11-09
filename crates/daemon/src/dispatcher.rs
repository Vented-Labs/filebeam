use crate::{
    DeviceJobState, DeviceJobs, DeviceProvision, daemon_store, registry_scope, registry_store,
};
use anyhow::bail;
use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
#[cfg(windows)]
use filebeam_client_core::JobAction;
use filebeam_client_core::JobObservation;
use filebeam_client_core::device_registry::{DeviceId, DeviceRegistry, RegistryRoute};
use filebeam_transfer_native::protocol::peer::PeerParties;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const REGISTRY_FRESHNESS: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(300);
const MAX_INBOUND_RETRY_DELAY: Duration = Duration::from_secs(60);
/// Longer than the coordinator's rendezvous lifetime, so an inbox entry that
/// outlives a failed `end` request is never answered twice.
const INBOUND_SEEN_RETENTION: Duration = Duration::from_secs(15 * 60);

type ActiveJobs = Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>;

#[derive(Clone)]
pub(crate) enum ActiveJob {
    Preparing(filebeam_transfer_native::control::Control),
    Transferring(JobObservation),
}

#[cfg(windows)]
pub(crate) fn runtime(home: &Path) -> Result<filebeam_client_core::DeviceRuntime> {
    let state_home = home.join("transfers");
    let settings = settings(state_home.clone());
    filebeam_client_core::DeviceRuntime::new(filebeam_client_core::ApplicationSettings::new(
        settings,
        filebeam_client_core::SchedulerLimits {
            workers: 2,
            memory_bytes: 1024 * 1024 * 1024,
        },
    ))
}

/// Owns every background dispatcher thread for one daemon profile.
pub(crate) struct Dispatcher {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Dispatcher {
    pub(crate) fn shutdown(&mut self, timeout: Duration) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        let deadline = std::time::Instant::now() + timeout;
        while self.threads.iter().any(|thread| !thread.is_finished()) {
            if std::time::Instant::now() >= deadline {
                bail!("dispatcher did not stop before the shutdown deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        Ok(())
    }
}

/// Per-target retry schedule. An unreachable or unpaired device is skipped
/// until its delay elapses, so it never holds up jobs for other devices.
#[derive(Default)]
struct Backoff(HashMap<String, (u32, Instant)>);

impl Backoff {
    fn ready(&self, target: &str, now: Instant) -> bool {
        self.0
            .get(target)
            .is_none_or(|(_, retry_at)| now >= *retry_at)
    }

    fn failed(&mut self, target: &str, now: Instant) {
        let entry = self.0.entry(target.into()).or_insert((0, now));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = now + retry_delay(entry.0);
    }

    fn succeeded(&mut self, target: &str) {
        self.0.remove(target);
    }
}

fn retry_delay(failures: u32) -> Duration {
    Duration::from_secs(5u64 << failures.min(6)).min(MAX_RETRY_DELAY)
}

fn log_error(context: &str, error: &anyhow::Error) {
    eprintln!("filebeamd: {context}: {error:#}");
}

/// Starts the outbound worker and its independent registry refresh schedule.
pub(crate) fn start(
    home: PathBuf,
    runtime: filebeam_client_core::DeviceRuntime,
    jobs: Arc<Mutex<DeviceJobs>>,
    active: ActiveJobs,
    certificate: Option<reqwest::Certificate>,
    receive_enabled: Arc<AtomicBool>,
    #[cfg(unix)] lan_enabled: Option<Arc<AtomicBool>>,
) -> Dispatcher {
    let stop = Arc::new(AtomicBool::new(false));
    let outbound_stop = stop.clone();
    let refresh_home = home.clone();
    let refresh_certificate = certificate.clone();
    let inbound_home = home.clone();
    let inbound_runtime = runtime.clone();
    let inbound_certificate = certificate.clone();
    let outbound = std::thread::spawn(move || {
        let mut backoff = Backoff::default();
        while !outbound_stop.load(Ordering::Relaxed) {
            let outbound = Outbound {
                home: &home,
                runtime: &runtime,
                jobs: &jobs,
                active: &active,
                certificate: certificate.clone(),
                stop: &outbound_stop,
                #[cfg(unix)]
                lan_enabled: lan_enabled.as_ref(),
            };
            if let Err(error) = outbound.once(&mut backoff) {
                log_error("outbound device jobs", &error);
            }
            for _ in 0..20 {
                if outbound_stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    let refresh_stop = stop.clone();
    let refresh = std::thread::spawn(move || {
        while !refresh_stop.load(Ordering::Relaxed) {
            if let Ok(Some(provision)) = daemon_store(&refresh_home)
                .and_then(|store| store.load_named::<DeviceProvision>("device-provision-v1.json"))
                && let Err(error) =
                    refresh_registry(&refresh_home, &provision, refresh_certificate.clone())
            {
                log_error("device registry refresh", &error);
            }
            for _ in 0..300 {
                if refresh_stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    let inbound_stop = stop.clone();
    let inbound = std::thread::spawn(move || {
        let seen = Arc::new(Mutex::new(HashMap::new()));
        let mut failures = 0u32;
        while !inbound_stop.load(Ordering::Acquire) {
            match incoming_once(
                &inbound_home,
                &inbound_runtime,
                &receive_enabled,
                inbound_certificate.clone(),
                &seen,
            ) {
                Ok(()) => failures = 0,
                Err(error) => {
                    failures = failures.saturating_add(1);
                    log_error("inbound device connections", &error);
                }
            }
            // Poll often while healthy so senders are answered promptly, and
            // back off while the coordinator is unreachable or denies us.
            let pause = if failures == 0 {
                Duration::from_secs(2)
            } else {
                retry_delay(failures).min(MAX_INBOUND_RETRY_DELAY)
            };
            let resume = Instant::now() + pause;
            while Instant::now() < resume && !inbound_stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    Dispatcher {
        stop,
        threads: vec![outbound, refresh, inbound],
    }
}

fn incoming_once(
    home: &Path,
    runtime: &filebeam_client_core::DeviceRuntime,
    receive_enabled: &Arc<AtomicBool>,
    certificate: Option<reqwest::Certificate>,
    seen: &Arc<Mutex<HashMap<String, Instant>>>,
) -> Result<()> {
    if !receive_enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    let Some(provision) =
        daemon_store(home)?.load_named::<DeviceProvision>("device-provision-v1.json")?
    else {
        return Ok(());
    };
    let signaling = filebeam_transfer_native::webrtc::DeviceSignaling::new(
        signaling_client(certificate)?,
        &provision.instance,
        &provision.credential,
    )?;
    let incoming =
        filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(signaling.incoming())?;
    for connection in incoming.connections {
        let admitted = {
            let mut seen = seen
                .lock()
                .map_err(|_| anyhow::anyhow!("inbound connection state is poisoned"))?;
            let now = Instant::now();
            seen.retain(|_, at| now.duration_since(*at) < INBOUND_SEEN_RETENTION);
            seen.insert(connection.id.clone(), now).is_none()
        };
        if !admitted || !receive_enabled.load(Ordering::Acquire) {
            continue;
        }
        let inbound = Inbound {
            home: home.into(),
            runtime: runtime.clone(),
            receive_enabled: receive_enabled.clone(),
            scope: registry_scope(&provision)?,
            signer: signing_key(&provision)?,
            signaling: signaling.clone(),
        };
        // Each handshake waits on the remote peer, so one slow or failing
        // connection must not delay the others.
        std::thread::spawn(move || {
            let id = connection.id.clone();
            if let Err(error) = inbound.accept(connection) {
                log_error(&format!("inbound device connection {id}"), &error);
            }
            // SDP exchange is complete or abandoned; retire the rendezvous so
            // the coordinator stops offering it.
            let _ = filebeam_transfer_native::runtime::shared_tokio_runtime()
                .block_on(inbound.signaling.end(&id));
        });
    }
    Ok(())
}

struct Inbound {
    home: PathBuf,
    runtime: filebeam_client_core::DeviceRuntime,
    receive_enabled: Arc<AtomicBool>,
    scope: filebeam_client_core::device_registry::RegistryScope,
    signer: SigningKey,
    signaling: filebeam_transfer_native::webrtc::DeviceSignaling,
}

impl Inbound {
    fn accept(&self, connection: filebeam_transfer_native::webrtc::DeviceConnection) -> Result<()> {
        let peer = DeviceId::parse(connection.source_device_id)?;
        let session = {
            let store = registry_store(&self.home)?;
            let registry = DeviceRegistry::open(&store, self.scope.clone(), REGISTRY_FRESHNESS)?;
            registry.peer_session(&self.scope, &peer, RegistryRoute::Instance, now())?
        };
        let tokio = filebeam_transfer_native::runtime::shared_tokio_runtime();
        let servers = tokio.block_on(self.signaling.ice_servers(&connection.id))?;
        let (native, _, channels) =
            tokio.block_on(filebeam_transfer_native::webrtc::connect_device_responder(
                &self.signaling,
                &connection.id,
                &servers,
                false,
            ))?;
        let authenticated = tokio.block_on(session.authenticate(
            native,
            channels,
            Arc::new(self.signer.clone()),
            &connection.id,
        ))?;
        self.runtime.start_peer_receiver(
            authenticated,
            Arc::new(crate::DaemonPeerReceiver::with_policy(
                self.home.clone(),
                self.receive_enabled.clone(),
            )),
            Arc::new(self.signer.clone()),
        );
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn control(
    runtime: &filebeam_client_core::DeviceRuntime,
    active: &ActiveJobs,
    id: uuid::Uuid,
    action: &DeviceJobState,
) -> Result<()> {
    match (
        active
            .lock()
            .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?
            .get(&id)
            .cloned(),
        action,
    ) {
        (
            Some(ActiveJob::Preparing(control)),
            DeviceJobState::Paused | DeviceJobState::Cancelled,
        ) => control.cancel(),
        (
            Some(ActiveJob::Transferring(observation)),
            DeviceJobState::Paused | DeviceJobState::Cancelled,
        ) => {
            let _ = runtime
                .client_runtime()
                .dispatch(observation.id(), JobAction::Pause)?;
        }
        (Some(ActiveJob::Transferring(_)), DeviceJobState::AwaitingPeerConfiguration) => {
            bail!("active device jobs must pause before resuming")
        }
        _ => {}
    };
    Ok(())
}

struct Outbound<'a> {
    home: &'a Path,
    runtime: &'a filebeam_client_core::DeviceRuntime,
    jobs: &'a Arc<Mutex<DeviceJobs>>,
    active: &'a ActiveJobs,
    certificate: Option<reqwest::Certificate>,
    stop: &'a AtomicBool,
    #[cfg(unix)]
    lan_enabled: Option<&'a Arc<AtomicBool>>,
}

impl Outbound<'_> {
    fn once(&self, backoff: &mut Backoff) -> Result<()> {
        let Some(provision) =
            daemon_store(self.home)?.load_named::<DeviceProvision>("device-provision-v1.json")?
        else {
            return Ok(());
        };
        let pending = {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?;
            jobs.expire(u64::try_from(now()).unwrap_or_default())?;
            jobs.pending()
        };
        for job in pending {
            if self.stop.load(Ordering::Acquire) {
                break;
            }
            if !backoff.ready(&job.target, Instant::now()) {
                continue;
            }
            match self.attempt(&provision, &job) {
                Ok(()) => backoff.succeeded(&job.target),
                Err(error) => {
                    backoff.failed(&job.target, Instant::now());
                    log_error(&format!("device job {}", job.id), &error);
                    self.jobs
                        .lock()
                        .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                        .note_retry(job.id, &format!("{error:#}"))?;
                }
            }
        }
        Ok(())
    }

    fn registry<T>(
        &self,
        scope: &filebeam_client_core::device_registry::RegistryScope,
        read: impl FnOnce(&DeviceRegistry<'_>) -> Result<T>,
    ) -> Result<T> {
        let store = registry_store(self.home)?;
        let registry = DeviceRegistry::open(&store, scope.clone(), REGISTRY_FRESHNESS)?;
        read(&registry)
    }

    fn attempt(&self, provision: &DeviceProvision, job: &crate::DeviceJob) -> Result<()> {
        let scope = registry_scope(provision)?;
        let key = signing_key(provision)?;
        let peer = DeviceId::parse(job.target.clone())?;
        if !job.prepared {
            let parties = self.registry(&scope, |registry| {
                let target =
                    registry.pinned_target(&scope, &peer, RegistryRoute::Instance, now())?;
                let local = registry.local_device()?;
                Ok(PeerParties {
                    sender_device_id: local.id().to_owned(),
                    sender_key_version: local.key_version(),
                    receiver_device_id: target.id.as_str().to_owned(),
                    receiver_key_version: target.key_version,
                })
            })?;
            return prepare(self.home, self.jobs, self.active, job, parties);
        }
        #[cfg(unix)]
        if self
            .lan_enabled
            .is_some_and(|enabled| enabled.load(Ordering::Relaxed))
        {
            let observation = self.registry(&scope, |registry| {
                crate::lan_runtime::try_send(
                    self.runtime,
                    registry,
                    &scope,
                    &peer,
                    &key,
                    self.home,
                    job,
                )
            })?;
            if let Some(observation) = observation {
                return start_transfer(self.jobs, self.active, job.id, observation);
            }
        }
        // The session owns the native canonical registry cache, so release the
        // encrypted Store before any potentially slow network operation.
        let session = self.registry(&scope, |registry| {
            registry.peer_session(&scope, &peer, RegistryRoute::Instance, now())
        })?;
        if self.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let signaling = filebeam_transfer_native::webrtc::DeviceSignaling::new(
            signaling_client(self.certificate.clone())?,
            &provision.instance,
            &provision.credential,
        )?;
        let tokio = filebeam_transfer_native::runtime::shared_tokio_runtime();
        let (native, connection, channels) = tokio
            .block_on(filebeam_transfer_native::webrtc::connect_device_initiator(
                &signaling,
                peer.as_str(),
                &[],
                false,
            ))
            .context("the receiving device did not answer")?;
        if self.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let authenticated = tokio.block_on(session.authenticate(
            native,
            channels,
            Arc::new(key.clone()),
            &connection.id,
        ))?;
        let store = filebeam_transfer_native::checkpoint::Store::open(
            &self.home.join("transfers"),
            &job.id.to_string(),
        )?;
        let prepared = filebeam_transfer_native::protocol::peer::load_peer_job(&store)?;
        drop(store);
        let observation = self.runtime.start_peer_sender(
            authenticated,
            prepared,
            job.grant_id.clone(),
            None,
            job.expires_at_unix_seconds,
        );
        start_transfer(self.jobs, self.active, job.id, observation)
    }
}

fn prepare(
    home: &Path,
    jobs: &Arc<Mutex<DeviceJobs>>,
    active: &ActiveJobs,
    job: &crate::DeviceJob,
    parties: PeerParties,
) -> Result<()> {
    jobs.lock()
        .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
        .set_state(job.id, DeviceJobState::Preparing, false)?;
    let store = filebeam_transfer_native::checkpoint::Store::create(
        &home.join("transfers"),
        &job.id.to_string(),
    )?;
    let control = filebeam_transfer_native::control::Control::new(
        settings(home.join("transfers")),
        std::sync::mpsc::channel().0,
    );
    active
        .lock()
        .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?
        .insert(job.id, ActiveJob::Preparing(control.clone()));
    let sources = job.sources.clone();
    let id = job.id;
    let jobs = jobs.clone();
    let active = active.clone();
    std::thread::spawn(move || {
        let result = filebeam_transfer_native::protocol::peer::prepare_peer_tree_job(
            &store,
            &sources,
            Default::default(),
            parties,
            filebeam_transfer_native::webrtc::FRAME_PAYLOAD_BYTES as u64,
            control,
        );
        if let Err(error) = &result {
            log_error(&format!("device job {id} preparation"), error);
        }
        let _ = jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
            .and_then(|mut jobs| {
                if user_stopped(&jobs, id) {
                    return Ok(());
                }
                match &result {
                    Ok(_) => jobs.set_state(id, DeviceJobState::AwaitingPeerConfiguration, true),
                    Err(error) => jobs.fail(id, &format!("preparation failed: {error:#}")),
                }
            });
        let _ = active.lock().map(|mut active| active.remove(&id));
    });
    Ok(())
}

fn user_stopped(jobs: &DeviceJobs, id: uuid::Uuid) -> bool {
    jobs.list().iter().any(|job| {
        job.id == id
            && matches!(
                job.state,
                DeviceJobState::Paused | DeviceJobState::Cancelled
            )
    })
}

fn start_transfer(
    jobs: &Arc<Mutex<DeviceJobs>>,
    active: &ActiveJobs,
    id: uuid::Uuid,
    observation: JobObservation,
) -> Result<()> {
    active
        .lock()
        .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?
        .insert(id, ActiveJob::Transferring(observation.clone()));
    jobs.lock()
        .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
        .set_state(id, DeviceJobState::Transferring, true)?;
    monitor(jobs.clone(), active.clone(), id, observation);
    Ok(())
}

fn monitor(
    jobs: Arc<Mutex<DeviceJobs>>,
    active: ActiveJobs,
    id: uuid::Uuid,
    observation: JobObservation,
) {
    std::thread::spawn(move || {
        loop {
            let Some(snapshot) = observation.snapshot() else {
                return;
            };
            let state = match snapshot.job.state {
                filebeam_client_core::JobState::Complete => Some(DeviceJobState::Delivered),
                filebeam_client_core::JobState::Paused => Some(DeviceJobState::Paused),
                filebeam_client_core::JobState::Failed => Some(DeviceJobState::Failed),
                _ => None,
            };
            if let Some(state) = state {
                let _ = jobs
                    .lock()
                    .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
                    .and_then(|mut jobs| {
                        if user_stopped(&jobs, id) {
                            Ok(())
                        } else if state == DeviceJobState::Failed {
                            let reason = snapshot
                                .job
                                .error
                                .clone()
                                .unwrap_or_else(|| "the transfer failed".into());
                            eprintln!("filebeamd: device job {id}: {reason}");
                            jobs.fail(id, &reason)
                        } else {
                            jobs.set_state(id, state, true)
                        }
                    });
                let _ = active.lock().map(|mut active| active.remove(&id));
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

fn settings(state_home: PathBuf) -> filebeam_client_core::control::TransferSettings {
    filebeam_client_core::control::TransferSettings {
        state_home: state_home.clone(),
        max_concurrency: Some(2),
        memory_budget: 512 * 1024 * 1024,
        client_user_agent: Some("filebeamd".into()),
        webrtc_relay_only: false,
        checkpoint_secret_store: Some(
            filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(state_home),
        ),
        source_resolver: None,
    }
}

fn signing_key(provision: &DeviceProvision) -> Result<SigningKey> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    Ok(SigningKey::from_bytes(
        &STANDARD
            .decode(&provision.signing_key)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("device signing key has an invalid length"))?,
    ))
}

fn signaling_client(certificate: Option<reqwest::Certificate>) -> Result<reqwest::Client> {
    let mut client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10));
    if let Some(certificate) = certificate {
        client = client.add_root_certificate(certificate);
    }
    client.build().context("create device signaling client")
}

fn now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .unwrap_or(i64::MAX)
}

fn refresh_registry(
    home: &Path,
    provision: &DeviceProvision,
    certificate: Option<reqwest::Certificate>,
) -> Result<()> {
    crate::ControlPlane::new(home, certificate).refresh_registry(provision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_per_target_and_bounded() {
        let mut backoff = Backoff::default();
        let start = Instant::now();
        backoff.failed("offline", start);
        assert!(!backoff.ready("offline", start));
        assert!(backoff.ready("online", start));
        assert!(backoff.ready("offline", start + retry_delay(1)));
        for _ in 0..20 {
            backoff.failed("offline", start);
        }
        assert!(backoff.ready("offline", start + MAX_RETRY_DELAY));
        backoff.succeeded("offline");
        assert!(backoff.ready("offline", start));
    }
}
