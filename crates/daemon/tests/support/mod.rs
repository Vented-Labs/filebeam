use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use filebeam_client_core::device_registry::{
    AccountId, DeploymentId, DeviceGeneration, DeviceId, DeviceRegistry, Origin, RegistryScope,
};
use filebeam_transfer_native::checkpoint::Store;
use rcgen::generate_simple_self_signed;
use rustls::{
    ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Identity {
    pub device_id: String,
    pub signing_key: String,
    pub credential: String,
}

pub struct Coordinator {
    pub instance: String,
    pub deployment: String,
    pub account_id: u64,
    pub sender: Identity,
    pub receiver: Identity,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    certificate_pem: String,
}

impl Coordinator {
    pub fn start() -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let certificate = generate_simple_self_signed(vec!["localhost".into()])?;
        let certificate_pem = certificate.cert.pem();
        let config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(certificate.cert.der().clone())],
                    PrivateKeyDer::Pkcs8(certificate.signing_key.serialize_der().into()),
                )?,
        );
        let sender = identity([1; 32], "01ARZ3NDEKTSV4RRFFQ69G5FAV", 'a');
        let receiver = identity([2; 32], "01ARZ3NDEKTSV4RRFFQ69G5FAW", 'b');
        let state = Arc::new(Mutex::new(State {
            sender: sender.clone(),
            receiver: receiver.clone(),
            connection: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let thread = thread::spawn(move || {
            while !server_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = serve(stream, config.clone(), &state);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => return,
                }
            }
        });
        Ok(Self {
            instance: format!("https://localhost:{port}"),
            deployment: "d2d-test".into(),
            account_id: 1,
            sender,
            receiver,
            stop,
            thread: Some(thread),
            certificate_pem,
        })
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn certificate_pem(&self) -> &str {
        &self.certificate_pem
    }
}

#[derive(Serialize, Deserialize)]
struct ChildConfig {
    home: PathBuf,
    certificate_pem: String,
}

pub struct ChildGuard {
    child: Option<Child>,
    _config: tempfile::NamedTempFile,
}

impl ChildGuard {
    pub fn wait(mut self) -> Result<ExitStatus> {
        Ok(self.child.take().expect("child is present").wait()?)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn spawn_daemon_child(home: &Path, certificate_pem: &str) -> Result<ChildGuard> {
    let config = tempfile::NamedTempFile::new()?;
    fs::set_permissions(config.path(), fs::Permissions::from_mode(0o600))?;
    fs::write(
        config.path(),
        serde_json::to_vec(&ChildConfig {
            home: home.into(),
            certificate_pem: certificate_pem.into(),
        })?,
    )?;
    let child = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("daemon_child_owner")
        .arg("--nocapture")
        .env("FILEBEAM_D2D_CHILD_CONFIG", config.path())
        .spawn()?;
    Ok(ChildGuard {
        child: Some(child),
        _config: config,
    })
}

pub fn run_daemon_child() -> Result<()> {
    let path = std::env::var_os("FILEBEAM_D2D_CHILD_CONFIG").context("missing child config")?;
    let config: ChildConfig = serde_json::from_slice(&fs::read(path)?)?;
    let certificate = reqwest::Certificate::from_pem(config.certificate_pem.as_bytes())?;
    filebeam_daemon::Daemon::open_with_trusted_certificate(&config.home, certificate)?.run()
}

#[derive(Clone)]
struct Connection {
    offer: Option<Value>,
    answer: Option<Value>,
}
struct State {
    sender: Identity,
    receiver: Identity,
    connection: Option<Connection>,
}

fn identity(seed: [u8; 32], device_id: &str, credential: char) -> Identity {
    Identity {
        device_id: device_id.into(),
        signing_key: STANDARD.encode(SigningKey::from_bytes(&seed).to_bytes()),
        credential: credential.to_string().repeat(64),
    }
}

pub fn provision(
    coordinator: &Coordinator,
    identity: &Identity,
) -> Result<filebeam_daemon::DeviceProvision> {
    let signing: [u8; 32] = STANDARD
        .decode(&identity.signing_key)
        .context("decode test device signing key")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("test device signing key has an invalid length"))?;
    let signing = SigningKey::from_bytes(&signing);
    Ok(filebeam_daemon::DeviceProvision {
        instance: coordinator.instance.clone(),
        deployment: coordinator.deployment.clone(),
        account_id: coordinator.account_id,
        generation: 1,
        device_id: identity.device_id.clone(),
        key_version: 1,
        public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
        signing_key: identity.signing_key.clone(),
        credential: identity.credential.clone(),
    })
}

/// This fixture writes only the registry pin that a completed pairing ceremony
/// would persist. Transport authentication remains entirely production code.
pub fn pin_peer(
    home: &Path,
    coordinator: &Coordinator,
    local: &Identity,
    peer: &Identity,
) -> Result<()> {
    let store = Store::open(&home.join("registry"), "device-registry-v1")?;
    let scope = RegistryScope {
        origin: Origin::parse(coordinator.instance.clone())?,
        deployment: DeploymentId::parse(coordinator.deployment.clone())?,
        account: AccountId(coordinator.account_id),
        generation: DeviceGeneration(1),
        local_device: DeviceId::parse(local.device_id.clone())?,
    };
    let mut registry = DeviceRegistry::open(&store, scope, Duration::from_secs(60))?;
    registry.approve_pairing(&DeviceId::parse(peer.device_id.clone())?, unix_now())
}

pub fn make_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub fn wait_for_socket(home: &Path) -> Result<()> {
    wait_until("daemon socket", Duration::from_secs(5), || {
        filebeam_daemon::socket_path(home).exists()
    })
}

pub fn wait_until(
    description: &str,
    timeout: Duration,
    condition: impl Fn() -> bool,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("timed out waiting for {description}")
}

pub fn shutdown(home: &Path) {
    let _ = filebeam_daemon::request(
        home,
        filebeam_daemon::Request::Shutdown {
            version: filebeam_daemon::IPC_VERSION,
        },
    );
}

pub fn profile(root: &Path, name: &str) -> Result<PathBuf> {
    let path = root.join(name);
    make_private_directory(&path)?;
    Ok(path)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs().try_into().unwrap_or(i64::MAX))
        .unwrap_or(i64::MAX)
}

