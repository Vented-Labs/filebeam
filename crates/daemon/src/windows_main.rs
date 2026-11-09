use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{
    ControlPlane, DaemonState, DestinationGrants, DeviceJobs, Reply, Request, windows::Pipe,
};

type HmacSha256 = Hmac<Sha256>;
pub const IPC_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct ServerHello {
    version: u32,
    nonce: [u8; 32],
    proof: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct ClientHello {
    version: u32,
    nonce: [u8; 32],
    proof: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct AuthenticatedPayload {
    payload: Vec<u8>,
    proof: [u8; 32],
}

pub fn run() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let home = match (arguments.next(), arguments.next()) {
        (Some(flag), Some(path)) if flag == "--home" => PathBuf::from(path),
        _ => bail!("usage: filebeamd --home PROFILE_HOME"),
    };
    WindowsDaemon::open(&home)?.run()
}

pub struct WindowsDaemon {
    home: PathBuf,
    state: DaemonState,
    ipc_key: [u8; 32],
    jobs: std::sync::Arc<std::sync::Mutex<DeviceJobs>>,
    runtime: filebeam_client_core::DeviceRuntime,
    active_jobs:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, crate::ActiveJob>>>,
    grants: DestinationGrants,
    receive_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    dispatcher: Option<crate::dispatcher::Dispatcher>,
    // Pins the profile and its validated ancestors until every secret-bearing
    // subsystem and the daemon named pipe have stopped.
    _profile_guard: filebeam_transfer_native::checkpoint::PrivateProfileGuard,
    _lock: File,
}

impl WindowsDaemon {
    pub fn open(home: &Path) -> Result<Self> {
        let profile_guard = filebeam_transfer_native::checkpoint::create_private_profile(home)
            .context("secure daemon profile")?;
        let home = fs::canonicalize(home)?;
        let runtime = home.join("runtime").join("daemon");
        // This state is written outside checkpoint Store, so give its parent
        // and leaf explicit protected ACLs before reading or publishing state.
        filebeam_transfer_native::checkpoint::create_private_profile(&runtime)
            .context("secure daemon runtime state")?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(runtime.join("owner.lock"))?;
        lock.try_lock_exclusive()
            .context("another Filebeam daemon already owns this profile")?;
        let state = crate::read_state(&runtime.join("state.json"))?;
        let receive_enabled =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(state.receive_enabled));
        Ok(Self {
            ipc_key: ipc_key(&home)?,
            state,
            jobs: std::sync::Arc::new(std::sync::Mutex::new(DeviceJobs::open(&home)?)),
            runtime: crate::dispatcher::runtime(&home)?,
            active_jobs: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            grants: DestinationGrants::open(&home)?,
            receive_enabled,
            dispatcher: None,
            home,
            _profile_guard: profile_guard,
            _lock: lock,
        })
    }

    pub fn run(mut self) -> Result<()> {
        let jobs = self.jobs.clone();
        self.dispatcher = Some(crate::dispatcher::start(
            self.home.clone(),
            self.runtime.clone(),
            jobs.clone(),
            self.active_jobs.clone(),
            None,
            self.receive_enabled.clone(),
        ));
        let name = crate::windows::pipe_name(&self.home)?;
        let mut shutdown = false;
        while !shutdown {
            let mut pipe = Pipe::create_server(&name)?;
            if pipe.accept().is_err() {
                continue;
            }
            let server_nonce = random_nonce()?;
            if write_message(
                &mut pipe,
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
            let client: ClientHello = match read_message(&mut pipe) {
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
            let request: AuthenticatedPayload = match read_message(&mut pipe) {
                Ok(request) => request,
                Err(_) => continue,
            };
            let reply = if request.proof
                != proof(
                    &self.ipc_key,
                    b"filebeam-daemon-request-v1",
                    &[&server_nonce, &client.nonce, &request.payload],
                ) {
                self.reply(Some("IPC request authentication failed".into()))
            } else {
                match serde_json::from_slice::<Request>(&request.payload) {
                    Ok(Request::Status { version }) => self.reply_for_version(version),
                    Ok(Request::SetReceiveEnabled { version, enabled }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            self.state.receive_enabled = enabled;
                            self.receive_enabled
                                .store(enabled, std::sync::atomic::Ordering::Release);
                            if let Err(error) = crate::write_state(
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
                            if let Err(error) = crate::write_state(
                                &self.home.join("runtime/daemon/state.json"),
                                &self.state,
                            ) {
                                reply.error = Some(error.to_string());
                            }
                            reply.state = self.state.clone();
                        }
                        reply
                    }
                    Ok(Request::ListJobs { version }) => self.with_version(version, |reply| {
                        reply.jobs = jobs
                            .lock()
                            .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                            .list()
                            .to_vec();
                        Ok(())
                    }),
                    Ok(Request::EnqueueJob {
                        version,
                        target,
                        grant_id,
                        sources,
                    }) => self.with_version(version, |reply| {
                        reply.jobs = vec![
                            jobs.lock()
                                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                                .enqueue(&target, &grant_id, sources)?,
                        ];
                        Ok(())
                    }),
                    Ok(Request::ControlJob {
                        version,
                        id,
                        action,
                    }) => self.with_version(version, |reply| {
                        crate::dispatcher::control(&self.runtime, &self.active_jobs, id, &action)?;
                        reply.jobs = vec![
                            jobs.lock()
                                .map_err(|_| anyhow::anyhow!("device jobs state is poisoned"))?
                                .control(id, action)?,
                        ];
                        Ok(())
                    }),
                    Ok(Request::ProvisionDevice { version, provision }) => {
                        let mut reply = self.reply_for_version(version);
                        if reply.error.is_none() {
                            if let Err(error) = ControlPlane::new(&self.home, None)
                                .provision(&mut self.state, provision)
                            {
                                reply.error = Some(error.to_string());
                            }
                            reply.state = self.state.clone();
                        }
                        reply
                    }
                    Ok(Request::ListDevices { version }) => self.with_version(version, |reply| {
                        reply.devices = ControlPlane::new(&self.home, None).list_devices()?;
                        Ok(())
                    }),
                    Ok(Request::ConfirmPeerVerification {
                        version,
                        device_id,
                        key_version,
                        fingerprint,
                        confirmation_nonce,
                    }) => self.with_version(version, |_| {
                        ControlPlane::new(&self.home, None).confirm_peer(
                            &device_id,
                            key_version,
                            &fingerprint,
                            &confirmation_nonce,
                        )
                    }),
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
                    Ok(Request::Shutdown { version }) => {
                        let reply = self.reply_for_version(version);
                        shutdown = reply.error.is_none();
                        if shutdown {
                            if let Err(error) = self.stop_background() {
                                shutdown = false;
                                let mut reply = reply;
                                reply.error = Some(error.to_string());
                                reply
                            } else {
                                let _ = self.runtime.shutdown(std::time::Duration::from_secs(10));
                                reply
                            }
                        } else {
                            reply
                        }
                    }
                    Ok(_) | Err(_) => {
                        self.reply(Some("Windows daemon operation is not implemented".into()))
                    }
                }
            };
            let payload = serde_json::to_vec(&reply)?;
            if write_message(
                &mut pipe,
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
        let _ = self.runtime.shutdown(std::time::Duration::from_secs(10));
        Ok(())
    }

    fn reply_for_version(&self, version: u32) -> Reply {
        self.reply(
            (version != IPC_VERSION)
                .then(|| "IPC version mismatch; upgrade the client or daemon".into()),
        )
    }

    fn reply(&self, error: Option<String>) -> Reply {
        Reply {
            version: IPC_VERSION,
            state: self.state.clone(),
            error,
            destinations: Vec::new(),
            jobs: Vec::new(),
            devices: Vec::new(),
        }
    }

    fn with_version(
        &self,
        version: u32,
        operation: impl FnOnce(&mut Reply) -> Result<()>,
    ) -> Reply {
        let mut reply = self.reply_for_version(version);
        if reply.error.is_none() {
            if let Err(error) = operation(&mut reply) {
                reply.error = Some(error.to_string());
            }
        }
        reply
    }

    fn stop_background(&mut self) -> Result<()> {
        if let Some(dispatcher) = self.dispatcher.as_mut() {
            dispatcher.shutdown(std::time::Duration::from_secs(60))?;
        }
        self.dispatcher = None;
        Ok(())
    }
}

fn ipc_key(home: &Path) -> Result<[u8; 32]> {
    use filebeam_transfer_native::checkpoint::Store;

    let root = home.join("runtime");
    let store = match Store::open(&root, "daemon-ipc-v1") {
        Ok(store) => store,
        Err(error) if error.to_string().contains("does not exist") => {
            Store::create(&root, "daemon-ipc-v1")?
        }
        Err(error) => return Err(error).context("open daemon IPC checkpoint"),
    };
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

fn random_nonce() -> Result<[u8; 32]> {
    let mut nonce = [0; 32];
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

fn write_message<T: Serialize, S: Write>(stream: &mut S, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 > MAX_MESSAGE_BYTES {
        bail!("IPC message exceeds 1 MiB");
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    Ok(())
}

fn read_message<T: for<'de> Deserialize<'de>, S: Read>(stream: &mut S) -> Result<T> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as u64;
    if length > MAX_MESSAGE_BYTES {
        bail!("IPC message exceeds 1 MiB");
    }
    let mut bytes = vec![0; length as usize];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).context("decode IPC message")
}

/// Sends an authenticated request over the owner-only profile named pipe.
pub fn request(home: &Path, request: Request) -> Result<Reply> {
    // Keep the same validated path guard through pipe discovery and the full
    // authenticated exchange so an attacker cannot swap in another profile.
    let _profile_guard = filebeam_transfer_native::checkpoint::guard_private_profile(home)
        .context("secure daemon profile")?;
    let name = crate::windows::pipe_name(home)?;
    let mut pipe = Pipe::connect_client(&name)?;
    let key = ipc_key(home)?;
    let server: ServerHello = read_message(&mut pipe)?;
    if server.version != IPC_VERSION
        || server.proof != proof(&key, b"filebeam-daemon-server-v1", &[&server.nonce])
    {
        bail!("daemon identity authentication failed");
    }
    let client_nonce = random_nonce()?;
    write_message(
        &mut pipe,
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
        &mut pipe,
        &AuthenticatedPayload {
            proof: proof(
                &key,
                b"filebeam-daemon-request-v1",
                &[&server.nonce, &client_nonce, &payload],
            ),
            payload,
        },
    )?;
    let response: AuthenticatedPayload = read_message(&mut pipe)?;
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
