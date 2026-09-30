use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use sha2::{Digest, Sha256};
use url::Url;

const MAX_PAYLOAD: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// `filebeam://transfer/<base64url-http-origin>/<transfer-id>#<share-key>` is the desktop
/// protocol form. The encoded origin is explicit so it cannot silently use the default instance.
pub enum Ingress {
    Path(PathBuf),
    Link {
        link: filebeam_transfer_native::protocol::Link,
    },
}

pub fn parse(value: &str, origin: &Url) -> Result<Ingress> {
    if value.starts_with("filebeam:") {
        let url = Url::parse(value).context("invalid filebeam protocol link")?;
        if url.scheme() != "filebeam" || url.host_str() != Some("transfer") {
            bail!("unsupported filebeam protocol link");
        }
        let parts: Vec<_> = url
            .path_segments()
            .context("invalid filebeam protocol link")?
            .collect();
        if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) || url.query().is_some() {
            bail!("filebeam protocol links require an encoded origin and transfer ID");
        }
        let instance = String::from_utf8(
            URL_SAFE_NO_PAD
                .decode(parts[0])
                .context("protocol origin is not base64url")?,
        )
        .context("protocol origin is not UTF-8")?;
        let raw = format!(
            "{instance}/{}{}",
            parts[1],
            url.fragment().map_or(String::new(), |f| format!("#{f}"))
        );
        return Ok(Ingress::Link {
            link: filebeam_transfer_native::protocol::parse_link_for_instance(&raw, &instance)?,
        });
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(Ingress::Link {
            link: filebeam_transfer_native::protocol::parse_link_for_instance(
                value,
                origin.as_str(),
            )?,
        });
    }
    // A 26-byte token is an attempted bare transfer ID, not a filename fallback.
    if value.len() == 26 && value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Ok(Ingress::Link {
            link: filebeam_transfer_native::protocol::parse_link_for_instance(
                value,
                origin.as_str(),
            )?,
        });
    }
    let path = PathBuf::from(value);
    Ok(Ingress::Path(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    }))
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Activation {
    pub inputs: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Request {
    nonce: String,
    activation: Activation,
}
#[derive(Serialize, Deserialize)]
struct Response {
    accepted: bool,
}

pub enum Instance {
    Owner(InstanceOwner),
    Forwarded,
}

pub struct InstanceOwner {
    receiver: Receiver<Activation>,
    _lock: File,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(unix)]
    socket_identity: (u64, u64),
}

impl InstanceOwner {
    pub fn recv(&self) -> Result<Activation> {
        self.receiver.recv().context("desktop ingress closed")
    }
    pub fn try_recv(&self) -> Result<Option<Activation>> {
        match self.receiver.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("desktop ingress closed"),
        }
    }
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        #[cfg(unix)]
        {
            let _ = std::os::unix::net::UnixStream::connect(&self.socket);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{FileTypeExt, MetadataExt};
            if let Ok(metadata) = fs::symlink_metadata(&self.socket)
                && metadata.file_type().is_socket()
                && (metadata.dev(), metadata.ino()) == self.socket_identity
            {
                let _ = fs::remove_file(&self.socket);
            }
        }
    }
}

pub fn acquire(home: &Path, initial: Option<&str>) -> Result<Instance> {
    acquire_inputs(
        home,
        initial.map_or_else(Vec::new, |value| vec![value.to_owned()]),
    )
}

pub fn acquire_inputs(home: &Path, inputs: Vec<String>) -> Result<Instance> {
    let runtime = secure_runtime(home)?;
    let lock_path = runtime.join("ingress.lock");
    if fs::symlink_metadata(&lock_path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("ingress lock must not be a symlink");
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    let activation = Activation { inputs };
    match lock.try_lock_exclusive() {
        Ok(()) => acquire_owner(runtime, lock),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            forward(&runtime, activation)?;
            Ok(Instance::Forwarded)
        }
        Err(error) => Err(error.into()),
    }
}

fn secure_runtime(home: &Path) -> Result<PathBuf> {
    let home = home.canonicalize().context("canonicalize Filebeam home")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(&home)?;
        if metadata.uid() != current_uid()? || metadata.mode() & 0o022 != 0 {
            bail!("Filebeam home must be owned and writable only by the current user");
        }
    }
    let runtime = home.join("desktop");
    if fs::symlink_metadata(&runtime)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("desktop runtime must not be a symlink");
    }
    fs::create_dir_all(&runtime)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    }
    Ok(runtime)
}

