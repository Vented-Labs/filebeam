use crate::{
    DeviceJobState, DeviceJobs, DeviceProvision, daemon_store, registry_scope, registry_store,
};
use anyhow::bail;
use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
#[cfg(windows)]
use filebeam_client_core::JobAction;
use filebeam_client_core::JobObservation;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

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

/// Starts the outbound worker and its independent registry refresh schedule.
pub(crate) fn start(
    home: PathBuf,
    runtime: filebeam_client_core::DeviceRuntime,
    jobs: Arc<Mutex<DeviceJobs>>,
    active: Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
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
        while !outbound_stop.load(Ordering::Relaxed) {
            let _ = once(
                &home,
                &runtime,
                &jobs,
                &active,
                certificate.clone(),
                &outbound_stop,
                #[cfg(unix)]
                lan_enabled.as_ref(),
            );
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
            {
                let _ = refresh_registry(&refresh_home, &provision, refresh_certificate.clone());
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
        while !inbound_stop.load(Ordering::Acquire) {
            let _ = incoming_once(
                &inbound_home,
                &inbound_runtime,
                &receive_enabled,
                inbound_certificate.clone(),
                &seen,
            );
            for _ in 0..20 {
                if inbound_stop.load(Ordering::Acquire) {
                    break;
                }
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
    seen: &Arc<Mutex<HashMap<String, std::time::Instant>>>,
) -> Result<()> {
    if !receive_enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    let Some(provision) =
        daemon_store(home)?.load_named::<DeviceProvision>("device-provision-v1.json")?
    else {
        return Ok(());
    };
    let scope = registry_scope(&provision)?;
    let signer = signing_key(&provision)?;
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
            let now = std::time::Instant::now();
            seen.retain(|_, at| now.duration_since(*at) < Duration::from_secs(120));
            seen.insert(connection.id.clone(), now).is_none()
        };
        if !admitted || !receive_enabled.load(Ordering::Acquire) {
            continue;
        }
        let peer =
            filebeam_client_core::device_registry::DeviceId::parse(connection.source_device_id)?;
        let session = {
            let store = registry_store(home)?;
            let registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                &store,
                scope.clone(),
                Duration::from_secs(7 * 24 * 60 * 60),
            )?;
            registry.peer_session(
                &scope,
                &peer,
                filebeam_client_core::device_registry::RegistryRoute::Instance,
                now(),
            )?
        };
        let servers = filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(signaling.ice_servers(&connection.id))?;
        let (native, _, channels) = filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(filebeam_transfer_native::webrtc::connect_device_responder(
                &signaling,
                &connection.id,
                &servers,
                false,
            ))?;
        let authenticated = filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
            session.authenticate(native, channels, Arc::new(signer.clone()), &connection.id),
        )?;
        runtime.start_peer_receiver(
            authenticated,
            Arc::new(crate::DaemonPeerReceiver::with_policy(
                home.into(),
                receive_enabled.clone(),
            )),
            Arc::new(signer.clone()),
        );
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn control(
    runtime: &filebeam_client_core::DeviceRuntime,
    active: &Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
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

fn once(
    home: &Path,
    runtime: &filebeam_client_core::DeviceRuntime,
    jobs: &Arc<Mutex<DeviceJobs>>,
    active: &Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
    certificate: Option<reqwest::Certificate>,
    stop: &AtomicBool,
    #[cfg(unix)] lan_enabled: Option<&Arc<AtomicBool>>,
) -> Result<()> {
    let Some(provision) =
        daemon_store(home)?.load_named::<DeviceProvision>("device-provision-v1.json")?
    else {
        return Ok(());
    };
    let scope = registry_scope(&provision)?;
    let key = signing_key(&provision)?;
    let pending = jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
        .pending();
    for job in pending {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let peer = filebeam_client_core::device_registry::DeviceId::parse(job.target.clone())?;
        if !job.prepared {
            let (target, local) = {
                let store = registry_store(home)?;
                let registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                    &store,
                    scope.clone(),
                    Duration::from_secs(7 * 24 * 60 * 60),
                )?;
                let target = registry.pinned_target(
                    &scope,
                    &peer,
                    filebeam_client_core::device_registry::RegistryRoute::Instance,
                    now(),
                )?;
                (target, registry.local_device()?)
            };
            prepare(
                home,
                jobs,
                active,
                &job,
                local.id().to_owned(),
                local.key_version(),
                target.id.as_str().to_owned(),
                target.key_version,
            )?;
            continue;
        }
        #[cfg(unix)]
        if lan_enabled.is_some_and(|enabled| enabled.load(Ordering::Relaxed)) {
            let observation = {
                let store = registry_store(home)?;
                let registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                    &store,
                    scope.clone(),
                    Duration::from_secs(7 * 24 * 60 * 60),
                )?;
                crate::lan_runtime::try_send(runtime, &registry, &scope, &peer, &key, home, &job)?
            };
            if let Some(observation) = observation {
                start_transfer(jobs, active, job.id, observation)?;
                continue;
            }
        }
        // The session owns the native canonical registry cache, so release the
        // encrypted Store before any potentially slow network operation.
        let session = {
            let store = registry_store(home)?;
            let registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                &store,
                scope.clone(),
                Duration::from_secs(7 * 24 * 60 * 60),
            )?;
            registry.peer_session(
                &scope,
                &peer,
                filebeam_client_core::device_registry::RegistryRoute::Instance,
                now(),
            )?
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let signaling = filebeam_transfer_native::webrtc::DeviceSignaling::new(
            signaling_client(certificate.clone())?,
            &provision.instance,
            &provision.credential,
        )?;
        let (native, connection, channels) =
            filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
                filebeam_transfer_native::webrtc::connect_device_initiator(
                    &signaling,
                    peer.as_str(),
                    &[],
                    false,
                ),
            )?;
        if stop.load(Ordering::Acquire) {
            break;
        }
        let authenticated = filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
            session.authenticate(native, channels, Arc::new(key.clone()), &connection.id),
        )?;
        let store = filebeam_transfer_native::checkpoint::Store::open(
            &home.join("transfers"),
            &job.id.to_string(),
        )?;
        let prepared = filebeam_transfer_native::protocol::peer::load_peer_job(&store)?;
        drop(store);
        let observation = runtime.start_peer_sender(
            authenticated,
            prepared,
            job.grant_id.clone(),
            None,
            job.expires_at_unix_seconds,
        );
        start_transfer(jobs, active, job.id, observation)?;
    }
    Ok(())
}

