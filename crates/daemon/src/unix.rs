#![cfg(unix)]

use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        io::{AsRawFd, FromRawFd},
    },
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::ActiveJob;
use crate::lan_runtime;
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
#[cfg(test)]
use ed25519_dalek::VerifyingKey;
use filebeam_client_core::JobAction;
use fs2::FileExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub const IPC_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;

pub use crate::grants::DestinationGrants;
pub use crate::jobs::DeviceJobs;
pub use crate::receiver::DaemonPeerReceiver;
pub use crate::schema::*;

#[derive(Debug, Serialize, Deserialize)]
struct ServerHello {
    version: u32,
    nonce: [u8; 32],
    proof: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientHello {
    version: u32,
    nonce: [u8; 32],
    proof: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthenticatedPayload {
    payload: Vec<u8>,
    proof: [u8; 32],
}

impl std::fmt::Debug for DeviceProvision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceProvision")
            .field("instance", &self.instance)
            .field("deployment", &self.deployment)
            .field("account_id", &self.account_id)
            .field("generation", &self.generation)
            .field("device_id", &self.device_id)
            .field("key_version", &self.key_version)
            .field("public_key", &"[redacted]")
            .field("signing_key", &"[redacted]")
            .field("credential", &"[redacted]")
            .finish()
    }
}

impl DeviceProvision {
    #[cfg(test)]
    fn validate(&self) -> Result<()> {
        filebeam_client_core::device_registry::Origin::parse(self.instance.clone())?;
        filebeam_client_core::device_registry::DeploymentId::parse(self.deployment.clone())?;
        filebeam_client_core::device_registry::DeviceId::parse(self.device_id.clone())?;
        if self.generation == 0
            || self.key_version == 0
            || self.credential.len() != 64
            || !self
                .credential
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
        {
            bail!("device provisioning identity is invalid");
        }
        let public = STANDARD
            .decode(&self.public_key)
            .context("device public key is not base64")?;
        let public: [u8; 32] = public
            .try_into()
            .map_err(|_| anyhow::anyhow!("device public key has an invalid length"))?;
        let signing = STANDARD
            .decode(&self.signing_key)
            .context("device signing key is not base64")?;
        let signing: [u8; 32] = signing
            .try_into()
            .map_err(|_| anyhow::anyhow!("device signing key has an invalid length"))?;
        let signing = SigningKey::from_bytes(&signing);
        if signing.verifying_key()
            != VerifyingKey::from_bytes(&public).context("device public key is invalid")?
        {
            bail!("device signing key does not match its public key");
        }
        Ok(())
    }
}

pub fn socket_path(home: &Path) -> PathBuf {
    home.join("runtime").join("daemon").join("ipc.sock")
}

pub fn request(home: &Path, request: Request) -> Result<Reply> {
    use std::os::unix::net::UnixStream;

    let path = socket_path(home);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("inspect daemon endpoint {}", path.display()))?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        bail!("daemon endpoint is not an owner-only socket");
    }
    let mut stream = UnixStream::connect(&path).context("connect to Filebeam daemon")?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    let key = ipc_key(home)?;
    let server: ServerHello = read_message(&mut stream)?;
    if server.version != IPC_VERSION
        || server.proof != proof(&key, b"filebeam-daemon-server-v1", &[&server.nonce])
    {
        bail!("daemon identity authentication failed");
    }
    let client_nonce = random_nonce()?;
    write_message(
        &mut stream,
        &ClientHello {
            version: IPC_VERSION,
            nonce: client_nonce,
            proof: proof(
                &key,
                b"filebeam-daemon-client-v1",
                &[&server.nonce, &client_nonce],
            ),
        },
    )?;
    let payload = serde_json::to_vec(&request)?;
    write_message(
        &mut stream,
        &AuthenticatedPayload {
            proof: proof(
                &key,
                b"filebeam-daemon-request-v1",
                &[&server.nonce, &client_nonce, &payload],
            ),
            payload,
        },
    )?;
    let response: AuthenticatedPayload = read_message(&mut stream)?;
    if response.proof
        != proof(
            &key,
            b"filebeam-daemon-response-v1",
            &[&server.nonce, &client_nonce, &response.payload],
        )
    {
        bail!("daemon response authentication failed");
    }
    serde_json::from_slice(&response.payload).context("decode daemon response")
}

pub struct Daemon {
    home: PathBuf,
    state: DaemonState,
    ipc_key: [u8; 32],
    grants: DestinationGrants,
    jobs: Arc<Mutex<DeviceJobs>>,
    active_jobs: Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
    receive_enabled: Arc<AtomicBool>,
    lan_enabled: Arc<AtomicBool>,
    device_runtime: filebeam_client_core::DeviceRuntime,
    trusted_certificate: Option<reqwest::Certificate>,
    inbound_connections: Arc<Mutex<HashSet<String>>>,
    dispatcher: Option<crate::dispatcher::Dispatcher>,
    inbound_stop: Arc<AtomicBool>,
    inbound_thread: Option<std::thread::JoinHandle<()>>,
    lan_stop: Arc<AtomicBool>,
    lan_thread: Option<std::thread::JoinHandle<()>>,
    _lock: File,
}

impl Daemon {
    pub fn open(home: &Path) -> Result<Self> {
        Self::open_with_certificate(home, None)
    }

    /// Opens a daemon for an explicitly configured local instance CA. Normal
    /// hostname and chain verification remain enabled.
    pub fn open_with_trusted_certificate(
        home: &Path,
        certificate: reqwest::Certificate,
    ) -> Result<Self> {
        Self::open_with_certificate(home, Some(certificate))
    }

    fn open_with_certificate(
        home: &Path,
        trusted_certificate: Option<reqwest::Certificate>,
    ) -> Result<Self> {
        let home = secure_home(home)?;
        let runtime = home.join("runtime").join("daemon");
        secure_directory(&runtime)?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(runtime.join("owner.lock"))?;
        lock.try_lock_exclusive()
            .context("another Filebeam daemon already owns this profile")?;
        let mut state = read_state(&runtime.join("state.json"))?;
        let ipc_key = ipc_key(&home)?;
        let grants = DestinationGrants::open(&home)?;
        let jobs = Arc::new(Mutex::new(DeviceJobs::open(&home)?));
        let receive_enabled = Arc::new(AtomicBool::new(state.receive_enabled));
        let lan_enabled = Arc::new(AtomicBool::new(state.lan_enabled));
        if daemon_store(&home)?
            .load_named::<DeviceProvision>("device-provision-v1.json")?
            .is_some()
        {
            state.runtime = RuntimeAvailability::Provisioned;
        }
        let state_home = home.join("transfers");
        let settings = filebeam_client_core::control::TransferSettings {
            state_home: state_home.clone(),
            max_concurrency: Some(2),
            memory_budget: 512 * 1024 * 1024,
            client_user_agent: Some("filebeamd".into()),
            webrtc_relay_only: false,
            checkpoint_secret_store: Some(
                filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(
                    state_home,
                ),
            ),
            source_resolver: None,
        };
        let device_runtime = filebeam_client_core::DeviceRuntime::new(
            filebeam_client_core::ApplicationSettings::new(
                settings,
                filebeam_client_core::SchedulerLimits {
                    workers: 2,
                    memory_bytes: 1024 * 1024 * 1024,
                },
            ),
        )?;
        Ok(Self {
            home,
            state,
            ipc_key,
            grants,
            jobs,
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
            receive_enabled,
            lan_enabled,
            device_runtime,
            trusted_certificate,
            inbound_connections: Arc::new(Mutex::new(HashSet::new())),
            dispatcher: None,
            inbound_stop: Arc::new(AtomicBool::new(false)),
            inbound_thread: None,
            lan_stop: Arc::new(AtomicBool::new(false)),
            lan_thread: None,
            _lock: lock,
        })
    }

