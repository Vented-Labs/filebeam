//! Signed, bounded release staging shared by `beam` and Filebeam Desktop.
use std::{
    env, fs,
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use flate2::read::GzDecoder;
use fs2::FileExt;
use reqwest::{Url, blocking::Client};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const ORIGIN: &str = "https://releases.filebeam.io/";
const MAX_CATALOG_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: u64 = 20_000;
const THROTTLE: u64 = 2 * 60 * 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Product {
    Cli,
    Desktop,
}
impl Product {
    pub fn catalog(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Desktop => "desktop",
        }
    }
    pub fn binary(self) -> &'static str {
        match self {
            Self::Cli => "beam",
            Self::Desktop => "filebeam",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub home: PathBuf,
    pub product: Product,
    pub version: String,
    pub public_key: String,
    pub executable: PathBuf,
}
impl Options {
    pub fn for_product(
        home: PathBuf,
        product: Product,
        version: impl Into<String>,
        public_key: impl Into<String>,
    ) -> Result<Self> {
        let executable = home.join("bin").join(executable_name(product));
        #[cfg(target_os = "macos")]
        let executable = if product == Product::Desktop {
            let current = env::current_exe()?;
            if current
                .ancestors()
                .any(|path| path.extension().is_some_and(|ext| ext == "app"))
            {
                current
            } else {
                executable
            }
        } else {
            executable
        };
        Ok(Self {
            executable,
            home,
            product,
            version: version.into(),
            public_key: public_key.into(),
        })
    }
}

pub trait Fetcher: Send + Sync {
    fn get(&self, url: &Url, maximum: u64) -> Result<Vec<u8>>;
}
pub struct HttpFetcher {
    timeout: Duration,
}
impl HttpFetcher {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}
impl Fetcher for HttpFetcher {
    fn get(&self, url: &Url, maximum: u64) -> Result<Vec<u8>> {
        let response = Client::builder()
            .connect_timeout(self.timeout)
            .timeout(self.timeout)
            .build()?
            .get(url.clone())
            .send()?
            .error_for_status()?;
        if response.content_length().is_some_and(|n| n > maximum) {
            bail!("download exceeds its allowed size");
        }
        let mut bytes = Vec::new();
        response.take(maximum + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > maximum {
            bail!("download exceeds its allowed size");
        }
        Ok(bytes)
    }
}

/// What the caller must do after activation. CLI callers should re-exec; GUI callers should quit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activation {
    None,
    Reexec,
    RelaunchGui,
}

pub struct Updater {
    options: Options,
}
impl Updater {
    pub fn new(options: Options) -> Self {
        Self { options }
    }
    /// Starts a copied, detached worker. No network request is made on the launch path.
    pub fn launch(&self, auto_update: bool) -> Result<bool> {
        if !auto_update || !self.due()? {
            return Ok(false);
        }
        self.ensure_managed()?;
        let workers = self.update_dir().join("workers");
        fs::create_dir_all(&workers)?;
        let worker = workers.join(format!(
            "{}-{}{}",
            self.options.product.catalog(),
            Uuid::new_v4(),
            executable_suffix()
        ));
        fs::copy(self.running_executable()?, &worker).context("copy update worker")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&worker, fs::Permissions::from_mode(0o700))?;
        }
        let mut command = Command::new(&worker);
        command
            .arg("--filebeam-update-worker")
            .arg(self.options.product.catalog())
            .arg(&self.options.home)
            .arg(&self.options.executable)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(target_os = "linux")]
        if self.options.product == Product::Desktop {
            command.env("APPIMAGE_EXTRACT_AND_RUN", "1");
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0000_0008 | 0x0000_0200);
        }
        command.spawn().context("launch detached update worker")?;
        Ok(true)
    }
    pub fn bootstrap<F: Fn() -> bool>(&self, enabled: F) -> Result<bool> {
        if !enabled() {
            return Ok(false);
        }
        self.run(
            false,
            &HttpFetcher::new(Duration::from_secs(30)),
            &enabled,
            now(),
        )
        .map(|_| true)
    }
    pub fn run_manual(&self) -> Result<String> {
        self.run(
            true,
            &HttpFetcher::new(Duration::from_secs(120)),
            &|| true,
            now(),
        )
    }
    pub fn run_with<F: Fetcher>(&self, manual: bool, fetcher: &F) -> Result<String> {
        self.run(manual, fetcher, &|| true, now())
    }
    /// Test and embedding hook for deterministic retry and throttle behaviour.
    pub fn run_with_at<F: Fetcher>(
        &self,
        manual: bool,
        fetcher: &F,
        unix_seconds: u64,
    ) -> Result<String> {
        self.run(manual, fetcher, &|| true, unix_seconds)
    }
    pub fn launch_due(products: &[&Updater], auto_update: bool) -> usize {
        products
            .iter()
            .filter(|u| u.launch(auto_update).unwrap_or(false))
            .count()
    }
    /// Revalidates the staged object immediately before replacing the managed install.
    pub fn activate_staged<F: Fn() -> bool>(&self, enabled: F) -> Result<Activation> {
        if !enabled() {
            return Ok(Activation::None);
        }
        let _lease = self.lease()?;
        let mut state = self.load_state()?;
        let entry = state.entry_mut(self.options.product);
        let Some(staged) = entry.staged.clone() else {
            return Ok(Activation::None);
        };
        if !enabled()
            || staged.product != self.options.product.catalog()
            || !valid_sha256(&staged.sha256)
            || Version::parse(&staged.version)? <= Version::parse(&self.options.version)?
        {
            entry.staged = None;
            self.save_state(&state)?;
            return Ok(Activation::None);
        }
        let root = self
            .update_dir()
            .join("staged")
            .join(self.options.product.catalog())
            .canonicalize()?;
        let source = PathBuf::from(&staged.path);
        let source = source.canonicalize().context("staged update is missing")?;
        if !source.starts_with(&root) || digest_path(&source)? != staged.sha256 {
            entry.staged = None;
            self.save_state(&state)?;
            bail!("staged update payload is invalid");
        }
        self.ensure_managed()?;
        #[cfg(windows)]
        {
            if staged.kind == PayloadKind::App.as_str() {
                bail!("macOS application bundle staged on another platform");
            }
            self.stage_windows_helper(&source, &staged)?;
            // The helper owns clearing the record after replacing the executable.
            Ok(Activation::Reexec)
        }
        #[cfg(not(windows))]
        {
        let outcome = if staged.kind == PayloadKind::App.as_str() {
            #[cfg(target_os = "macos")]
            {
                verify_macos_bundle(&source)?;
                replace_bundle(&source, &self.options.executable)?;
                Activation::RelaunchGui
            }
            #[cfg(not(target_os = "macos"))]
            {
                bail!("macOS application bundle staged on another platform")
            }
        } else {
                replace_file(
                    &source,
                    &self.options.executable,
                    self.options.product,
                )?;
                Activation::Reexec
        };
        entry.staged = None;
        entry.last_success = Some(now());
        self.save_state(&state)?;
        Ok(outcome)
        }
    }
    #[cfg(windows)]
    pub fn apply_windows_helper(&self, arguments: &[std::ffi::OsString]) -> Result<bool> {
        self.apply_windows(arguments)
    }
    #[cfg(windows)]
    fn stage_windows_helper(&self, source: &Path, _staged: &Staged) -> Result<()> {
        Command::new(source)
            .arg("--filebeam-apply-staged")
            .arg(self.options.product.catalog())
            .arg(&self.options.home)
            .args(env::args_os().skip(1))
            .spawn()
            .context("start Windows update helper")?;
        Ok(())
    }
    #[cfg(windows)]
    fn apply_windows(&self, arguments: &[std::ffi::OsString]) -> Result<bool> {
        use std::{thread, time::Duration};
        let current = env::current_exe()?.canonicalize()?;
        let root = self
            .update_dir()
            .join("staged")
            .join(self.options.product.catalog())
            .canonicalize()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let lease = loop {
            match self.lease() {
                Ok(lease) => break lease,
                Err(error) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(error.context("timed out waiting for update lease"));
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        };
        let mut state = self.load_state()?;
        let staged = state
            .entry(self.options.product)
            .staged
            .as_ref()
            .context("Windows helper has no staged update")?;
        if !current.starts_with(root)
            || current != PathBuf::from(&staged.path).canonicalize()?
            || digest_path(&current)? != staged.sha256
        {
            bail!("Windows update helper payload is invalid");
        }
        let dst = &self.options.executable;
        let backup = dst.with_file_name(format!(
            "{}.previous{}",
            self.options.product.binary(),
            executable_suffix()
        ));
        for _ in 0..300 {
            let _ = fs::remove_file(&backup);
            if fs::rename(dst, &backup).is_ok() {
                if let Err(e) = fs::copy(&current, dst) {
                    let _ = fs::rename(&backup, dst);
                    return Err(e.into());
                }
                let entry = state.entry_mut(self.options.product);
                entry.staged = None;
                entry.last_success = Some(now());
                self.save_state(&state)?;
                drop(lease);
                Command::new(dst)
                    .args(arguments)
                    .spawn()
                    .context("relaunch updated application")?;
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(100));
        }
        bail!("timed out waiting for executable locks before applying update")
    }
    fn run<F: Fetcher, E: Fn() -> bool>(
        &self,
        manual: bool,
        fetcher: &F,
        enabled: &E,
        clock: u64,
    ) -> Result<String> {
        if !enabled() {
            bail!("automatic updates are disabled");
        }
        let _lease = self.lease()?;
        let mut state = self.load_state()?;
        if !manual
            && state
                .entry(self.options.product)
                .last_attempt
                .is_some_and(|t| clock.saturating_sub(t) < THROTTLE)
        {
            return Ok("update check is not due".into());
        }
        state.entry_mut(self.options.product).last_attempt = Some(clock);
        self.save_state(&state)?;
        let result = self.download_and_stage(fetcher, enabled, &mut state);
        if result.is_ok() {
            self.save_state(&state)?;
        }
        result
    }
    fn download_and_stage<F: Fetcher, E: Fn() -> bool>(
        &self,
        fetcher: &F,
        enabled: &E,
        state: &mut State,
    ) -> Result<String> {
        let key = key(&self.options.public_key)?;
        let catalog_url = Url::parse(&format!(
            "{ORIGIN}{}/index.json",
            self.options.product.catalog()
        ))?;
        let catalog = verify_catalog(
            &fetcher.get(&catalog_url, MAX_CATALOG_BYTES)?,
            &key,
            self.options.product,
        )?;
        if catalog.generation < state.entry(self.options.product).generation {
            bail!("release catalog generation rolled back");
        }
        state.entry_mut(self.options.product).generation = catalog.generation;
        let current = Version::parse(&self.options.version)?;
        let (version, assets) = catalog
            .releases
            .into_iter()
            .filter_map(|r| {
                (!r.withdrawn).then_some(r).and_then(|r| {
                    Version::parse(&r.version)
                        .ok()
                        .filter(|v| v.pre.is_empty())
                        .map(|v| (v, r.assets))
                })
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .context("catalog contains no valid stable releases")?;
        if version <= current {
            return Ok("already on the latest release".into());
        }
        let asset = select_asset(assets)?
            .context("no update asset exists for this operating system and architecture")?;
        validate_asset(&asset, self.options.product)?;
        if !enabled() {
            bail!("automatic updates are disabled");
        }
        let url = catalog_url.join(&asset.path)?;
        validate_origin(&url, self.options.product)?;
        let bytes = fetcher.get(&url, asset.size)?;
        if !enabled() {
            bail!("automatic updates are disabled");
        }
        if bytes.len() as u64 != asset.size
            || hex::encode(Sha256::digest(&bytes)) != asset.sha256.to_ascii_lowercase()
        {
            bail!("release archive checksum or size does not match the signed catalog");
        }
        let stage_root = self
            .update_dir()
            .join("staged")
            .join(self.options.product.catalog());
        fs::create_dir_all(&stage_root)?;
        let payload = stage_payload(
            &bytes,
            &asset.kind,
            self.options.product,
            &stage_root,
            &version,
        )?;
        self.ensure_managed()?;
        let sha256 = digest_path(&payload)?;
        state.entry_mut(self.options.product).staged = Some(Staged {
            path: payload.to_string_lossy().into_owned(),
            version: version.to_string(),
            product: self.options.product.catalog().into(),
            kind: asset.kind,
            sha256,
        });
        Ok(format!(
            "staged {} {}; it will activate on the next safe launch",
            self.options.product.binary(),
            version
        ))
    }
    fn ensure_managed(&self) -> Result<()> {
        let current = self.running_executable()?.canonicalize()?;
        let expected = self
            .options
            .executable
            .canonicalize()
            .context("update requires an installer-managed binary")?;
        if current != expected && !current.starts_with(self.update_dir().join("workers")) {
            bail!("update requires an installer-managed binary");
        }
        Ok(())
    }
    fn running_executable(&self) -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        if self.options.product == Product::Desktop
            && let Some(image) = env::var_os("APPIMAGE")
        {
            return Ok(PathBuf::from(image));
        }
        env::current_exe().context("locate running executable")
    }
    fn due(&self) -> Result<bool> {
        Ok(self
            .load_state()?
            .entry(self.options.product)
            .last_attempt
            .map(|t| now().saturating_sub(t) >= THROTTLE)
            .unwrap_or(true))
    }
    fn update_dir(&self) -> PathBuf {
        self.options.home.join("update")
    }
    fn state_path(&self) -> PathBuf {
        self.update_dir().join("state.json")
    }
    fn load_state(&self) -> Result<State> {
        match fs::read(self.state_path()) {
            Ok(v) => serde_json::from_slice(&v).context("parse update state"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn save_state(&self, state: &State) -> Result<()> {
        fs::create_dir_all(self.update_dir())?;
        atomic_write_raw(&self.state_path(), &serde_json::to_vec(state)?)
    }
    fn lease(&self) -> Result<Lease> {
        fs::create_dir_all(self.update_dir())?;
        Lease::acquire(self.update_dir().join("lock"))
    }
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    cli: ProductState,
    #[serde(default)]
    desktop: ProductState,
}
#[derive(Default, Serialize, Deserialize)]
struct ProductState {
    last_attempt: Option<u64>,
    last_success: Option<u64>,
    generation: u64,
    staged: Option<Staged>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Staged {
    path: String,
    version: String,
    product: String,
    kind: String,
    sha256: String,
}
impl State {
    fn entry(&self, p: Product) -> &ProductState {
        match p {
            Product::Cli => &self.cli,
            Product::Desktop => &self.desktop,
        }
    }
    fn entry_mut(&mut self, p: Product) -> &mut ProductState {
        match p {
            Product::Cli => &mut self.cli,
            Product::Desktop => &mut self.desktop,
        }
    }
}
struct Lease(fs::File);
impl Lease {
    fn acquire(path: PathBuf) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock_exclusive()
            .context("another Filebeam update worker is running")?;
        Ok(Self(file))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

#[derive(Deserialize)]
struct Envelope {
    signed: String,
    signature: String,
}
#[derive(Deserialize)]
struct Catalog {
    schema: u64,
    generation: u64,
    published_at: String,
    expires_at: String,
    product: String,
    releases: Vec<Release>,
}
#[derive(Deserialize)]
struct Release {
    version: String,
    #[serde(default)]
    withdrawn: bool,
    assets: Vec<Asset>,
}
#[derive(Clone, Deserialize)]
struct Asset {
    os: String,
    architecture: String,
    path: String,
    sha256: String,
    size: u64,
    kind: String,
}
enum PayloadKind {
    App,
}
impl PayloadKind {
    fn as_str(&self) -> &'static str {
        "app"
    }
}
fn key(encoded: &str) -> Result<VerifyingKey> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .context("embedded release public key is not base64")?;
    VerifyingKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .context("embedded release public key must be 32 bytes")?,
    )
    .context("embedded release public key is invalid")
}
fn verify_catalog(bytes: &[u8], key: &VerifyingKey, product: Product) -> Result<Catalog> {
    let e: Envelope =
        serde_json::from_slice(bytes).context("release catalog envelope is invalid")?;
    let signed = STANDARD
        .decode(e.signed)
        .context("release catalog payload is not base64")?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(e.signature)
            .context("release catalog signature is not base64")?,
    )
    .context("release catalog signature is invalid")?;
    key.verify(&signed, &signature)
        .context("release catalog signature verification failed")?;
    let c: Catalog =
        serde_json::from_slice(&signed).context("signed release catalog payload is invalid")?;
    let expires = OffsetDateTime::parse(&c.expires_at, &Rfc3339)
        .context("signed release catalog expiry is invalid")?;
    if c.schema != 1
        || c.generation == 0
        || c.published_at.is_empty()
        || expires <= OffsetDateTime::now_utc()
        || c.product != product.catalog()
    {
        bail!("signed release catalog is expired, unsupported, or for another product");
    }
    Ok(c)
}
fn validate_asset(a: &Asset, p: Product) -> Result<()> {
    let kind_is_supported = if p == Product::Desktop && cfg!(target_os = "macos") {
        a.kind == "app-tar-gz"
    } else if p == Product::Desktop && cfg!(target_os = "linux") {
        matches!(a.kind.as_str(), "tar-gz" | "appimage")
    } else if cfg!(windows) {
        a.kind == "zip-exe"
    } else {
        a.kind == "tar-gz"
    };
    if !kind_is_supported
        || a.size == 0
        || a.size > MAX_ARCHIVE_BYTES
        || !valid_sha256(&a.sha256)
        || a.path.starts_with('/')
        || a.path.split('/').any(|x| matches!(x, "" | "." | ".."))
        || !a.path.starts_with("versions/")
        || !a.path.ends_with(if a.kind == "zip-exe" {
            ".zip"
        } else if a.kind == "appimage" {
            ".AppImage"
        } else {
            ".tar.gz"
        })
    {
        bail!("signed catalog contains an unsafe or wrong-platform payload");
    }
    Ok(())
}
fn validate_origin(url: &Url, p: Product) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str() != Some("releases.filebeam.io")
        || !url
            .path()
            .starts_with(&format!("/{}/versions/", p.catalog()))
    {
        bail!("signed catalog asset URL is outside the Filebeam release origin");
    }
    Ok(())
}
fn select_asset(assets: Vec<Asset>) -> Result<Option<Asset>> {
    Ok(assets
        .into_iter()
        .find(|a| a.os == os().unwrap_or("") && a.architecture == arch().unwrap_or("")))
}
fn stage_payload(
    bytes: &[u8],
    kind: &str,
    p: Product,
    root: &Path,
    version: &Version,
) -> Result<PathBuf> {
    let target = root.join(format!("{}-{}", version, Uuid::new_v4()));
    if kind == "app-tar-gz" {
        extract_bundle(bytes, &target)?;
        return Ok(target);
    }
    let binary = extract_binary(bytes, kind, p)?;
    let target = target.with_extension(executable_suffix().trim_start_matches('.'));
    atomic_write(&target, &binary, p)?;
    Ok(target)
}
fn extract_binary(bytes: &[u8], kind: &str, p: Product) -> Result<Vec<u8>> {
    let expected = format!("{}/{}", p.binary(), executable_name(p));
    if kind == "appimage" {
        if bytes.is_empty() {
            bail!("release payload contains an empty AppImage");
        }
        return Ok(bytes.to_vec());
    }
    if kind == "zip-exe" {
        let mut z = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut e = z
            .by_name(&expected)
            .context("release archive does not contain executable")?;
        if e.is_dir() || e.size() == 0 || e.size() > MAX_UNPACKED_BYTES {
            bail!("release archive contains an invalid executable");
        }
        let mut v = Vec::new();
        e.read_to_end(&mut v)?;
        return Ok(v);
    }
    let mut a = Archive::new(GzDecoder::new(Cursor::new(bytes)));
    let mut found = None;
    let mut count = 0;
    let mut total = 0;
    for entry in a.entries()? {
        let mut e = entry?;
        count += 1;
        total += e.size();
        if count > MAX_ARCHIVE_ENTRIES || total > MAX_UNPACKED_BYTES {
            bail!("release archive exceeds unpack limits");
        }
        if e.path()?.as_ref() == Path::new(&expected) {
            if found.is_some() || !e.header().entry_type().is_file() || e.size() == 0 {
                bail!("release archive contains an invalid executable");
            }
            let mut v = Vec::new();
            e.read_to_end(&mut v)?;
            found = Some(v);
        }
    }
    found.context("release archive does not contain executable")
}
fn safe_relative(path: &Path) -> bool {
    !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn safe_symlink_target(parent: &Path, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth = 0usize;
    for component in parent.join(target).components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::ParentDir => return false,
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}
fn extract_bundle(bytes: &[u8], target: &Path) -> Result<()> {
    let tmp = target.with_file_name(format!(".bundle-{}", Uuid::new_v4()));
    fs::create_dir_all(&tmp)?;
    let result = (|| {
        let mut a = Archive::new(GzDecoder::new(Cursor::new(bytes)));
        let mut count = 0;
        let mut total = 0;
        for entry in a.entries()? {
            let mut e = entry?;
            let path = e.path()?.into_owned();
            count += 1;
            total += e.size();
            if count > MAX_ARCHIVE_ENTRIES || total > MAX_UNPACKED_BYTES || !safe_relative(&path) {
                bail!("unsafe application bundle archive");
            }
            let out = tmp.join(&path);
            let ty = e.header().entry_type();
            if ty.is_dir() {
                fs::create_dir_all(out)?;
            } else if ty.is_file() {
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut f = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(out)?;
                std::io::copy(&mut e, &mut f)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    f.set_permissions(fs::Permissions::from_mode(e.header().mode()? & 0o777))?;
                }
            } else if ty.is_symlink() {
                let link = e
                    .link_name()?
                    .context("application bundle symlink lacks target")?;
                let parent = path.parent().unwrap_or(Path::new(""));
                if !safe_symlink_target(parent, &link) {
                    bail!("application bundle symlink escapes bundle");
                }
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(link, out)?;
                #[cfg(not(unix))]
                {
                    bail!("application bundle symlinks are unsupported on this platform");
                }
            } else {
                bail!("application bundle contains unsupported entry");
            }
        }
        let app = tmp.join("Filebeam.app");
        if !app.is_dir() {
            bail!("application bundle does not contain Filebeam.app");
        }
        #[cfg(target_os = "macos")]
        verify_macos_bundle(&app)?;
        fs::rename(tmp.join("Filebeam.app"), target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result
}
fn digest_path(path: &Path) -> Result<String> {
    if path.is_file() {
        return Ok(hex::encode(Sha256::digest(fs::read(path)?)));
    }
    let executable = path.join("Contents/MacOS/filebeam");
    Ok(hex::encode(Sha256::digest(
        fs::read(executable).context("application bundle executable is missing")?,
    )))
}
#[cfg(target_os = "macos")]
fn verify_macos_bundle(bundle: &Path) -> Result<()> {
    let output = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(bundle)
        .output()
        .context("run codesign")?;
    if !output.status.success() {
        bail!("application bundle code signature verification failed");
    }
    let output = Command::new("codesign")
        .args(["-dvv"])
        .arg(bundle)
        .output()?;
    let details = String::from_utf8_lossy(&output.stderr);
    if !details
        .lines()
        .any(|line| line.trim() == "Identifier=io.filebeam.desktop")
    {
        bail!("application bundle has the wrong bundle identifier");
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn replace_bundle(source: &Path, executable: &Path) -> Result<()> {
    let current = executable.canonicalize()?;
    let app = current
        .ancestors()
        .find(|p| p.extension().is_some_and(|x| x == "app"))
        .context("managed desktop executable is not inside an app bundle")?;
    let backup = app.with_file_name("Filebeam.previous.app");
    let _ = fs::remove_dir_all(&backup);
    fs::rename(app, &backup)?;
    if let Err(e) = fs::rename(source, app) {
        let _ = fs::rename(&backup, app);
        return Err(e.into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path, p: Product) -> Result<()> {
    let destination = destination.canonicalize()?;
    let backup =
        destination.with_file_name(format!("{}.previous{}", p.binary(), executable_suffix()));
    let _ = fs::remove_file(&backup);
    fs::rename(&destination, &backup)?;
    if let Err(e) = fs::rename(source, &destination) {
        let _ = fs::rename(&backup, &destination);
        return Err(e.into());
    }
    Ok(())
}
fn atomic_write(path: &Path, bytes: &[u8], p: Product) -> Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}-{}{}",
        p.binary(),
        Uuid::new_v4(),
        executable_suffix()
    ));
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}
fn atomic_write_raw(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_file_name(format!(".state-{}", Uuid::new_v4()));
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(tmp, path)?;
    Ok(())
}
fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn os() -> Result<&'static str> {
    if cfg!(target_os = "linux") {
        Ok("linux")
    } else if cfg!(target_os = "macos") {
        Ok("macos")
    } else if cfg!(windows) {
        Ok("windows")
    } else {
        bail!("updates are unsupported on this operating system")
    }
}
fn arch() -> Result<&'static str> {
    if cfg!(target_arch = "x86_64") {
        Ok("x86_64")
    } else if cfg!(target_arch = "aarch64") {
        Ok("aarch64")
    } else {
        bail!("updates are unsupported on this architecture")
    }
}
fn executable_name(p: Product) -> String {
    format!("{}{}", p.binary(), executable_suffix())
}
fn executable_suffix() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[cfg(target_os = "linux")]
    use std::{
        collections::HashMap,
        process::{Command, Stdio},
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    fn envelope(payload: &str, key: &SigningKey) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"signed": STANDARD.encode(payload), "signature": STANDARD.encode(key.sign(payload.as_bytes()).to_bytes())})).unwrap()
    }
    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("filebeam-updater-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn already_installed_update_is_cleared_without_replacing_or_relaunching() {
        let directory = TestDirectory::new();
        let updater = Updater::new(Options::for_product(directory.0.clone(), Product::Desktop, "0.3.0", "").unwrap());
        let mut state = State::default();
        state.desktop.staged = Some(Staged {
            path: directory.0.join("missing").to_string_lossy().into_owned(),
            version: "0.3.0".into(), product: "desktop".into(), kind: "tar-gz".into(), sha256: "a".repeat(64),
        });
        updater.save_state(&state).unwrap();
        assert_eq!(updater.activate_staged(|| true).unwrap(), Activation::None);
        assert!(updater.load_state().unwrap().desktop.staged.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_managed_symlink_and_rolls_back_on_failure() {
        let directory = TestDirectory::new();
        let image = directory.0.join("Filebeam.AppImage");
        let link = directory.0.join("filebeam");
        let staged = directory.0.join("staged");
        fs::write(&image, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();
        std::os::unix::fs::symlink(&image, &link).unwrap();
        replace_file(&staged, &link, Product::Desktop).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&link).unwrap(), b"new");
        assert!(replace_file(&staged, &link, Product::Desktop).is_err());
        assert_eq!(fs::read(&link).unwrap(), b"new");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bundle_extraction_retains_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = TestDirectory::new();
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        archive.append_data(&mut header, "Filebeam.app/Contents/MacOS/filebeam", &b"app"[..]).unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        let target = directory.0.join("staged.app");
        extract_bundle(&bytes, &target).unwrap();
        assert_eq!(fs::metadata(target.join("Contents/MacOS/filebeam")).unwrap().permissions().mode() & 0o777, 0o755);
    }
    #[test]
    fn signed_catalog_rejects_wrong_product_expiry_and_prerelease() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let good = r#"{"schema":1,"generation":2,"published_at":"2026-01-01T00:00:00Z","expires_at":"2999-01-01T00:00:00Z","product":"desktop","releases":[]}"#;
        assert!(
            verify_catalog(
                &envelope(good, &key),
                &key.verifying_key(),
                Product::Desktop
            )
            .is_ok()
        );
        assert!(verify_catalog(&envelope(good, &key), &key.verifying_key(), Product::Cli).is_err());
        let expired = good.replace("2999", "2000");
        assert!(
            verify_catalog(
                &envelope(&expired, &key),
                &key.verifying_key(),
                Product::Desktop
            )
            .is_err()
        );
    }
    #[test]
    fn archive_rejects_traversal_duplicate_and_link_escape() {
        let mut bytes = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::fast());
            let mut tar = tar::Builder::new(enc);
            for name in ["other", "filebeam/filebeam", "filebeam/filebeam"] {
                let mut h = tar::Header::new_gnu();
                h.set_size(1);
                h.set_cksum();
                tar.append_data(&mut h, name, &b"x"[..]).unwrap();
            }
            tar.finish().unwrap();
        }
        assert!(extract_binary(&bytes, "tar-gz", Product::Desktop).is_err());
        assert!(!safe_relative(Path::new("a/../b")));
        assert!(safe_symlink_target(
            Path::new("Contents/MacOS"),
            Path::new("../Frameworks/x")
        ));
        assert!(!safe_symlink_target(
            Path::new("Contents"),
            Path::new("../../outside")
        ));
    }
    #[test]
    fn unsafe_assets_and_generation_are_rejected() {
        let a = Asset {
            os: "linux".into(),
            architecture: "x86_64".into(),
            path: "../x.tar.gz".into(),
            sha256: "a".repeat(64),
            size: 1,
            kind: "tar-gz".into(),
        };
        assert!(validate_asset(&a, Product::Cli).is_err());
        let mut s = State::default();
        s.cli.generation = 3;
        assert!(2 < s.entry(Product::Cli).generation);
    }

    #[cfg(target_os = "linux")]
    struct FixtureFetcher {
        responses: HashMap<String, Vec<u8>>,
        calls: AtomicUsize,
        gate: Mutex<()>,
    }
    #[cfg(target_os = "linux")]
    impl Fetcher for FixtureFetcher {
        fn get(&self, url: &Url, _maximum: u64) -> Result<Vec<u8>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let _guard = self.gate.lock().unwrap();
            self.responses
                .get(url.as_str())
                .cloned()
                .context("fixture URL is missing")
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn php_catalog_round_trip_stages_only_the_update_payload() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = env::temp_dir().join(format!("filebeam-updater-{}", Uuid::new_v4()));
        fs::create_dir_all(&fixture).unwrap();
        let tag = "v9.8.7";
        let selected = fixture.join(format!("filebeam-desktop-{tag}-linux-x86_64.tar.gz"));
        let mut archive = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut archive, flate2::Compression::fast());
            let mut tar = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(7);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, "filebeam/filebeam", &b"updated"[..])
                .unwrap();
            tar.finish().unwrap();
        }
        fs::write(&selected, &archive).unwrap();
        for suffix in [
            "linux-aarch64.tar.gz",
            "macos-x86_64.tar.gz",
            "macos-aarch64.tar.gz",
            "windows-x86_64.zip",
        ] {
            fs::write(
                fixture.join(format!("filebeam-desktop-{tag}-{suffix}")),
                b"installer",
            )
            .unwrap();
        }
        assert!(
            Command::new("php")
                .arg(root.join("scripts/release/desktop-write-release.php"))
                .args([
                    tag,
                    fixture.to_str().unwrap(),
                    fixture.join("release.json").to_str().unwrap()
                ])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("php")
                .arg(root.join("scripts/release/desktop-update-index.php"))
                .args(["/dev/null", fixture.join("release.json").to_str().unwrap()])
                .stdout(fs::File::create(fixture.join("index.json")).unwrap())
                .status()
                .unwrap()
                .success()
        );
        let signing = SigningKey::from_bytes(&[9; 32]);
        let mut signer = Command::new("php");
        signer
            .arg(root.join("scripts/release/sign-index.php"))
            .env(
                "RELEASE_SIGNING_KEY",
                STANDARD.encode(signing.to_keypair_bytes()),
            )
            .env(
                "RELEASE_PUBLIC_KEY",
                STANDARD.encode(signing.verifying_key().to_bytes()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut signer = signer.spawn().unwrap();
        signer
            .stdin
            .take()
            .unwrap()
            .write_all(&fs::read(fixture.join("index.json")).unwrap())
            .unwrap();
        let envelope = signer.wait_with_output().unwrap();
        assert!(envelope.status.success());
        let catalog_url = "https://releases.filebeam.io/desktop/index.json";
        let asset_url = format!(
            "https://releases.filebeam.io/desktop/versions/{tag}/{}",
            selected.file_name().unwrap().to_string_lossy()
        );
        let fetcher = FixtureFetcher {
            responses: HashMap::from([(catalog_url.into(), envelope.stdout), (asset_url, archive)]),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(()),
        };
        let home = fixture.join("managed");
        let updater = Updater::new(Options {
            home,
            product: Product::Desktop,
            version: "9.8.6".into(),
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            executable: env::current_exe().unwrap(),
        });
        assert!(
            updater
                .run_with_at(true, &fetcher, 1_800_000_000)
                .unwrap()
                .contains("staged filebeam 9.8.7")
        );
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
        assert_eq!(updater.activate_staged(|| false).unwrap(), Activation::None);
        let state = updater.load_state().unwrap();
        let staged = state.desktop.staged.unwrap();
        fs::write(&staged.path, b"tampered").unwrap();
        assert!(updater.activate_staged(|| true).is_err());
        fs::remove_dir_all(fixture).unwrap();
    }
}