#[cfg(unix)]
fn socket_path(runtime: &Path) -> Result<PathBuf> {
    let uid = current_uid()?;
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    socket_path_at(runtime, uid, xdg, std::env::temp_dir())
}

#[cfg(unix)]
fn socket_path_at(
    runtime: &Path,
    uid: u32,
    xdg: Option<PathBuf>,
    temporary: PathBuf,
) -> Result<PathBuf> {
    let runtime = runtime
        .canonicalize()
        .context("canonicalize desktop runtime")?;
    let mut base = xdg.unwrap_or_else(|| temporary.join(format!("filebeam-{uid}")));
    // macOS TMPDIR can itself approach sockaddr_un's limit. Keep the full
    // home digest and use a protected per-user directory under the short /tmp.
    if base.join("filebeam").as_os_str().as_encoded_bytes().len() + 38 >= 100 {
        base = PathBuf::from("/tmp").join(format!("filebeam-{uid}"));
    }
    ensure_private_directory(&base, uid)?;
    let directory = base.join("filebeam");
    ensure_private_directory(&directory, uid)?;

    let digest = Sha256::digest(runtime.as_os_str().as_encoded_bytes());
    let name: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let socket = directory.join(format!("{name}.sock"));
    // macOS allows 104 bytes and Linux 108; retain room for platform terminators.
    if socket.as_os_str().as_encoded_bytes().len() >= 100 {
        bail!("private runtime directory path is too long for a Unix socket");
    }
    Ok(socket)
}

#[cfg(unix)]
fn ensure_private_directory(path: &Path, uid: u32) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || metadata.uid() != uid
            || metadata.mode() & 0o077 != 0
        {
            bail!("private runtime directory is not owned and protected by the current user");
        }
    } else {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.mode() & 0o077 != 0
    {
        bail!("private runtime directory is not owned and protected by the current user");
    }
    Ok(())
}

#[cfg(unix)]
fn current_uid() -> Result<u32> {
    let output = std::process::Command::new("id")
        .arg("-u")
        .output()
        .context("determine current user")?;
    std::str::from_utf8(&output.stdout)?
        .trim()
        .parse()
        .context("current user id is invalid")
}

fn nonce() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::getrandom(&mut bytes).context("obtain ingress nonce")?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn write_nonce(runtime: &Path) -> Result<String> {
    let nonce = nonce()?;
    let path = runtime.join("ingress.nonce");
    let _ = fs::remove_file(&path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(nonce.as_bytes())?;
    file.sync_all()?;
    Ok(nonce)
}
fn read_nonce(runtime: &Path) -> Result<String> {
    let path = runtime.join("ingress.nonce");
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
        bail!("ingress nonce must not be a symlink");
    }
    let nonce = fs::read_to_string(path)?;
    if nonce.len() != 64 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("ingress nonce is invalid");
    }
    Ok(nonce)
}
fn encode(request: &Request) -> Result<Vec<u8>> {
    let value = serde_json::to_vec(request)?;
    if value.len() > MAX_PAYLOAD {
        bail!("desktop activation exceeds 64 KiB");
    }
    Ok(value)
}
fn write_frame(stream: &mut impl Write, value: &[u8]) -> io::Result<()> {
    stream.write_all(&(value.len() as u32).to_be_bytes())?;
    stream.write_all(value)
}
fn read_frame(stream: &mut impl Read) -> Result<Vec<u8>> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_PAYLOAD {
        bail!("desktop ingress frame exceeds 64 KiB");
    }
    let mut value = vec![0; length];
    stream.read_exact(&mut value)?;
    Ok(value)
}