    pub fn run(mut self) -> Result<()> {
        use std::os::unix::net::UnixListener;

        let path = socket_path(&self.home);
        remove_stale_socket(&path)?;
        let listener =
            UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        self.start_dispatcher();
        self.lan_thread = Some(lan_runtime::start(
            self.home.clone(),
            self.device_runtime.clone(),
            self.receive_enabled.clone(),
            self.lan_enabled.clone(),
            self.lan_stop.clone(),
        ));
        let mut shutdown = false;
        while !shutdown {
            let (mut stream, _) = listener.accept()?;
            if !same_user(&stream)? {
                continue;
            }
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
            stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
            let server_nonce = random_nonce()?;
            if write_message(
                &mut stream,
                &ServerHello {
                    version: IPC_VERSION,
                    nonce: server_nonce,
                    proof: proof(
                        &self.ipc_key,
                        b"filebeam-daemon-server-v1",
                        &[&server_nonce],
                    ),
                },
            )
            .is_err()
            {
                continue;
            }
            let client: ClientHello = match read_message(&mut stream) {
                Ok(client) => client,
                Err(_) => continue,
            };
            if client.version != IPC_VERSION
                || client.proof
                    != proof(
                        &self.ipc_key,
                        b"filebeam-daemon-client-v1",
                        &[&server_nonce, &client.nonce],
                    )
            {
                continue;
            }
            let request: AuthenticatedPayload = match read_message(&mut stream) {
                Ok(request) => request,
                Err(_) => continue,
            };
            let reply = if request.proof
                != proof(
                    &self.ipc_key,
                    b"filebeam-daemon-request-v1",
                    &[&server_nonce, &client.nonce, &request.payload],
                ) {
                Reply {
                    version: IPC_VERSION,
                    state: self.state.clone(),
                    error: Some("IPC request authentication failed".into()),
                    destinations: Vec::new(),
                    jobs: Vec::new(),
                    devices: Vec::new(),
                }
            } else {
                match serde_json::from_slice::<Request>(&request.payload) {
                    Ok(Request::Status { version }) => self.reply_for_version(version),
                    Ok(Request::SetReceiveEnabled { version, enabled }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            self.state.receive_enabled = enabled;
                            self.receive_enabled.store(enabled, Ordering::Relaxed);
                            if let Err(error) = write_state(
                                &self.home.join("runtime/daemon/state.json"),
                                &self.state,
                            ) {
                                reply.error = Some(error.to_string());
                            }
                            reply.state = self.state.clone();
                        }
                        reply
                    }
                    Ok(Request::SetLanEnabled { version, enabled }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            self.state.lan_enabled = enabled;
                            self.lan_enabled.store(enabled, Ordering::Relaxed);
                            if let Err(error) = write_state(
                                &self.home.join("runtime/daemon/state.json"),
                                &self.state,
                            ) {
                                reply.error = Some(error.to_string());
                            }
                            reply.state = self.state.clone();
                        }
                        reply
                    }
                    Ok(Request::ListDestinations { version }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            reply.destinations = self.grants.list().to_vec();
                        }
                        reply
                    }
                    Ok(Request::AddDestination {
                        version,
                        alias,
                        path,
                        approval,
                    }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            match self.grants.add(&alias, &path, approval) {
                                Ok(grant) => reply.destinations = vec![grant],
                                Err(error) => reply.error = Some(error.to_string()),
                            }
                        }
                        reply
                    }
                    Ok(Request::RevokeDestination {
                        version,
                        alias_or_id,
                    }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            match self.grants.revoke(&alias_or_id) {
                                Ok(grant) => reply.destinations = vec![grant],
                                Err(error) => reply.error = Some(error.to_string()),
                            }
                        }
                        reply
                    }
                    Ok(Request::ListJobs { version }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            reply.jobs = self
                                .jobs
                                .lock()
                                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                                .list()
                                .to_vec();
                        }
                        reply
                    }
                    Ok(Request::EnqueueJob {
                        version,
                        target,
                        grant_id,
                        sources,
                    }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            match self
                                .jobs
                                .lock()
                                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                                .enqueue(&target, &grant_id, sources)
                            {
                                Ok(job) => reply.jobs = vec![job],
                                Err(error) => reply.error = Some(error.to_string()),
                            }
                        }
                        reply
                    }
                    Ok(Request::ProvisionDevice { version, provision }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            match self.provision(provision) {
                                Ok(()) => reply.state = self.state.clone(),
                                Err(error) => reply.error = Some(error.to_string()),
                            }
                        }
                        reply
                    }
                    Ok(Request::ListDevices { version }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            match self.list_devices() {
                                Ok(devices) => reply.devices = devices,
                                Err(error) => reply.error = Some(error.to_string()),
                            }
                        }
                        reply
                    }
                    Ok(Request::ConfirmPeerVerification {
                        version,
                        device_id,
                        key_version,
                        fingerprint,
                        confirmation_nonce,
                    }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            if let Err(error) = self.confirm_peer(
                                &device_id,
                                key_version,
                                &fingerprint,
                                &confirmation_nonce,
                            ) {
                                reply.error = Some(error.to_string());
                            }
                        }
                        reply
                    }
                    Ok(Request::ControlJob {
                        version,
                        id,
                        action,
                    }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            let validation = self
                                .jobs
                                .lock()
                                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                                .validate_control(id, &action);
                            if let Err(error) = validation {
                                reply.error = Some(error.to_string());
                            } else if let Err(error) = self.control_active_job(id, &action) {
                                reply.error = Some(error.to_string());
                            } else {
                                match self
                                    .jobs
                                    .lock()
                                    .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                                    .control(id, action)
                                {
                                    Ok(job) => reply.jobs = vec![job],
                                    Err(error) => reply.error = Some(error.to_string()),
                                }
                            }
                        }
                        reply
                    }
                    Ok(Request::Shutdown { version }) => {
                        let mut reply = self.reply_for_version(version);
                        shutdown = reply.error.is_none();
                        if shutdown {
                            if let Err(error) = self.stop_background() {
                                shutdown = false;
                                reply.error = Some(error.to_string());
                            } else {
                                let _ = self.device_runtime.shutdown(Duration::from_secs(10));
                            }
                        }
                        reply
                    }
                    Err(_) => Reply {
                        version: IPC_VERSION,
                        state: self.state.clone(),
                        error: Some("invalid IPC request".into()),
                        destinations: Vec::new(),
                        jobs: Vec::new(),
                        devices: Vec::new(),
                    },
                }
            };
            let payload = serde_json::to_vec(&reply)?;
            if write_message(
                &mut stream,
                &AuthenticatedPayload {
                    proof: proof(
                        &self.ipc_key,
                        b"filebeam-daemon-response-v1",
                        &[&server_nonce, &client.nonce, &payload],
                    ),
                    payload,
                },
            )
            .is_err()
            {
                continue;
            }
        }
        self.stop_background()?;
        let _ = self.device_runtime.shutdown(Duration::from_secs(10));
        let _ = fs::remove_file(path);
        Ok(())
    }

    fn provision(&mut self, provision: DeviceProvision) -> Result<()> {
        crate::ControlPlane::new(&self.home, self.trusted_certificate.clone())
            .provision(&mut self.state, provision)
    }

    fn list_devices(&self) -> Result<Vec<DaemonDevice>> {
        crate::ControlPlane::new(&self.home, self.trusted_certificate.clone()).list_devices()
    }

    fn confirm_peer(
        &self,
        device_id: &str,
        key_version: u32,
        fingerprint: &str,
        confirmation_nonce: &str,
    ) -> Result<()> {
        crate::ControlPlane::new(&self.home, self.trusted_certificate.clone()).confirm_peer(
            device_id,
            key_version,
            fingerprint,
            confirmation_nonce,
        )
    }

    fn start_dispatcher(&mut self) {
        self.dispatcher = Some(crate::dispatcher::start(
            self.home.clone(),
            self.device_runtime.clone(),
            self.jobs.clone(),
            self.active_jobs.clone(),
            self.trusted_certificate.clone(),
            self.receive_enabled.clone(),
            Some(self.lan_enabled.clone()),
        ));
    }

    fn stop_background(&mut self) -> Result<()> {
        self.inbound_stop.store(true, Ordering::Release);
        self.lan_stop.store(true, Ordering::Release);
        if let Some(dispatcher) = self.dispatcher.as_mut() {
            dispatcher.shutdown(Duration::from_secs(60))?;
        }
        self.dispatcher = None;
        if let Some(thread) = self.inbound_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.lan_thread.take() {
            let _ = thread.join();
        }
        Ok(())
    }

    fn control_active_job(&self, id: uuid::Uuid, action: &DeviceJobState) -> Result<()> {
        let active = self
            .active_jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?
            .get(&id)
            .cloned();
        match (active, action) {
            (
                Some(ActiveJob::Preparing(control)),
                DeviceJobState::Paused | DeviceJobState::Cancelled,
            ) => control.cancel(),
            (
                Some(ActiveJob::Transferring(observation)),
                DeviceJobState::Paused | DeviceJobState::Cancelled,
            ) => {
                self.device_runtime
                    .client_runtime()
                    .dispatch(observation.id(), JobAction::Pause)?;
            }
            (Some(ActiveJob::Transferring(_)), DeviceJobState::AwaitingPeerConfiguration) => {
                bail!("active device jobs must pause before resuming")
            }
            _ => {}
        }
        Ok(())
    }

    fn reply_for_version(&self, version: u32) -> Reply {
        Reply {
            version: IPC_VERSION,
            state: self.state.clone(),
            error: (version != IPC_VERSION)
                .then(|| "IPC version mismatch; upgrade the client or daemon".into()),
            destinations: Vec::new(),
            jobs: Vec::new(),
            devices: Vec::new(),
        }
    }
}