fn prepare(
    home: &Path,
    jobs: &Arc<Mutex<DeviceJobs>>,
    active: &Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
    job: &crate::DeviceJob,
    sender: String,
    sender_version: u32,
    receiver: String,
    receiver_version: u32,
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
            sender,
            sender_version,
            receiver,
            receiver_version,
            filebeam_transfer_native::webrtc::FRAME_PAYLOAD_BYTES as u64,
            control,
        );
        let _ = jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
            .and_then(|mut jobs| {
                if jobs.list().iter().any(|job| {
                    job.id == id
                        && matches!(
                            job.state,
                            DeviceJobState::Paused | DeviceJobState::Cancelled
                        )
                }) {
                    Ok(())
                } else {
                    jobs.set_state(
                        id,
                        if result.is_ok() {
                            DeviceJobState::AwaitingPeerConfiguration
                        } else {
                            DeviceJobState::Failed
                        },
                        result.is_ok(),
                    )
                }
            });
        let _ = active.lock().map(|mut active| active.remove(&id));
    });
    Ok(())
}
fn start_transfer(
    jobs: &Arc<Mutex<DeviceJobs>>,
    active: &Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
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
    active: Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
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
                        if jobs.list().iter().any(|job| {
                            job.id == id
                                && matches!(
                                    job.state,
                                    DeviceJobState::Paused | DeviceJobState::Cancelled
                                )
                        }) {
                            Ok(())
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