fn serve(stream: TcpStream, config: Arc<ServerConfig>, state: &Arc<Mutex<State>>) -> Result<()> {
    let connection = ServerConnection::new(config)?;
    let mut stream = StreamOwned::new(connection, stream);
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            bail!("incomplete coordinator request");
        }
        request.extend_from_slice(&buffer[..count]);
        let Some(headers_end) = request.windows(4).position(|part| part == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length: ")
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + length {
            break;
        }
    }
    let headers_end = request
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .expect("complete headers");
    let headers = String::from_utf8_lossy(&request[..headers_end]);
    let mut parts = headers
        .lines()
        .next()
        .context("missing request line")?
        .split_whitespace();
    let method = parts.next().context("missing method")?;
    let path = parts.next().context("missing path")?;
    let credential = headers.lines().find_map(|line| {
        line.split_once(':').and_then(|(name, value)| {
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim().strip_prefix("Bearer "))
                .flatten()
        })
    });
    let body = &request[headers_end + 4..];
    let response = route(method, path, credential, body, state)?;
    let status = if method == "POST" && path == "/api/native/v1/devices/connections" {
        "201 Created"
    } else if method == "PUT" && (path.ends_with("/offer") || path.ends_with("/answer")) {
        "204 No Content"
    } else {
        "200 OK"
    };
    stream.write_all(
        format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nConnection: close\r\n")
            .as_bytes(),
    )?;
    stream.write_all(format!("Content-Length: {}\r\n\r\n", response.len()).as_bytes())?;
    stream.write_all(&response)?;
    Ok(())
}

fn route(
    method: &str,
    path: &str,
    credential: Option<&str>,
    body: &[u8],
    state: &Arc<Mutex<State>>,
) -> Result<Vec<u8>> {
    let mut state = state
        .lock()
        .map_err(|_| anyhow::anyhow!("coordinator state poisoned"))?;
    let sender = credential == Some(state.sender.credential.as_str());
    let receiver = credential == Some(state.receiver.credential.as_str());
    if !sender && !receiver {
        bail!("coordinator rejected device credential");
    }
    let device = |identity: &Identity| -> Result<Value> {
        Ok(
            json!({"id": identity.device_id, "display_name":"d2d", "platform":"test", "public_key": public_key(identity)?, "key_version":1, "status":"active", "generation":1, "last_seen_at":null, "presence_expires_at":null, "revoked_at":null}),
        )
    };
    if method == "GET" && path == "/api/native/v1/devices/sync" {
        return Ok(serde_json::to_vec(
            &json!({"data":{"revision":1,"generation":1,"offline_freshness_seconds":60,"devices":[device(&state.sender)?,device(&state.receiver)?]}}),
        )?);
    }
    if method == "POST" && path == "/api/native/v1/devices/connections" && sender {
        state.connection = Some(Connection {
            offer: None,
            answer: None,
        });
        return connection(&state);
    }
    if method == "GET" && path == "/api/native/v1/devices/connections" && receiver {
        let connections = state
            .connection
            .as_ref()
            .map(|_| vec![connection_value(&state)])
            .unwrap_or_default();
        return Ok(serde_json::to_vec(
            &json!({"data":{"connections":connections,"poll_after_seconds":1}}),
        )?);
    }
    if method == "GET" && path.ends_with("/ice") {
        return Ok(serde_json::to_vec(&json!({"data":{"ice_servers":[]}}))?);
    }
    if path.contains("/connections/") {
        if method == "PUT" && path.ends_with("/offer") && sender {
            state
                .connection
                .as_mut()
                .context("missing connection")?
                .offer = Some(serde_json::from_slice::<Value>(body)?["description"].clone());
            return Ok(Vec::new());
        }
        if method == "PUT" && path.ends_with("/answer") && receiver {
            state
                .connection
                .as_mut()
                .context("missing connection")?
                .answer = Some(serde_json::from_slice::<Value>(body)?["description"].clone());
            return Ok(Vec::new());
        }
        if method == "GET" {
            return connection(&state);
        }
    }
    bail!("unexpected coordinator request {method} {path}")
}

fn public_key(identity: &Identity) -> Result<String> {
    let bytes: [u8; 32] = STANDARD
        .decode(&identity.signing_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid fixture signing key"))?;
    Ok(STANDARD.encode(SigningKey::from_bytes(&bytes).verifying_key().to_bytes()))
}

fn connection(state: &State) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(
        &json!({"data":{"connection":connection_value(state)}}),
    )?)
}
fn connection_value(state: &State) -> Value {
    let connection = state.connection.as_ref().expect("connection exists");
    json!({"id":"d2d-connection","source_device_id":state.sender.device_id,"target_device_id":state.receiver.device_id,"offer":connection.offer,"answer":connection.answer,"phase":"waiting","expires_at":"2030-01-01T00:00:00Z"})
}