fn dispatch_once(
    home: &Path,
    runtime: &filebeam_client_core::DeviceRuntime,
    _jobs: &Arc<Mutex<DeviceJobs>>,
    _active_jobs: &Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
    receive_enabled: &Arc<AtomicBool>,
    _lan_enabled: bool,
    trusted_certificate: Option<reqwest::Certificate>,
    inbound_connections: &Arc<Mutex<HashSet<String>>>,
    refresh: bool,
) -> Result<()> {
    let Some(provision) =
        daemon_store(home)?.load_named::<DeviceProvision>("device-provision-v1.json")?
    else {
        return Ok(());
    };
    let client = service_client(&provision.instance, trusted_certificate.clone())?;
    // A successful authenticated sync refreshes the encrypted registry cache;
    // connection admission still reads the cache through DeviceRegistry gates.
    if refresh {
        match client.devices().device_sync(&provision.credential) {
            Ok(snapshot) => refresh_registry(home, &provision, snapshot)?,
            Err(error) => {
                // Only an authenticated control-plane denial or malformed
                // response invalidates offline trust. Transport and 5xx errors
                // leave the bounded cache available to the OfflineLan gate.
                if let Some(failure) = registry_sync_failure(&error.to_string()) {
                    let store = registry_store(home)?;
                    let mut registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                        &store,
                        registry_scope(&provision)?,
                        Duration::from_secs(7 * 24 * 60 * 60),
                    )?;
                    registry.fail(failure)?;
                }
            }
        }
    }
    let store = registry_store(home)?;
    let scope = registry_scope(&provision)?;
    let registry = filebeam_client_core::device_registry::DeviceRegistry::open(
        &store,
        scope.clone(),
        Duration::from_secs(7 * 24 * 60 * 60),
    )?;
    let signing_key = provision_signing_key(&provision)?;
    if receive_enabled.load(Ordering::Relaxed) {
        let signaling = filebeam_transfer_native::webrtc::DeviceSignaling::new(
            signaling_client(trusted_certificate.clone())?,
            &provision.instance,
            &provision.credential,
        )?;
        receive_incoming(
            home,
            runtime,
            &registry,
            &scope,
            &signaling,
            &signing_key,
            receive_enabled.load(Ordering::Relaxed),
            inbound_connections,
        )?;
    }
    Ok(())
}

fn registry_sync_failure(
    message: &str,
) -> Option<filebeam_client_core::device_registry::SyncFailure> {
    use filebeam_client_core::device_registry::SyncFailure;

    if message.contains("returned 401") {
        Some(SyncFailure::Unauthorized)
    } else if message.contains("returned 403") {
        Some(SyncFailure::Forbidden)
    } else if message.contains("native device request returned 4") {
        Some(SyncFailure::Disabled)
    } else if message.contains("decode native device response")
        || message.contains("response exceeds the control-plane limit")
    {
        Some(SyncFailure::Malformed)
    } else {
        None
    }
}

fn receive_incoming(
    home: &Path,
    runtime: &filebeam_client_core::DeviceRuntime,
    registry: &filebeam_client_core::device_registry::DeviceRegistry<'_>,
    scope: &filebeam_client_core::device_registry::RegistryScope,
    signaling: &filebeam_transfer_native::webrtc::DeviceSignaling,
    signing_key: &SigningKey,
    receive_enabled: bool,
    inbound_connections: &Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    let incoming =
        filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(signaling.incoming())?;
    for connection in incoming.connections {
        if !inbound_connections
            .lock()
            .map_err(|_| anyhow::anyhow!("inbound connection state is poisoned"))?
            .insert(connection.id.clone())
        {
            continue;
        }
        let peer =
            filebeam_client_core::device_registry::DeviceId::parse(connection.source_device_id)?;
        let session = registry.peer_session(
            scope,
            &peer,
            filebeam_client_core::device_registry::RegistryRoute::Instance,
            unix_now_i64(),
        )?;
        let servers = filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(signaling.ice_servers(&connection.id))?;
        let (native_peer, _, channels) = filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(filebeam_transfer_native::webrtc::connect_device_responder(
                signaling,
                &connection.id,
                &servers,
                false,
            ))?;
        let authenticated = filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
            session.authenticate(
                native_peer,
                channels,
                Arc::new(signing_key.clone()),
                &connection.id,
            ),
        )?;
        runtime.start_peer_receiver(
            authenticated,
            Arc::new(DaemonPeerReceiver::new(home.into(), receive_enabled)),
            Arc::new(signing_key.clone()),
        );
    }
    Ok(())
}