#[cfg(unix)]
fn acquire_owner(runtime: PathBuf, lock: File) -> Result<Instance> {
    use std::os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixListener,
    };
    let socket = socket_path(&runtime)?;
    if let Ok(metadata) = fs::symlink_metadata(&socket) {
        if metadata.file_type().is_symlink()
            || !metadata.file_type().is_socket()
            || metadata.uid() != current_uid()?
        {
            bail!("ingress socket has an unexpected owner or type");
        }
        fs::remove_file(&socket).context("remove stale ingress socket")?;
    }
    let nonce = write_nonce(&runtime)?;
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    let metadata = fs::symlink_metadata(&socket)?;
    listener.set_nonblocking(true)?;
    let (sender, receiver) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || serve_unix(listener, nonce, sender, worker_stop));
    Ok(Instance::Owner(InstanceOwner {
        receiver,
        _lock: lock,
        stop,
        worker: Some(worker),
        socket,
        socket_identity: (metadata.dev(), metadata.ino()),
    }))
}
#[cfg(unix)]
fn serve_unix(
    listener: std::os::unix::net::UnixListener,
    nonce: String,
    sender: mpsc::Sender<Activation>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                if let Ok(frame) = read_frame(&mut stream)
                    && let Ok(request) = serde_json::from_slice::<Request>(&frame)
                    && request.nonce == nonce
                {
                    let accepted = sender.send(request.activation).is_ok();
                    let _ = write_frame(
                        &mut stream,
                        &serde_json::to_vec(&Response { accepted }).unwrap_or_default(),
                    );
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(_) => break,
        }
    }
}
#[cfg(unix)]
fn forward(runtime: &Path, activation: Activation) -> Result<()> {
    use std::os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
    };
    let socket = socket_path(runtime)?;
    let metadata = fs::symlink_metadata(&socket).context("inspect existing desktop ingress")?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_socket()
        || metadata.uid() != current_uid()?
    {
        bail!("existing desktop ingress is not owned by the current user");
    }
    let request = Request {
        nonce: read_nonce(runtime)?,
        activation,
    };
    let mut stream = UnixStream::connect(socket).context("connect to existing Filebeam desktop")?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write_frame(&mut stream, &encode(&request)?)?;
    let response: Response = serde_json::from_slice(&read_frame(&mut stream)?)?;
    if !response.accepted {
        bail!("existing Filebeam desktop rejected activation");
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    };

    fn home() -> PathBuf {
        let path = std::env::temp_dir().join(format!("filebeam-ingress-test-{}", nonce().unwrap()));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn second_instance_forwards_an_empty_activation_and_drop_reacquires() {
        let home = home();
        let owner = match acquire(&home, None).unwrap() {
            Instance::Owner(owner) => owner,
            Instance::Forwarded => panic!(),
        };
        assert!(matches!(acquire(&home, None).unwrap(), Instance::Forwarded));
        assert_eq!(owner.recv().unwrap().inputs, Vec::<String>::new());
        drop(owner);
        assert!(matches!(acquire(&home, None).unwrap(), Instance::Owner(_)));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn stale_and_hostile_frames_do_not_block_an_owner() {
        let home = home();
        let runtime = home.join("desktop");
        fs::create_dir(&runtime).unwrap();
        let stale = UnixListener::bind(socket_path(&runtime).unwrap()).unwrap();
        drop(stale);
        let owner = match acquire(&home, None).unwrap() {
            Instance::Owner(owner) => owner,
            Instance::Forwarded => panic!(),
        };
        let socket = socket_path(&runtime).unwrap();
        let mut invalid = UnixStream::connect(&socket).unwrap();
        write_frame(&mut invalid, b"no").unwrap();
        drop(invalid);
        let mut stream = UnixStream::connect(socket).unwrap();
        stream
            .write_all(&((MAX_PAYLOAD as u32) + 1).to_be_bytes())
            .unwrap();
        drop(stream);
        assert!(matches!(
            acquire(&home, Some("valid.json")).unwrap(),
            Instance::Forwarded
        ));
        assert_eq!(owner.recv().unwrap().inputs, vec!["valid.json"]);
        drop(owner);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn long_home_uses_a_short_private_socket_endpoint() {
        let home = home();
        let long = home.join("a".repeat(120)).join("b".repeat(120));
        fs::create_dir_all(&long).unwrap();
        fs::set_permissions(&long, fs::Permissions::from_mode(0o700)).unwrap();
        let owner = match acquire_inputs(&long, Vec::new()).unwrap() {
            Instance::Owner(owner) => owner,
            Instance::Forwarded => panic!(),
        };
        let endpoint = socket_path(&long.join("desktop")).unwrap();
        assert!(endpoint.as_os_str().as_encoded_bytes().len() < 100);
        assert!(endpoint.exists());
        drop(owner);
        assert!(!endpoint.exists());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn long_platform_temp_directory_uses_a_short_owned_fallback() {
        let root = home();
        let socket = socket_path_at(
            &root,
            current_uid().unwrap(),
            None,
            root.join("t".repeat(100)),
        )
        .unwrap();
        assert!(socket.as_os_str().as_encoded_bytes().len() < 100);
        ensure_private_directory(socket.parent().unwrap(), current_uid().unwrap()).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_aliases_resolve_to_the_same_socket() {
        let root = home();
        let runtime = root.join("real");
        let alias = root.join("alias");
        fs::create_dir(&runtime).unwrap();
        std::os::unix::fs::symlink(&runtime, &alias).unwrap();
        assert_eq!(socket_path(&runtime).unwrap(), socket_path(&alias).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_runtime_rejects_permissive_or_stale_owned_directories() {
        let root = home();
        let runtime = root.join("runtime");
        fs::create_dir(&runtime).unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(ensure_private_directory(&runtime, current_uid().unwrap()).is_err());
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
        ensure_private_directory(&runtime, current_uid().unwrap()).unwrap();
        let stale = runtime.join("stale");
        fs::write(&stale, "old").unwrap();
        assert!(ensure_private_directory(&stale, current_uid().unwrap()).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn canonical_links_preserve_fragment_and_their_own_origin() {
        let origin = Url::parse("https://default.example").unwrap();
        let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let raw = format!("https://other.example/01ARZ3NDEKTSV4RRFFQ69G5FAV#v1.{key}");
        let Ingress::Link { link } = parse(&raw, &origin).unwrap() else {
            panic!()
        };
        assert_eq!(link.instance, "https://other.example");
        assert!(link.key.is_some());
        let encoded = URL_SAFE_NO_PAD.encode("https://third.example");
        let protocol = format!("filebeam://transfer/{encoded}/01ARZ3NDEKTSV4RRFFQ69G5FAV#v1.{key}");
        let Ingress::Link { link } = parse(&protocol, &origin).unwrap() else {
            panic!()
        };
        assert_eq!(link.instance, "https://third.example");
        assert!(parse("01ARZ3NDEKTSV4RRFFQ69G5FAI", &origin).is_err());
        assert!(parse("filebeam://transfer/not-base64/invalid", &origin).is_err());
    }
}

#[cfg(windows)]
fn acquire_owner(runtime: PathBuf, lock: File) -> Result<Instance> {
    use std::net::TcpListener;
    let nonce = write_nonce(&runtime)?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    fs::write(
        runtime.join("ingress.port"),
        listener.local_addr()?.port().to_string(),
    )?;
    listener.set_nonblocking(true)?;
    let (sender, receiver) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || serve_tcp(listener, nonce, sender, worker_stop));
    Ok(Instance::Owner(InstanceOwner {
        receiver,
        _lock: lock,
        stop,
        worker: Some(worker),
    }))
}
#[cfg(windows)]
fn serve_tcp(
    listener: std::net::TcpListener,
    nonce: String,
    sender: mpsc::Sender<Activation>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                if let Ok(frame) = read_frame(&mut stream)
                    && let Ok(request) = serde_json::from_slice::<Request>(&frame)
                    && request.nonce == nonce
                {
                    let accepted = sender.send(request.activation).is_ok();
                    let _ = write_frame(
                        &mut stream,
                        &serde_json::to_vec(&Response { accepted }).unwrap_or_default(),
                    );
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(_) => break,
        }
    }
}
#[cfg(windows)]
fn forward(runtime: &Path, activation: Activation) -> Result<()> {
    use std::net::TcpStream;
    let port: u16 = fs::read_to_string(runtime.join("ingress.port"))?
        .trim()
        .parse()
        .context("existing ingress port is invalid")?;
    let request = Request {
        nonce: read_nonce(runtime)?,
        activation,
    };
    let mut stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        IO_TIMEOUT,
    )?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write_frame(&mut stream, &encode(&request)?)?;
    if !serde_json::from_slice::<Response>(&read_frame(&mut stream)?)?.accepted {
        bail!("existing Filebeam desktop rejected activation");
    }
    Ok(())
}