fn dispatch_outbound(
    home: &Path,
    runtime: &filebeam_client_core::DeviceRuntime,
    jobs: &Arc<Mutex<DeviceJobs>>,
    active_jobs: &Arc<Mutex<HashMap<uuid::Uuid, ActiveJob>>>,
    registry: &filebeam_client_core::device_registry::DeviceRegistry<'_>,
    scope: &filebeam_client_core::device_registry::RegistryScope,
    trusted_certificate: Option<reqwest::Certificate>,
    instance: &str,
    credential: &str,
    signing_key: SigningKey,
    lan_enabled: bool,
) -> Result<()> {
    let pending = jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
        .pending();
    for job in pending {
        let peer = filebeam_client_core::device_registry::DeviceId::parse(job.target.clone())?;
        let target = registry.pinned_target(
            scope,
            &peer,
            filebeam_client_core::device_registry::RegistryRoute::Instance,
            unix_now_i64(),
        )?;
        if !job.prepared {
            jobs.lock()
                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                .set_state(job.id, DeviceJobState::Preparing, false)?;
            let checkpoint = job.id.to_string();
            let state_home = home.join("transfers");
            let store =
                filebeam_transfer_native::checkpoint::Store::create(&state_home, &checkpoint)?;
            let control = filebeam_transfer_native::control::Control::new(
                daemon_transfer_settings(state_home),
                std::sync::mpsc::channel().0,
            );
            active_jobs
                .lock()
                .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?
                .insert(job.id, ActiveJob::Preparing(control.clone()));
            let local = registry.local_device()?;
            let sources = job.sources.clone();
            let job_id = job.id;
            let sender_id = local.id().to_owned();
            let sender_key_version = local.key_version();
            let receiver_id = target.id.as_str().to_owned();
            let receiver_key_version = target.key_version;
            let jobs = jobs.clone();
            let active_jobs = active_jobs.clone();
            // Preparation can scan and encrypt a large tree. It owns the native
            // Control in the daemon registry so IPC can interrupt it promptly.
            std::thread::spawn(move || {
                let result = filebeam_transfer_native::protocol::peer::prepare_peer_tree_job(
                    &store,
                    &sources,
                    Default::default(),
                    sender_id,
                    sender_key_version,
                    receiver_id,
                    receiver_key_version,
                    filebeam_transfer_native::webrtc::FRAME_PAYLOAD_BYTES as u64,
                    control,
                );
                let _ = jobs
                    .lock()
                    .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
                    .and_then(|mut jobs| {
                        let stopped = jobs
                            .list()
                            .iter()
                            .find(|candidate| candidate.id == job_id)
                            .is_some_and(|candidate| {
                                matches!(
                                    candidate.state,
                                    DeviceJobState::Paused | DeviceJobState::Cancelled
                                )
                            });
                        if stopped {
                            return Ok(());
                        }
                        jobs.set_state(
                            job_id,
                            if result.is_ok() {
                                DeviceJobState::AwaitingPeerConfiguration
                            } else {
                                DeviceJobState::Failed
                            },
                            result.is_ok(),
                        )
                    });
                let _ = active_jobs.lock().map(|mut active| active.remove(&job_id));
            });
            continue;
        }
        if lan_enabled
            && let Some(observation) =
                lan_runtime::try_send(runtime, registry, scope, &peer, &signing_key, home, &job)?
        {
            active_jobs
                .lock()
                .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?
                .insert(job.id, ActiveJob::Transferring(observation.clone()));
            jobs.lock()
                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                .set_state(job.id, DeviceJobState::Transferring, true)?;
            let jobs = jobs.clone();
            let active_jobs = active_jobs.clone();
            let job_id = job.id;
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
                            .and_then(|mut jobs| jobs.set_state(job_id, state, true));
                        let _ = active_jobs.lock().map(|mut active| active.remove(&job_id));
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
            continue;
        }
        let session = registry.peer_session(
            scope,
            &peer,
            filebeam_client_core::device_registry::RegistryRoute::Instance,
            unix_now_i64(),
        )?;
        let signaling = filebeam_transfer_native::webrtc::DeviceSignaling::new(
            signaling_client(trusted_certificate.clone())?,
            instance,
            credential,
        )?;
        // Host candidates provide a real direct path when the peer is reachable;
        // the responder obtains the authorized ICE list for its rendezvous ID.
        let (native_peer, connection, channels) =
            filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
                filebeam_transfer_native::webrtc::connect_device_initiator(
                    &signaling,
                    peer.as_str(),
                    &[],
                    false,
                ),
            )?;
        let authenticated = filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
            session.authenticate(
                native_peer,
                channels,
                Arc::new(signing_key.clone()),
                &connection.id,
            ),
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
        let mut active = active_jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("active device job state is poisoned"))?;
        active.insert(job.id, ActiveJob::Transferring(observation.clone()));
        jobs.lock()
            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
            .set_state(job.id, DeviceJobState::Transferring, true)?;
        drop(active);
        let jobs = jobs.clone();
        let active_jobs = active_jobs.clone();
        std::thread::spawn(move || {
            loop {
                let Some(snapshot) = observation.snapshot() else {
                    return;
                };
                match snapshot.job.state {
                    filebeam_client_core::JobState::Complete => {
                        let _ = jobs
                            .lock()
                            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
                            .and_then(|mut jobs| {
                                jobs.set_state(job.id, DeviceJobState::Delivered, true)
                            });
                        let _ = active_jobs.lock().map(|mut active| active.remove(&job.id));
                        return;
                    }
                    filebeam_client_core::JobState::Paused => {
                        let _ = jobs
                            .lock()
                            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
                            .and_then(|mut jobs| {
                                if jobs
                                    .list()
                                    .iter()
                                    .find(|candidate| candidate.id == job.id)
                                    .is_some_and(|candidate| {
                                        candidate.state != DeviceJobState::Cancelled
                                    })
                                {
                                    jobs.set_state(job.id, DeviceJobState::Paused, true)
                                } else {
                                    Ok(())
                                }
                            });
                        let _ = active_jobs.lock().map(|mut active| active.remove(&job.id));
                        return;
                    }
                    filebeam_client_core::JobState::Failed => {
                        let _ = jobs
                            .lock()
                            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))
                            .and_then(|mut jobs| {
                                jobs.set_state(job.id, DeviceJobState::Failed, true)
                            });
                        let _ = active_jobs.lock().map(|mut active| active.remove(&job.id));
                        return;
                    }
                    _ => std::thread::sleep(Duration::from_millis(250)),
                }
            }
        });
    }
    Ok(())
}

fn daemon_transfer_settings(
    state_home: PathBuf,
) -> filebeam_client_core::control::TransferSettings {
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

pub(crate) fn provision_signing_key(provision: &DeviceProvision) -> Result<SigningKey> {
    let bytes: [u8; 32] = STANDARD
        .decode(&provision.signing_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("device signing key has an invalid length"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn service_client(
    instance: &str,
    certificate: Option<reqwest::Certificate>,
) -> Result<filebeam_client_core::services::ServiceClient> {
    match certificate {
        Some(certificate) => {
            filebeam_client_core::services::ServiceClient::new_with_trusted_certificate(
                instance,
                certificate,
            )
        }
        None => filebeam_client_core::services::ServiceClient::new(instance),
    }
}

fn signaling_client(certificate: Option<reqwest::Certificate>) -> Result<reqwest::Client> {
    let mut client = reqwest::Client::builder();
    if let Some(certificate) = certificate {
        client = client.add_root_certificate(certificate);
    }
    client.build().context("create device signaling client")
}

pub(crate) fn registry_scope(
    provision: &DeviceProvision,
) -> Result<filebeam_client_core::device_registry::RegistryScope> {
    use filebeam_client_core::device_registry::{
        AccountId, DeploymentId, DeviceGeneration, DeviceId, Origin, RegistryScope,
    };
    Ok(RegistryScope {
        origin: Origin::parse(provision.instance.clone())?,
        deployment: DeploymentId::parse(provision.deployment.clone())?,
        account: AccountId(provision.account_id),
        generation: DeviceGeneration(provision.generation),
        local_device: DeviceId::parse(provision.device_id.clone())?,
    })
}

fn unix_now_u64() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_secs())
}

pub(crate) fn unix_now_i64() -> i64 {
    i64::try_from(unix_now_u64().unwrap_or(u64::MAX)).unwrap_or(i64::MAX)
}

/// A receiver-owned directory handle. This is intentionally separate from a sender path string.
pub struct ConfinedRoot {
    root: File,
    device: u64,
    inode: u64,
}

/// Private receive staging rooted in an already-verified directory descriptor.
/// No operation below this type resolves the directory through a pathname.
pub(crate) struct StagingDirectory {
    directory: File,
    targets: Vec<Vec<String>>,
}

impl StagingDirectory {
    pub(crate) fn new(directory: File, targets: Vec<Vec<String>>) -> Self {
        Self { directory, targets }
    }

    fn open(&self, name: &str, flags: libc::c_int, mode: libc::mode_t) -> Result<File> {
        let name = CString::new(name)?;
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                mode,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn target(&self, item: usize) -> Result<File> {
        let components = self
            .targets
            .get(item)
            .context("peer staging target is missing")?;
        let (name, parents) = components
            .split_last()
            .context("peer staging target is empty")?;
        let mut directory = self.directory.try_clone()?;
        for parent in parents {
            directory = open_directory(&directory, std::ffi::OsStr::new(parent))?;
        }
        let name = CString::new(name.as_bytes())?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

impl filebeam_transfer_native::protocol::peer::PeerReceiveStaging for StagingDirectory {
    fn read_ciphertext(&self, item: usize, chunk: u64) -> Result<Vec<u8>> {
        let mut file = self.open(&format!("cipher-{item}-{chunk}"), libc::O_RDONLY, 0)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn write_ciphertext(&self, item: usize, chunk: u64, ciphertext: &[u8]) -> Result<()> {
        let mut file = self.open(
            &format!("cipher-{item}-{chunk}"),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        file.write_all(ciphertext)?;
        file.sync_all()?;
        self.directory.sync_all()?;
        Ok(())
    }

    fn write_plain(&self, item: usize, offset: u64, plain: &[u8]) -> Result<()> {
        let mut file = self.open(
            &format!("plain-{item}"),
            libc::O_WRONLY | libc::O_CREAT,
            0o600,
        )?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(plain)?;
        file.sync_all()?;
        Ok(())
    }

    fn read_plain(&self, item: usize) -> Result<File> {
        self.open(&format!("plain-{item}"), libc::O_RDONLY, 0)
    }

    fn write_target(&self, item: usize, plain: &mut File) -> Result<()> {
        let mut target = self.target(item)?;
        std::io::copy(plain, &mut target)?;
        target.sync_all()?;
        self.directory.sync_all()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitState {
    Intent,
    Renamed,
    ReceiptPersisted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitRecord {
    pub job_id: String,
    pub grant_id: uuid::Uuid,
    pub grant_revision: u64,
    pub staging_name: String,
    pub destination_name: String,
    #[serde(default)]
    pub destination_selected: bool,
    #[serde(default)]
    pub staged_identity: Option<CommitObjectIdentity>,
    pub state: CommitState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitObjectIdentity {
    pub device: u64,
    pub inode: u64,
}

/// Receiver-owned commit metadata. It persists in the existing encrypted checkpoint Store.
/// A crash after rename but before the journal advances remains intentionally ambiguous.
pub struct CommitJournal {
    home: PathBuf,
}

impl CommitJournal {
    pub fn new(home: &Path) -> Self {
        Self { home: home.into() }
    }

    pub fn begin(
        &self,
        job_id: &str,
        grant: &DestinationGrant,
        staging_name: &str,
        destination_name: &str,
    ) -> Result<CommitRecord> {
        validate_commit_name(job_id, "job")?;
        validate_top_level_name(staging_name)?;
        validate_top_level_name(destination_name)?;
        let name = record_name(job_id);
        let store = daemon_store(&self.home)?;
        let proposed = CommitRecord {
            job_id: job_id.into(),
            grant_id: grant.id,
            grant_revision: grant.revision,
            staging_name: staging_name.into(),
            destination_name: destination_name.into(),
            destination_selected: true,
            staged_identity: None,
            state: CommitState::Intent,
        };
        if let Some(existing) = store.load_named::<CommitRecord>(&name)? {
            if existing.grant_id == proposed.grant_id
                && existing.grant_revision == proposed.grant_revision
                && existing.staging_name == proposed.staging_name
                && existing.destination_name == proposed.destination_name
            {
                return Ok(existing);
            }
            bail!("commit journal conflicts with an existing job");
        }
        store.save_named(&name, &proposed)?;
        Ok(proposed)
    }

    pub fn commit(
        &self,
        root: &ConfinedRoot,
        mut record: CommitRecord,
        receipt: &[u8],
    ) -> Result<Vec<u8>> {
        if receipt.is_empty() || receipt.len() > 256 * 1024 {
            bail!("commit receipt is invalid");
        }
        let store = daemon_store(&self.home)?;
        if record.state == CommitState::ReceiptPersisted {
            return store
                .load_secret(&receipt_name(&record.job_id))?
                .context("committed receipt is missing");
        }
        if record.state == CommitState::Intent {
            rename_no_replace(root, &record.staging_name, &record.destination_name)?;
            root.root.sync_all()?;
            record.state = CommitState::Renamed;
            store.save_named(&record_name(&record.job_id), &record)?;
        }
        store.save_secret(&receipt_name(&record.job_id), receipt)?;
        record.state = CommitState::ReceiptPersisted;
        store.save_named(&record_name(&record.job_id), &record)?;
        Ok(receipt.to_vec())
    }

    /// Commit one selected top-level root from a private job staging directory.
    /// A peer job can contain several independently selected roots, so the
    /// staging directory itself must never become a user-visible destination.
    pub fn commit_from(
        &self,
        root: &ConfinedRoot,
        staging: &File,
        mut record: CommitRecord,
        receipt: &[u8],
    ) -> Result<Vec<u8>> {
        if receipt.is_empty() || receipt.len() > 256 * 1024 {
            bail!("commit receipt is invalid");
        }
        let store = daemon_store(&self.home)?;
        if record.state == CommitState::ReceiptPersisted {
            return store
                .load_secret(&receipt_name(&record.job_id))?
                .context("committed receipt is missing");
        }
        if record.state == CommitState::Intent {
            rename_from_no_replace(
                root,
                staging,
                &record.staging_name,
                &record.destination_name,
            )?;
            root.root.sync_all()?;
            record.state = CommitState::Renamed;
            store.save_named(&record_name(&record.job_id), &record)?;
        }
        store.save_secret(&receipt_name(&record.job_id), receipt)?;
        record.state = CommitState::ReceiptPersisted;
        store.save_named(&record_name(&record.job_id), &record)?;
        Ok(receipt.to_vec())
    }

    /// Renames one staged root to a durable, receiver-chosen collision-free
    /// name. The resulting record is retained until the signed receipt is
    /// persisted, so reconnects cannot allocate a second suffixed copy.
    pub fn commit_from_allocated(
        &self,
        root: &ConfinedRoot,
        staging: &File,
        job_id: &str,
        grant: &DestinationGrant,
        source_name: &str,
    ) -> Result<CommitRecord> {
        validate_commit_name(job_id, "job")?;
        validate_top_level_name(source_name)?;
        let store = daemon_store(&self.home)?;
        let name = record_name(job_id);
        let mut record = match store.load_named::<CommitRecord>(&name)? {
            Some(record) => {
                if record.grant_id != grant.id
                    || record.grant_revision != grant.revision
                    || record.staging_name != source_name
                {
                    bail!("commit journal conflicts with an existing job");
                }
                record
            }
            None => CommitRecord {
                job_id: job_id.into(),
                grant_id: grant.id,
                grant_revision: grant.revision,
                staging_name: source_name.into(),
                destination_name: source_name.into(),
                destination_selected: false,
                staged_identity: None,
                state: CommitState::Intent,
            },
        };
        if record.state == CommitState::Intent {
            let staged_identity = match record.staged_identity.clone() {
                Some(identity) => identity,
                None => object_identity(staging, &record.staging_name)?
                    .context("staged root is missing")?,
            };
            if record.staged_identity.is_none() {
                record.staged_identity = Some(staged_identity.clone());
                store.save_named(&name, &record)?;
            }
            if !record.destination_selected {
                let mut suffix = 0u32;
                loop {
                    let candidate = collision_name(source_name, suffix)?;
                    if !destination_exists(root, &candidate)? {
                        record.destination_name = candidate;
                        record.destination_selected = true;
                        store.save_named(&name, &record)?;
                        break;
                    }
                    suffix = suffix
                        .checked_add(1)
                        .context("destination collision overflow")?;
                }
            }
            match object_identity(&root.root, &record.destination_name)? {
                Some(identity) if identity == staged_identity => {
                    record.state = CommitState::Renamed;
                    store.save_named(&name, &record)?;
                }
                Some(_) => bail!("committed destination was replaced"),
                None => {
                    if object_identity(staging, &record.staging_name)? != Some(staged_identity) {
                        bail!("staged root was replaced")
                    }
                    rename_from_no_replace(
                        root,
                        staging,
                        &record.staging_name,
                        &record.destination_name,
                    )?;
                    root.root.sync_all()?;
                    record.state = CommitState::Renamed;
                    store.save_named(&name, &record)?;
                }
            }
        }
        Ok(record)
    }

    /// Stores the exact signed receipt after all selected roots have durable
    /// destination mappings. A persisted receipt is always replayed unchanged.
    pub fn persist_commit_receipt(
        &self,
        mut record: CommitRecord,
        receipt: &[u8],
    ) -> Result<Vec<u8>> {
        if receipt.is_empty() || receipt.len() > 256 * 1024 {
            bail!("commit receipt is invalid");
        }
        let store = daemon_store(&self.home)?;
        if record.state == CommitState::ReceiptPersisted {
            return store
                .load_secret(&receipt_name(&record.job_id))?
                .context("committed receipt is missing");
        }
        if record.state != CommitState::Renamed {
            bail!("commit receipt was requested before rename");
        }
        store.save_secret(&receipt_name(&record.job_id), receipt)?;
        record.state = CommitState::ReceiptPersisted;
        store.save_named(&record_name(&record.job_id), &record)?;
        Ok(receipt.to_vec())
    }
}

impl ConfinedRoot {
    pub fn open(path: &Path, home: &Path) -> Result<Self> {
        let root = fs::canonicalize(path)
            .with_context(|| format!("resolve destination {}", path.display()))?;
        let profile = fs::canonicalize(home)?;
        let user_home = dirs::home_dir()
            .map(|path| fs::canonicalize(path).context("resolve user home"))
            .transpose()?;
        if unsafe_destination_root(&root, &profile, user_home.as_deref())? {
            bail!("destination must be a dedicated directory outside Filebeam state");
        }
        let metadata = fs::metadata(&root)?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o022 != 0
        {
            bail!("destination directory is not an owner-only directory");
        }
        let file = File::open(&root)?;
        Ok(Self {
            root: file,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    pub fn create_new(&self, relative: &Path) -> Result<File> {
        self.verify_identity()?;
        let components = checked_components(relative)?;
        let mut directory = self.root.try_clone()?;
        for component in &components[..components.len() - 1] {
            directory = open_directory(&directory, component)?;
        }
        let name = CString::new(
            components
                .last()
                .expect("nonempty validated component")
                .as_bytes(),
        )?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    pub(crate) fn create_directory(&self, name: &str) -> Result<File> {
        self.verify_identity()?;
        validate_top_level_name(name)?;
        let name = CString::new(name)?;
        if unsafe { libc::mkdirat(self.root.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        open_directory(&self.root, std::ffi::OsStr::new(name.to_str()?))
    }

    pub(crate) fn open_directory(&self, name: &str) -> Result<File> {
        self.verify_identity()?;
        validate_top_level_name(name)?;
        open_directory(&self.root, std::ffi::OsStr::new(name))
    }

    /// Opens or creates a receiver-selected relative destination under this
    /// grant using descriptor-relative, no-follow traversal.
    pub(crate) fn open_or_create_subdirectory(&self, components: &[String]) -> Result<Self> {
        self.verify_identity()?;
        let mut directory = self.root.try_clone()?;
        for component in components {
            validate_top_level_name(component)?;
            let name = CString::new(component.as_bytes())?;
            let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
            if result != 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(std::io::Error::last_os_error().into());
            }
            directory = open_directory(&directory, std::ffi::OsStr::new(component))?;
            let metadata = directory.metadata()?;
            if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
                bail!("destination subdirectory is not owner-only");
            }
        }
        let metadata = directory.metadata()?;
        Ok(Self {
            root: directory,
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    pub(crate) fn remove_directory(&self, name: &str) -> Result<()> {
        self.verify_identity()?;
        validate_top_level_name(name)?;
        let name = CString::new(name)?;
        if unsafe { libc::unlinkat(self.root.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub fn identity(&self) -> (u64, u64) {
        (self.device, self.inode)
    }

    fn verify_identity(&self) -> Result<()> {
        let metadata = self.root.metadata()?;
        if metadata.dev() != self.device || metadata.ino() != self.inode || !metadata.is_dir() {
            bail!("destination changed since it was granted");
        }
        Ok(())
    }
}

fn unsafe_destination_root(root: &Path, profile: &Path, user_home: Option<&Path>) -> Result<bool> {
    if root.starts_with(profile) || profile.starts_with(root) || root.parent().is_none() {
        return Ok(true);
    }
    let root = fs::metadata(root)?;
    Ok(user_home.is_some_and(|user_home| {
        fs::metadata(user_home)
            .map(|candidate| candidate.dev() == root.dev() && candidate.ino() == root.ino())
            .unwrap_or(true)
    }))
}

fn checked_components(path: &Path) -> Result<Vec<&std::ffi::OsStr>> {
    let components: Vec<_> = path
        .components()
        .map(|component| match component {
            Component::Normal(value) if !value.is_empty() => Ok(value),
            _ => bail!("destination path must be a non-empty relative path"),
        })
        .collect::<Result<_>>()?;
    if components.is_empty() {
        bail!("destination path must not be empty");
    }
    Ok(components)
}

fn validate_commit_name(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!("invalid commit {label}");
    }
    Ok(())
}

fn validate_top_level_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 255
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        bail!("commit paths must be one safe destination component");
    }
    Ok(())
}

fn collision_name(source: &str, suffix: u32) -> Result<String> {
    if suffix == 0 {
        return Ok(source.into());
    }
    let suffix = format!(" ({suffix})");
    let maximum = 255usize
        .checked_sub(suffix.len())
        .context("destination name is too long")?;
    let mut end = source.len().min(maximum);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        bail!("destination name is too long")
    }
    Ok(format!("{}{}", &source[..end], suffix))
}

fn destination_exists(root: &ConfinedRoot, name: &str) -> Result<bool> {
    root.verify_identity()?;
    let name = CString::new(name)?;
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::fstatat(
            root.root.as_raw_fd(),
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        return Ok(true);
    }
    match std::io::Error::last_os_error().kind() {
        std::io::ErrorKind::NotFound => Ok(false),
        _ => Err(std::io::Error::last_os_error().into()),
    }
}

fn object_identity(directory: &File, name: &str) -> Result<Option<CommitObjectIdentity>> {
    let name = CString::new(name)?;
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Ok(Some(CommitObjectIdentity {
            device: stat.st_dev,
            inode: stat.st_ino,
        }));
    }
    match std::io::Error::last_os_error().kind() {
        std::io::ErrorKind::NotFound => Ok(None),
        _ => Err(std::io::Error::last_os_error().into()),
    }
}

fn record_name(job_id: &str) -> String {
    format!("commit-{job_id}.json")
}
fn receipt_name(job_id: &str) -> String {
    format!("commit-receipt-{job_id}")
}

fn same_public_key(left: &str, right: &str) -> Result<bool> {
    let left = STANDARD
        .decode(left)
        .context("backend device public key is not base64")?;
    let right = STANDARD
        .decode(right)
        .context("provisioned device public key is not base64")?;
    Ok(left == right)
}

fn device_fingerprint(public_key: &str) -> Result<String> {
    Ok(hex::encode(Sha256::digest(STANDARD.decode(public_key)?)))
}

fn refresh_registry(
    home: &Path,
    provision: &DeviceProvision,
    snapshot: filebeam_client_core::services::DeviceRegistrySnapshot,
) -> Result<()> {
    use filebeam_client_core::device_registry::{
        AccountId, DeviceGeneration, DeviceRegistry, DeviceStatus, RegistryDevice,
        RegistryResponse, RegistryScope,
    };

    let filebeam_client_core::services::DeviceRegistrySnapshot {
        revision,
        generation,
        offline_freshness_seconds,
        devices: snapshot_devices,
    } = snapshot;
    let freshness =
        offline_freshness_seconds.context("device registry response omitted offline freshness")?;
    let scope = RegistryScope {
        origin: filebeam_client_core::device_registry::Origin::parse(provision.instance.clone())?,
        deployment: filebeam_client_core::device_registry::DeploymentId::parse(
            provision.deployment.clone(),
        )?,
        account: AccountId(provision.account_id),
        generation: DeviceGeneration(provision.generation),
        local_device: filebeam_client_core::device_registry::DeviceId::parse(
            provision.device_id.clone(),
        )?,
    };
    let devices = snapshot_devices
        .into_iter()
        .map(|device| {
            Ok(RegistryDevice {
                id: filebeam_client_core::device_registry::DeviceId::parse(device.id)?,
                key_version: device.key_version,
                // The backend uses padded standard base64; the registry's canonical
                // wire form is unpadded standard base64.
                public_key: device.public_key.trim_end_matches('=').into(),
                status: match device.status.as_str() {
                    "pending" => DeviceStatus::Pending,
                    "active" => DeviceStatus::Active,
                    "revoked" => DeviceStatus::Revoked,
                    _ => bail!("backend returned an invalid device status"),
                },
                generation: DeviceGeneration(device.generation),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);
    let store = registry_store(home)?;
    let mut registry = DeviceRegistry::open(&store, scope, Duration::from_secs(7 * 24 * 60 * 60))?;
    registry.ingest(
        RegistryResponse {
            revision,
            generation: DeviceGeneration(generation),
            offline_freshness_seconds: freshness,
            devices,
        },
        now,
    )
}

#[cfg(target_os = "linux")]
fn rename_no_replace(root: &ConfinedRoot, staging: &str, destination: &str) -> Result<()> {
    root.verify_identity()?;
    let staging = CString::new(staging)?;
    let destination = CString::new(destination)?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            root.root.as_raw_fd(),
            staging.as_ptr(),
            root.root.as_raw_fd(),
            destination.as_ptr(),
            1u32,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn rename_from_no_replace(
    root: &ConfinedRoot,
    staging: &File,
    source: &str,
    destination: &str,
) -> Result<()> {
    root.verify_identity()?;
    let source = CString::new(source)?;
    let destination = CString::new(destination)?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            staging.as_raw_fd(),
            source.as_ptr(),
            root.root.as_raw_fd(),
            destination.as_ptr(),
            1u32,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn rename_no_replace(root: &ConfinedRoot, staging: &str, destination: &str) -> Result<()> {
    root.verify_identity()?;
    renameatx_no_replace(&root.root, &root.root, staging, destination)
}

#[cfg(target_os = "macos")]
fn rename_from_no_replace(
    root: &ConfinedRoot,
    staging: &File,
    source: &str,
    destination: &str,
) -> Result<()> {
    root.verify_identity()?;
    renameatx_no_replace(staging, &root.root, source, destination)
}

#[cfg(target_os = "macos")]
fn renameatx_no_replace(
    source_directory: &File,
    destination_directory: &File,
    source: &str,
    destination: &str,
) -> Result<()> {
    unsafe extern "C" {
        fn renameatx_np(
            fromfd: libc::c_int,
            from: *const libc::c_char,
            tofd: libc::c_int,
            to: *const libc::c_char,
            flags: libc::c_uint,
        ) -> libc::c_int;
    }
    const RENAME_EXCL: libc::c_uint = 0x0000_0004;
    let source = CString::new(source)?;
    let destination = CString::new(destination)?;
    if unsafe {
        renameatx_np(
            source_directory.as_raw_fd(),
            source.as_ptr(),
            destination_directory.as_raw_fd(),
            destination.as_ptr(),
            RENAME_EXCL,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_no_replace(_: &ConfinedRoot, _: &str, _: &str) -> Result<()> {
    bail!("atomic no-replace commit is not implemented on this platform")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_from_no_replace(_: &ConfinedRoot, _: &File, _: &str, _: &str) -> Result<()> {
    bail!("atomic no-replace commit is not implemented on this platform")
}

fn open_directory(parent: &File, name: &std::ffi::OsStr) -> Result<File> {
    let name = CString::new(name.as_bytes())?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn secure_home(home: &Path) -> Result<PathBuf> {
    fs::create_dir_all(home)?;
    let home = fs::canonicalize(home)?;
    let metadata = fs::metadata(&home)?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        bail!("daemon profile must be owned by the current user and mode 0700");
    }
    Ok(home)
}

pub(crate) fn daemon_store(home: &Path) -> Result<filebeam_transfer_native::checkpoint::Store> {
    use filebeam_transfer_native::checkpoint::Store;

    let root = home.join("transfers");
    match Store::open(&root, "daemon-private-v1") {
        Ok(store) => Ok(store),
        Err(error) if error.to_string().contains("does not exist") => {
            Store::create(&root, "daemon-private-v1")
        }
        Err(error) => Err(error).context("open daemon private checkpoint"),
    }
}

pub(crate) fn registry_store(home: &Path) -> Result<filebeam_transfer_native::checkpoint::Store> {
    use filebeam_transfer_native::checkpoint::Store;

    let root = home.join("registry");
    match Store::open(&root, "device-registry-v1") {
        Ok(store) => Ok(store),
        Err(error) if error.to_string().contains("does not exist") => {
            Store::create(&root, "device-registry-v1")
        }
        Err(error) => Err(error).context("open device registry checkpoint"),
    }
}

fn ipc_key(home: &Path) -> Result<[u8; 32]> {
    let store = ipc_store(home)?;
    match store.load_secret("ipc-authentication-key-v1")? {
        Some(key) => key
            .try_into()
            .map_err(|_| anyhow::anyhow!("daemon IPC key has invalid length")),
        None => {
            let key = random_nonce()?;
            store.save_secret("ipc-authentication-key-v1", &key)?;
            Ok(key)
        }
    }
}

fn ipc_store(home: &Path) -> Result<filebeam_transfer_native::checkpoint::Store> {
    use filebeam_transfer_native::checkpoint::Store;

    let root = home.join("ipc");
    match Store::open(&root, "ipc-authentication-v1") {
        Ok(store) => Ok(store),
        Err(error) if error.to_string().contains("does not exist") => {
            Store::create(&root, "ipc-authentication-v1")
        }
        Err(error) => Err(error).context("open IPC authentication checkpoint"),
    }
}

fn random_nonce() -> Result<[u8; 32]> {
    let mut nonce = [0u8; 32];
    getrandom::getrandom(&mut nonce).context("generate IPC challenge")?;
    Ok(nonce)
}

fn proof(key: &[u8; 32], domain: &[u8], fields: &[&[u8]]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts a fixed-size key");
    mac.update(domain);
    for field in fields {
        mac.update(&(field.len() as u64).to_be_bytes());
        mac.update(field);
    }
    mac.finalize().into_bytes().into()
}

fn write_message<T: Serialize>(
    stream: &mut std::os::unix::net::UnixStream,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 > MAX_MESSAGE_BYTES {
        bail!("IPC message exceeds 1 MiB");
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    Ok(())
}

fn read_message<T: for<'de> Deserialize<'de>>(
    stream: &mut std::os::unix::net::UnixStream,
) -> Result<T> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as u64;
    if length > MAX_MESSAGE_BYTES {
        bail!("IPC message exceeds 1 MiB");
    }
    let mut bytes = vec![0; length as usize];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).context("decode IPC message")
}

pub(crate) fn secure_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        bail!("daemon runtime directory is unsafe");
    }
    Ok(())
}

fn remove_stale_socket(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_socket() && metadata.uid() == unsafe { libc::geteuid() } =>
        {
            fs::remove_file(path).map_err(Into::into)
        }
        Ok(_) => bail!("refusing to replace unsafe daemon endpoint"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(target_os = "linux")]
fn same_user(stream: &std::os::unix::net::UnixStream) -> Result<bool> {
    let mut credential: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credential as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 || length as usize != std::mem::size_of::<libc::ucred>() {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(credential.uid == unsafe { libc::geteuid() })
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "openbsd"))]
fn same_user(stream: &std::os::unix::net::UnixStream) -> Result<bool> {
    let mut uid = 0;
    let mut gid = 0;
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(uid == unsafe { libc::geteuid() })
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
fn same_user(_: &std::os::unix::net::UnixStream) -> Result<bool> {
    bail!("peer credential checks are not implemented on this Unix platform")
}

fn read_state(path: &Path) -> Result<DaemonState> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("read daemon state"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DaemonState::default()),
        Err(error) => Err(error.into()),
    }
}

fn write_state(path: &Path, state: &DaemonState) -> Result<()> {
    write_json(path, state)
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("daemon state has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec(value)?)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn provision_requires_matching_key_and_redacts_secrets() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let provision = DeviceProvision {
            instance: "https://example.test".into(),
            deployment: "test".into(),
            account_id: 1,
            generation: 1,
            device_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            key_version: 1,
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            signing_key: STANDARD.encode(signing.to_bytes()),
            credential: "a".repeat(64),
        };
        provision.validate().unwrap();
        let debug = format!("{provision:?}");
        assert!(!debug.contains(&provision.credential));
        assert!(!debug.contains(&provision.signing_key));
    }

    #[test]
    fn confinement_rejects_traversal_and_existing_files() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let dropzone = destination.path().join("dropzone");
        fs::create_dir(&dropzone).unwrap();
        fs::set_permissions(&dropzone, fs::Permissions::from_mode(0o700)).unwrap();
        let root = ConfinedRoot::open(&dropzone, home.path()).unwrap();
        assert!(root.create_new(Path::new("../outside")).is_err());
        root.create_new(Path::new("new-file")).unwrap();
        assert!(root.create_new(Path::new("new-file")).is_err());
    }

    #[test]
    fn daemon_rejects_a_second_owner() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let first = Daemon::open(home.path()).unwrap();
        assert!(Daemon::open(home.path()).is_err());
        drop(first);
    }

    #[test]
    fn grant_identity_and_commit_receipt_are_durable_and_idempotent() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root_path = destination.path().join("dropzone");
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut grants = DestinationGrants::open(home.path()).unwrap();
        let grant = grants.add("work", &root_path, ApprovalMode::Ask).unwrap();
        let root = grants.open_grant(grant.id, grant.revision).unwrap();
        let mut staging = root.create_new(Path::new(".filebeam-stage-job1")).unwrap();
        staging.write_all(b"verified bytes").unwrap();
        staging.sync_all().unwrap();
        let journal = CommitJournal::new(home.path());
        let record = journal
            .begin("job1", &grant, ".filebeam-stage-job1", "report.pdf")
            .unwrap();
        assert_eq!(
            journal.commit(&root, record, b"signed receipt").unwrap(),
            b"signed receipt"
        );
        let replay = journal
            .begin("job1", &grant, ".filebeam-stage-job1", "report.pdf")
            .unwrap();
        assert_eq!(
            journal
                .commit(&root, replay, b"different receipt is ignored")
                .unwrap(),
            b"signed receipt"
        );
        assert_eq!(
            fs::read(root_path.join("report.pdf")).unwrap(),
            b"verified bytes"
        );
        grants.revoke("work").unwrap();
        assert!(grants.open_grant(grant.id, grant.revision).is_err());
    }

    #[test]
    fn commit_never_replaces_an_existing_destination() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root_path = destination.path().join("dropzone");
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root_path.join("report.pdf"), b"existing").unwrap();
        let mut grants = DestinationGrants::open(home.path()).unwrap();
        let grant = grants.add("work", &root_path, ApprovalMode::Ask).unwrap();
        let root = grants.open_grant(grant.id, grant.revision).unwrap();
        root.create_new(Path::new(".filebeam-stage-job2")).unwrap();
        let record = CommitJournal::new(home.path())
            .begin("job2", &grant, ".filebeam-stage-job2", "report.pdf")
            .unwrap();
        assert!(
            CommitJournal::new(home.path())
                .commit(&root, record, b"receipt")
                .is_err()
        );
        assert_eq!(fs::read(root_path.join("report.pdf")).unwrap(), b"existing");
    }

    #[test]
    fn peer_commit_publishes_selected_root_not_job_staging_directory() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root_path = destination.path().join("dropzone");
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut grants = DestinationGrants::open(home.path()).unwrap();
        let grant = grants.add("work", &root_path, ApprovalMode::Ask).unwrap();
        let root = grants.open_grant(grant.id, grant.revision).unwrap();
        let stage = root.create_directory(".filebeam-stage-job3").unwrap();
        let mut report = {
            let name = CString::new("report.pdf").unwrap();
            let fd = unsafe {
                libc::openat(
                    stage.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                    0o600,
                )
            };
            assert!(fd >= 0);
            unsafe { File::from_raw_fd(fd) }
        };
        report.write_all(b"verified bytes").unwrap();
        report.sync_all().unwrap();
        let journal = CommitJournal::new(home.path());
        let record = journal
            .begin("job3-0", &grant, "report.pdf", "report.pdf")
            .unwrap();
        journal
            .commit_from(&root, &stage, record, b"signed receipt")
            .unwrap();
        drop(report);
        drop(stage);
        root.remove_directory(".filebeam-stage-job3").unwrap();
        assert_eq!(
            fs::read(root_path.join("report.pdf")).unwrap(),
            b"verified bytes"
        );
        assert!(!root_path.join("received-job3").exists());
    }

    #[test]
    fn recovery_after_each_root_rename_advances_the_exact_durable_intent() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root_path = destination.path().join("dropzone");
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut grants = DestinationGrants::open(home.path()).unwrap();
        let grant = grants.add("work", &root_path, ApprovalMode::Ask).unwrap();
        let root = grants.open_grant(grant.id, grant.revision).unwrap();
        let stage = root.create_directory(".filebeam-stage-recovery").unwrap();

        for (index, source) in ["first", "second"].iter().enumerate() {
            let mut file = unsafe {
                let name = CString::new(*source).unwrap();
                let fd = libc::openat(
                    stage.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                    0o600,
                );
                assert!(fd >= 0);
                File::from_raw_fd(fd)
            };
            file.write_all(source.as_bytes()).unwrap();
            file.sync_all().unwrap();
            let job_id = format!("recovery-{index}");
            let record = CommitRecord {
                job_id: job_id.clone(),
                grant_id: grant.id,
                grant_revision: grant.revision,
                staging_name: source.to_string(),
                destination_name: format!("{source}-published"),
                destination_selected: true,
                staged_identity: object_identity(&stage, source).unwrap(),
                state: CommitState::Intent,
            };
            daemon_store(home.path())
                .unwrap()
                .save_named(&record_name(&job_id), &record)
                .unwrap();
            // Fault injection boundary: the rename reached disk but journal advancement did not.
            rename_from_no_replace(&root, &stage, source, &record.destination_name).unwrap();

            let recovered = CommitJournal::new(home.path())
                .commit_from_allocated(&root, &stage, &job_id, &grant, source)
                .unwrap();
            assert_eq!(recovered.state, CommitState::Renamed);
            assert_eq!(recovered.destination_name, format!("{source}-published"));
            assert_eq!(
                fs::read(root_path.join(&recovered.destination_name)).unwrap(),
                source.as_bytes()
            );
            assert!(!root_path.join(format!("{source}-published (1)")).exists());
        }
    }

    #[test]
    fn replaced_grant_root_is_not_reopened_by_path() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root_path = destination.path().join("dropzone");
        let old_path = destination.path().join("old-dropzone");
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o700)).unwrap();
        let mut grants = DestinationGrants::open(home.path()).unwrap();
        let grant = grants.add("work", &root_path, ApprovalMode::Ask).unwrap();
        fs::rename(&root_path, &old_path).unwrap();
        fs::create_dir(&root_path).unwrap();
        fs::set_permissions(&root_path, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(grants.open_grant(grant.id, grant.revision).is_err());
    }

    #[test]
    fn home_and_profile_ancestors_are_not_destination_roots() {
        let home = tempfile::tempdir().unwrap();
        let profile = home.path().join(".config/filebeam");
        fs::create_dir_all(&profile).unwrap();
        let dedicated = tempfile::tempdir().unwrap();
        assert!(unsafe_destination_root(home.path(), &profile, Some(home.path())).unwrap());
        assert!(
            unsafe_destination_root(home.path().join(".config").as_path(), &profile, None).unwrap()
        );
        assert!(!unsafe_destination_root(dedicated.path(), &profile, Some(home.path())).unwrap());
    }

    #[test]
    fn revoked_grant_stays_disabled_after_registry_reopen() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let root = destination.path().join("dropzone");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let mut grants = DestinationGrants::open(home.path()).unwrap();
        let grant = grants.add("work", &root, ApprovalMode::Ask).unwrap();
        grants.revoke("work").unwrap();
        drop(grants);
        assert!(
            DestinationGrants::open(home.path())
                .unwrap()
                .open_grant(grant.id, grant.revision)
                .is_err()
        );
    }

    #[test]
    fn owner_authenticated_ipc_persists_receive_policy_and_stops() {
        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = home.path().to_owned();
        let owner = Daemon::open(&path).unwrap();
        let server = thread::spawn(move || owner.run());
        for _ in 0..20 {
            if socket_path(&path).exists() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !request(
                &path,
                Request::Status {
                    version: IPC_VERSION
                }
            )
            .unwrap()
            .state
            .receive_enabled
        );
        assert!(
            request(
                &path,
                Request::SetReceiveEnabled {
                    version: IPC_VERSION,
                    enabled: true,
                },
            )
            .unwrap()
            .state
            .receive_enabled
        );
        request(
            &path,
            Request::Shutdown {
                version: IPC_VERSION,
            },
        )
        .unwrap();
        server.join().unwrap().unwrap();
    }

    #[test]
    fn disconnected_ipc_handshake_keeps_the_owner_running() {
        use std::os::unix::net::UnixStream;

        let home = tempfile::tempdir().unwrap();
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = home.path().to_owned();
        let server = thread::spawn({
            let path = path.clone();
            move || Daemon::open(&path).unwrap().run()
        });
        for _ in 0..20 {
            if socket_path(&path).exists() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        drop(UnixStream::connect(socket_path(&path)).unwrap());
        thread::sleep(Duration::from_millis(20));
        assert!(
            request(
                &path,
                Request::Status {
                    version: IPC_VERSION
                }
            )
            .is_ok()
        );
        request(
            &path,
            Request::Shutdown {
                version: IPC_VERSION,
            },
        )
        .unwrap();
        server.join().unwrap().unwrap();

        // Joining all background owners releases the profile lock before a new
        // active daemon can acquire it.
        let reopened = Daemon::open(&path).unwrap();
        drop(reopened);
    }
}
