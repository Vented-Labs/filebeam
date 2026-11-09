//! Signed, bounded release staging shared by `beam` and Filebeam Desktop.
use std::{
    collections::BTreeSet,
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
        #[cfg(target_os = "linux")]
        let executable = if product == Product::Desktop {
            current_appimage()?.unwrap_or(executable)
        } else {
            executable
        };
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

/// Coordinates an installed daemon with its supervisor during a component switch.
/// Implementations must leave profile data intact when returning an error.
pub trait DaemonService {
    fn drain(&self, _current: &Path) -> Result<()> {
        Ok(())
    }
    fn start(&self, _launcher: &Path) -> Result<()> {
        Ok(())
    }
    fn health(&self, candidate: &Path) -> Result<()>;
    fn rollback(&self, _restored_launcher: Option<&Path>) -> Result<()> {
        Ok(())
    }
}

/// Result of removing one product's claim on a shared daemon component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonRemoval {
    pub retained_for_other_owners: bool,
}

/// Stable per-profile service suffix. The default profile keeps the historical
/// name so an existing user service is migrated in place rather than duplicated.
pub fn daemon_service_name(home: &Path) -> Result<String> {
    if !home.is_absolute() {
        bail!("daemon profile home must be absolute");
    }
    let normalized = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    if let Some(default) = env::var_os("HOME") {
        let default = PathBuf::from(default).join(".filebeam");
        let default = default.canonicalize().unwrap_or(default);
        if normalized == default {
            return Ok("filebeam-daemon".into());
        }
    }
    let digest = hex::encode(Sha256::digest(normalized.to_string_lossy().as_bytes()));
    Ok(format!("filebeam-daemon-{}", &digest[..12]))
}

struct HealthOnly<H>(H);
impl<H: Fn(&Path) -> Result<()>> DaemonService for HealthOnly<H> {
    fn health(&self, candidate: &Path) -> Result<()> {
        (self.0)(candidate)
    }
}

pub struct Updater {
    options: Options,
}
impl Updater {
    pub fn new(options: Options) -> Self {
        Self { options }
    }
    /// Installs a daemon only from the component asset bound into this product's
    /// existing signed catalog. Callers provide a local health check; a failed
    /// check leaves the prior launcher untouched.
    pub fn install_daemon_component<F: Fetcher, H: Fn(&Path) -> Result<()>>(
        &self,
        fetcher: &F,
        owner: &str,
        ipc: u32,
        storage_schema: u32,
        health: H,
    ) -> Result<String> {
        self.install_daemon_component_with(fetcher, owner, ipc, storage_schema, &HealthOnly(health))
    }
    /// Installs or upgrades the catalog-bound shared daemon. The service hooks
    /// provide the drain/start/rollback boundary for CLI and desktop supervisors.
    pub fn install_daemon_component_with<F: Fetcher, S: DaemonService>(
        &self,
        fetcher: &F,
        owner: &str,
        ipc: u32,
        storage_schema: u32,
        service: &S,
    ) -> Result<String> {
        if owner.is_empty()
            || !owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("invalid daemon component owner");
        }
        if cfg!(windows) {
            bail!("Windows daemon installation requires runtime-reported service capabilities");
        }
        let _lease = self.lease()?;
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
        let mut state = self.load_state()?;
        if catalog.generation < state.entry(self.options.product).generation {
            bail!("release catalog generation rolled back");
        }
        let current = Version::parse(&self.options.version)?;
        let component = catalog
            .releases
            .into_iter()
            .filter(|release| !release.withdrawn)
            .filter_map(|release| {
                Version::parse(&release.version)
                    .ok()
                    .map(|version| (version, release.daemon))
            })
            .filter(|(version, _)| version >= &current)
            .filter_map(|(_, component)| component)
            .filter(|component| {
                component.min_ipc <= ipc
                    && ipc <= component.max_ipc
                    && component.storage_schema == storage_schema
                    && component.asset.os == os().unwrap_or("")
                    && component.asset.architecture == arch().unwrap_or("")
            })
            .max_by(|left, right| {
                Version::parse(&left.version)
                    .unwrap()
                    .cmp(&Version::parse(&right.version).unwrap())
            })
            .context("catalog has no compatible daemon component")?;
        let asset = component.asset.clone();
        let url = catalog_url.join(&asset.path)?;
        validate_origin(&url, self.options.product)?;
        let bytes = fetcher.get(&url, asset.size)?;
        if bytes.len() as u64 != asset.size
            || hex::encode(Sha256::digest(&bytes)) != asset.sha256.to_ascii_lowercase()
        {
            bail!("daemon archive checksum or size does not match the signed catalog");
        }
        let binary = extract_daemon_binary(&bytes)?;
        let root = self.options.home.join("runtime/daemon");
        ensure_component_root(&root)?;
        let version = Version::parse(&component.version)?;
        let component_version = version.to_string();
        let mut record = load_component_record(&root)?;
        if record
            .current
            .as_deref()
            .and_then(|current| Version::parse(current).ok())
            .is_some_and(|current| current > version)
        {
            bail!("signed daemon component would downgrade the installed version");
        }
        let target = root.join("versions").join(version.to_string());
        if fs::symlink_metadata(&target)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            bail!("daemon component version directory must not be a symlink");
        }
        if !target.join(daemon_executable_name()).exists() {
            let candidate = root
                .join("versions")
                .join(format!(".{}-{}", version, Uuid::new_v4()));
            fs::create_dir(&candidate)?;
            atomic_write(
                &candidate.join(daemon_executable_name()),
                &binary,
                Product::Cli,
            )?;
            fs::rename(candidate, &target)?;
        }
        let candidate_binary = target.join(daemon_executable_name());
        if fs::symlink_metadata(&candidate_binary)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            bail!("daemon component executable must not be a symlink");
        }
        service.health(&candidate_binary)?;
        let launcher = root.join("filebeamd");
        let previous_launcher = fs::read(&launcher).ok();
        let switching = record.current.as_deref() != Some(component_version.as_str());
        if switching && launcher.exists() {
            service.drain(&launcher)?;
        }
        let temporary = root.join(format!(".filebeamd-{}", Uuid::new_v4()));
        atomic_write(
            &temporary,
            format!(
                "#!/usr/bin/env sh\nexec \"{}\" \"$@\"\n",
                candidate_binary.display()
            )
            .as_bytes(),
            Product::Cli,
        )?;
        fs::rename(temporary, &launcher)?;
        if let Err(error) = service
            .start(&launcher)
            .and_then(|_| service.health(&candidate_binary))
        {
            if let Some(previous) = previous_launcher {
                atomic_write(&launcher, &previous, Product::Cli)?;
                let _ = service.rollback(Some(&launcher));
            } else {
                let _ = fs::remove_file(&launcher);
                let _ = service.rollback(None);
            }
            return Err(
                error.context("daemon component activation failed; restored prior launcher")
            );
        }
        record.current = Some(component_version);
        record.owners.insert(owner.into());
        save_component_record(&root, &record)?;
        state.entry_mut(self.options.product).generation = catalog.generation;
        self.save_state(&state)?;
        Ok(launcher.to_string_lossy().into_owned())
    }
    pub fn install_daemon_component_http<H: Fn(&Path) -> Result<()>>(
        &self,
        owner: &str,
        ipc: u32,
        storage_schema: u32,
        health: H,
    ) -> Result<String> {
        self.install_daemon_component(
            &HttpFetcher::new(Duration::from_secs(30)),
            owner,
            ipc,
            storage_schema,
            health,
        )
    }
    /// Drops one product's claim without deleting profile data or versioned binaries.
    /// The callback stops the supervisor only when this was the final owner.
    pub fn remove_daemon_component<F: FnOnce() -> Result<()>>(
        &self,
        owner: &str,
        stop_if_last: F,
    ) -> Result<DaemonRemoval> {
        if owner.is_empty()
            || !owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("invalid daemon component owner");
        }
        let _lease = self.lease()?;
        let root = self.options.home.join("runtime/daemon");
        let mut record = load_component_record(&root)?;
        record.owners.remove(owner);
        if !record.owners.is_empty() {
            save_component_record(&root, &record)?;
            return Ok(DaemonRemoval {
                retained_for_other_owners: true,
            });
        }
        stop_if_last()?;
        record.current = None;
        save_component_record(&root, &record)?;
        Ok(DaemonRemoval {
            retained_for_other_owners: false,
        })
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
                replace_file(&source, &self.options.executable, self.options.product)?;
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
            && let Some(image) = current_appimage()?
        {
            return Ok(image);
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
        let mut state: State = match fs::read(self.state_path()) {
            Ok(v) => serde_json::from_slice(&v).context("parse update state"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e.into()),
        }?;
        if let Ok(generation) =
            fs::read_to_string(self.options.home.join("cache/catalog-generation"))
            && let Ok(generation) = generation.trim().parse::<u64>()
        {
            state.cli.generation = state.cli.generation.max(generation);
        }
        Ok(state)
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

#[cfg(target_os = "linux")]
fn current_appimage() -> Result<Option<PathBuf>> {
    Ok(appimage_for_executable(
        &env::current_exe()?,
        env::var_os("APPIMAGE").map(PathBuf::from),
        env::var_os("APPDIR").map(PathBuf::from),
    ))
}

#[cfg(target_os = "linux")]
fn appimage_for_executable(
    current: &Path,
    image: Option<PathBuf>,
    directory: Option<PathBuf>,
) -> Option<PathBuf> {
    let image = image?;
    let bundled = directory?.join("usr/bin/filebeam").canonicalize().ok()?;
    // APPIMAGE may be inherited from an unrelated AppImage (such as a terminal).
    // Only update it when this process is its bundled Filebeam executable.
    (current.canonicalize().ok()? == bundled)
        .then(|| image.canonicalize().ok())
        .flatten()
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

#[derive(Default)]
struct ComponentRecord {
    schema: u64,
    current: Option<String>,
    owners: BTreeSet<String>,
}

fn ensure_component_root(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        bail!("daemon component root must not be a symlink");
    }
    let versions = root.join("versions");
    fs::create_dir_all(&versions)?;
    if fs::symlink_metadata(&versions)?.file_type().is_symlink() {
        bail!("daemon component versions directory must not be a symlink");
    }
    Ok(())
}

fn load_component_record(root: &Path) -> Result<ComponentRecord> {
    let path = root.join("component.json");
    let record = match fs::read(path) {
        Ok(bytes) => {
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).context("parse daemon component record")?;
            let owners = value
                .get("owners")
                .and_then(serde_json::Value::as_object)
                .context("daemon component record owners are invalid")?
                .iter()
                .filter_map(|(owner, present)| {
                    present
                        .as_bool()
                        .filter(|present| *present)
                        .map(|_| owner.clone())
                })
                .collect();
            ComponentRecord {
                schema: value
                    .get("schema")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                current: value
                    .get("current")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                owners,
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ComponentRecord::default(),
        Err(error) => return Err(error.into()),
    };
    if record.schema > 1
        || record
            .current
            .as_deref()
            .is_some_and(|version| Version::parse(version).is_err())
        || record.owners.iter().any(|owner| {
            owner.is_empty()
                || !owner
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        bail!("daemon component record is invalid");
    }
    Ok(record)
}

fn save_component_record(root: &Path, record: &ComponentRecord) -> Result<()> {
    ensure_component_root(root)?;
    let owners = record
        .owners
        .iter()
        .map(|owner| (owner.clone(), serde_json::Value::Bool(true)))
        .collect::<serde_json::Map<_, _>>();
    atomic_write_raw(
        &root.join("component.json"),
        &serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "current": record.current,
            "owners": owners,
        }))?,
    )
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
    #[serde(default = "legacy_cli_product")]
    product: String,
    releases: Vec<Release>,
}
#[derive(Deserialize)]
struct Release {
    version: String,
    #[serde(default)]
    withdrawn: bool,
    assets: Vec<Asset>,
    #[serde(default)]
    daemon: Option<DaemonComponent>,
}
#[derive(Clone, Deserialize)]
struct DaemonComponent {
    version: String,
    min_ipc: u32,
    max_ipc: u32,
    storage_schema: u32,
    asset: Asset,
}
#[derive(Clone, Deserialize)]
struct Asset {
    #[serde(default)]
    os: String,
    architecture: String,
    path: String,
    sha256: String,
    size: u64,
    #[serde(default)]
    kind: String,
}
fn legacy_cli_product() -> String {
    "cli".into()
}
enum PayloadKind {
    App,
}
impl PayloadKind {
    fn as_str(&self) -> &'static str {
        "app-tar-gz"
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
    let mut c: Catalog =
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
    if product == Product::Cli {
        for asset in c
            .releases
            .iter_mut()
            .flat_map(|release| &mut release.assets)
        {
            // Original CLI catalogs used implicit Linux assets and archive extensions.
            if asset.os.is_empty() {
                asset.os = "linux".into();
            }
            if asset.kind.is_empty() {
                asset.kind = if asset.os == "windows" {
                    "zip-exe"
                } else {
                    "tar-gz"
                }
                .into();
            }
        }
    }
    for release in &c.releases {
        if let Some(component) = &release.daemon {
            validate_daemon_component(component, product)?;
        }
    }
    Ok(c)
}

fn validate_daemon_component(component: &DaemonComponent, _product: Product) -> Result<()> {
    if Version::parse(&component.version).is_err()
        || component.min_ipc == 0
        || component.min_ipc > component.max_ipc
        || component.storage_schema == 0
        || component.asset.kind != "tar-gz"
    {
        bail!("signed catalog daemon component is invalid");
    }
    let asset = &component.asset;
    if asset.os.is_empty()
        || asset.architecture.is_empty()
        || asset.size == 0
        || asset.size > MAX_ARCHIVE_BYTES
        || !valid_sha256(&asset.sha256)
        || asset.path.starts_with('/')
        || asset
            .path
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || !asset.path.starts_with("versions/")
        || !asset.path.ends_with(".tar.gz")
    {
        bail!("signed catalog daemon component asset is unsafe");
    }
    Ok(())
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
    let suffix = if kind == "app-tar-gz" {
        ".app"
    } else {
        executable_suffix()
    };
    let target = root.join(format!("{}-{}{}", version, Uuid::new_v4(), suffix));
    if kind == "app-tar-gz" {
        extract_bundle(bytes, &target)?;
        return Ok(target);
    }
    let binary = extract_binary(bytes, kind, p)?;
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
fn extract_daemon_binary(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut archive = Archive::new(GzDecoder::new(Cursor::new(bytes)));
    let mut found = None;
    let mut count = 0;
    let mut total = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        count += 1;
        total += entry.size();
        if count > MAX_ARCHIVE_ENTRIES || total > MAX_UNPACKED_BYTES {
            bail!("daemon archive exceeds unpack limits");
        }
        if !safe_relative(&path)
            || !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir()
        {
            bail!("daemon archive contains an unsafe entry");
        }
        if path == Path::new(&format!("filebeamd/{}", daemon_executable_name())) {
            if found.is_some() || !entry.header().entry_type().is_file() || entry.size() == 0 {
                bail!("daemon archive contains an invalid executable");
            }
            let mut binary = Vec::new();
            entry.read_to_end(&mut binary)?;
            found = Some(binary);
        }
    }
    found.context("daemon archive does not contain filebeamd/filebeamd")
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
fn daemon_executable_name() -> &'static str {
    if cfg!(windows) {
        "filebeamd.exe"
    } else {
        "filebeamd"
    }
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
    fn signed_daemon_component_requires_valid_metadata_and_signature() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let payload = serde_json::json!({
            "schema": 1, "product": "cli", "generation": 1,
            "published_at": "2025-01-01T00:00:00Z", "expires_at": "2999-01-01T00:00:00Z",
            "releases": [{"version": "1.2.3", "assets": [], "daemon": {
                "version": "1.2.3", "min_ipc": 1, "max_ipc": 1, "storage_schema": 1,
                "asset": {"os": "linux", "architecture": "x86_64", "kind": "tar-gz", "path": "versions/v1.2.3/filebeamd.tar.gz", "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 1}
            }}]
        }).to_string();
        assert!(
            verify_catalog(
                &envelope(&payload, &key),
                &key.verifying_key(),
                Product::Cli
            )
            .is_ok()
        );
        let macos_desktop = payload
            .replace("\"product\":\"cli\"", "\"product\":\"desktop\"")
            .replace("\"os\":\"linux\"", "\"os\":\"macos\"");
        assert!(
            verify_catalog(
                &envelope(&macos_desktop, &key),
                &key.verifying_key(),
                Product::Desktop
            )
            .is_ok()
        );
        let tampered = payload.replace("\"max_ipc\":1", "\"max_ipc\":0");
        assert!(
            verify_catalog(
                &envelope(&tampered, &key),
                &key.verifying_key(),
                Product::Cli
            )
            .is_err()
        );
        let mut forged = envelope(&payload, &key);
        *forged.last_mut().unwrap() ^= 1;
        assert!(verify_catalog(&forged, &key.verifying_key(), Product::Cli).is_err());
    }

    #[test]
    fn isolated_profiles_receive_distinct_stable_service_names() {
        let left = Path::new("/tmp/filebeam-profile-a");
        let right = Path::new("/tmp/filebeam-profile-b");
        let left_name = daemon_service_name(left).unwrap();
        assert_eq!(left_name, daemon_service_name(left).unwrap());
        assert_ne!(left_name, daemon_service_name(right).unwrap());
        assert!(left_name.starts_with("filebeam-daemon-"));
    }

    #[test]
    fn already_installed_update_is_cleared_without_replacing_or_relaunching() {
        let directory = TestDirectory::new();
        let updater = Updater::new(
            Options::for_product(directory.0.clone(), Product::Desktop, "0.3.0", "").unwrap(),
        );
        let mut state = State::default();
        state.desktop.staged = Some(Staged {
            path: directory.0.join("missing").to_string_lossy().into_owned(),
            version: "0.3.0".into(),
            product: "desktop".into(),
            kind: "tar-gz".into(),
            sha256: "a".repeat(64),
        });
        updater.save_state(&state).unwrap();
        assert_eq!(updater.activate_staged(|| true).unwrap(), Activation::None);
        assert!(updater.load_state().unwrap().desktop.staged.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn portable_appimage_identity_rejects_an_inherited_parent_image() {
        let root = TestDirectory::new();
        let directory = root.0.join("mount");
        fs::create_dir_all(directory.join("usr/bin")).unwrap();
        let bundled = directory.join("usr/bin/filebeam");
        let other = root.0.join("standalone-filebeam");
        let image = root.0.join("Filebeam.AppImage");
        for path in [&bundled, &other, &image] {
            fs::write(path, b"fixture").unwrap();
        }
        assert_eq!(
            appimage_for_executable(&bundled, Some(image.clone()), Some(directory.clone())),
            Some(image.canonicalize().unwrap())
        );
        assert!(appimage_for_executable(&other, Some(image), Some(directory)).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn repeated_staging_keeps_the_first_validated_payload_intact() {
        let directory = TestDirectory::new();
        let version = Version::parse("0.3.0").unwrap();
        let first = stage_payload(
            b"first",
            "appimage",
            Product::Desktop,
            &directory.0,
            &version,
        )
        .unwrap();
        let second = stage_payload(
            b"second",
            "appimage",
            Product::Desktop,
            &directory.0,
            &version,
        )
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), b"first");
        assert_eq!(fs::read(second).unwrap(), b"second");
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
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
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
        archive
            .append_data(
                &mut header,
                "Filebeam.app/Contents/MacOS/filebeam",
                &b"app"[..],
            )
            .unwrap();
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        let target = stage_payload(
            &bytes,
            "app-tar-gz",
            Product::Desktop,
            &directory.0,
            &Version::parse("0.3.0").unwrap(),
        )
        .unwrap();
        assert_eq!(target.extension().unwrap(), "app");
        assert_eq!(
            fs::metadata(target.join("Contents/MacOS/filebeam"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
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
    fn existing_cli_catalogs_keep_their_legacy_wire_contract() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let payload = serde_json::json!({
            "schema": 1, "generation": 4, "published_at": "2026-01-01T00:00:00Z",
            "expires_at": "2999-01-01T00:00:00Z", "releases": [{"version": "0.2.0", "assets": [
                {"architecture": "x86_64", "path": "versions/v0.2.0/beam.tar.gz", "sha256": "a".repeat(64), "size": 1},
                {"os": "windows", "architecture": "x86_64", "path": "versions/v0.2.0/beam.zip", "sha256": "a".repeat(64), "size": 1}
            ]}]
        });
        let bytes = envelope(&payload.to_string(), &signing);
        let catalog = verify_catalog(&bytes, &signing.verifying_key(), Product::Cli).unwrap();
        assert_eq!(catalog.releases[0].assets[0].os, "linux");
        assert_eq!(catalog.releases[0].assets[0].kind, "tar-gz");
        assert_eq!(catalog.releases[0].assets[1].kind, "zip-exe");
        assert!(verify_catalog(&bytes, &signing.verifying_key(), Product::Desktop).is_err());
    }

    #[test]
    fn cli_upgrade_preserves_the_previous_catalog_rollback_floor() {
        let directory = TestDirectory::new();
        fs::create_dir(directory.0.join("cache")).unwrap();
        fs::write(directory.0.join("cache/catalog-generation"), "42\n").unwrap();
        let updater = Updater::new(
            Options::for_product(directory.0.clone(), Product::Cli, "0.3.0", "").unwrap(),
        );
        assert_eq!(updater.load_state().unwrap().cli.generation, 42);
        let mut state = updater.load_state().unwrap();
        state.cli.generation = 43;
        updater.save_state(&state).unwrap();
        assert_eq!(updater.load_state().unwrap().cli.generation, 43);
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
        php_catalog_round_trip(Product::Desktop);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn php_cli_catalog_round_trip_stages_the_published_cli_payload() {
        php_catalog_round_trip(Product::Cli);
    }
    #[cfg(target_os = "linux")]
    fn php_catalog_round_trip(product: Product) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = env::temp_dir().join(format!("filebeam-updater-{}", Uuid::new_v4()));
        fs::create_dir_all(&fixture).unwrap();
        let tag = "v9.8.7";
        let namespace = product.catalog();
        let writer_tag = if product == Product::Cli {
            format!("beam-{tag}")
        } else {
            tag.into()
        };
        let prefix = if product == Product::Cli {
            format!("beam-{tag}")
        } else {
            format!("filebeam-desktop-{tag}")
        };
        let selected = fixture.join(format!("{prefix}-linux-{}.tar.gz", arch().unwrap()));
        let mut archive = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut archive, flate2::Compression::fast());
            let mut tar = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(7);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(
                &mut header,
                format!("{0}/{0}", product.binary()),
                &b"updated"[..],
            )
            .unwrap();
            tar.finish().unwrap();
        }
        fs::write(&selected, &archive).unwrap();
        for suffix in [
            "linux-x86_64.tar.gz",
            "linux-aarch64.tar.gz",
            "macos-x86_64.tar.gz",
            "macos-aarch64.tar.gz",
            "windows-x86_64.zip",
        ] {
            let path = fixture.join(format!("{prefix}-{suffix}"));
            if path != selected {
                fs::write(path, b"installer").unwrap();
            }
        }
        assert!(
            Command::new("php")
                .arg(root.join(format!("scripts/release/{namespace}-write-release.php")))
                .args([
                    &writer_tag,
                    fixture.to_str().unwrap(),
                    fixture.join("release.json").to_str().unwrap()
                ])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("php")
                .arg(root.join(format!("scripts/release/{namespace}-update-index.php")))
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
        let catalog_url = format!("https://releases.filebeam.io/{namespace}/index.json");
        let asset_url = format!(
            "https://releases.filebeam.io/{namespace}/versions/{tag}/{}",
            selected.file_name().unwrap().to_string_lossy()
        );
        let fetcher = FixtureFetcher {
            responses: HashMap::from([(catalog_url, envelope.stdout), (asset_url, archive)]),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(()),
        };
        let home = fixture.join("managed");
        let updater = Updater::new(Options {
            home,
            product,
            version: "9.8.6".into(),
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            executable: env::current_exe().unwrap(),
        });
        assert!(
            updater
                .run_with_at(true, &fetcher, 1_800_000_000)
                .unwrap()
                .contains(&format!("staged {} 9.8.7", product.binary()))
        );
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
        assert_eq!(updater.activate_staged(|| false).unwrap(), Activation::None);
        let state = updater.load_state().unwrap();
        let staged = state.entry(product).staged.clone().unwrap();
        fs::write(&staged.path, b"tampered").unwrap();
        assert!(updater.activate_staged(|| true).is_err());
        fs::remove_dir_all(fixture).unwrap();
    }

    #[cfg(target_os = "linux")]
    fn daemon_archive() -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(6);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "filebeamd/filebeamd", &b"daemon"[..])
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap()
    }

    #[cfg(target_os = "linux")]
    fn traversal_daemon_archive() -> Vec<u8> {
        let mut tar = vec![0u8; 512];
        tar[..9].copy_from_slice(b"../escape");
        for (range, value) in [(100..108, 0o755), (124..136, 1), (136..148, 0)] {
            let text = format!("{:0width$o}\0", value, width = range.len() - 1);
            tar[range].copy_from_slice(text.as_bytes());
        }
        tar[148..156].fill(b' ');
        tar[156] = b'0';
        tar[257..263].copy_from_slice(b"ustar\0");
        tar[263..265].copy_from_slice(b"00");
        let checksum: u32 = tar.iter().map(|byte| u32::from(*byte)).sum();
        let text = format!("{:06o}\0 ", checksum);
        tar[148..156].copy_from_slice(text.as_bytes());
        tar.extend_from_slice(b"x");
        tar.resize(1024, 0);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar).unwrap();
        encoder.finish().unwrap()
    }

    #[cfg(target_os = "linux")]
    fn daemon_component_fetcher(key: &SigningKey, archive: Vec<u8>) -> FixtureFetcher {
        let path = "versions/v1.0.0/filebeamd.tar.gz";
        let payload = serde_json::json!({
            "schema": 1, "product": "cli", "generation": 1,
            "published_at": "2026-01-01T00:00:00Z", "expires_at": "2999-01-01T00:00:00Z",
            "releases": [{"version": "1.0.0", "assets": [], "daemon": {
                "version": "1.0.0", "min_ipc": 1, "max_ipc": 1, "storage_schema": 1,
                "asset": {"os": "linux", "architecture": arch().unwrap(), "kind": "tar-gz", "path": path,
                    "sha256": hex::encode(Sha256::digest(&archive)), "size": archive.len()}
            }}]
        });
        FixtureFetcher {
            responses: HashMap::from([
                (
                    "https://releases.filebeam.io/cli/index.json".into(),
                    envelope(&payload.to_string(), key),
                ),
                (format!("https://releases.filebeam.io/cli/{path}"), archive),
            ]),
            calls: AtomicUsize::new(0),
            gate: Mutex::new(()),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn signed_daemon_component_keeps_owners_data_and_restart_recovery() {
        let directory = TestDirectory::new();
        let signing = SigningKey::from_bytes(&[3; 32]);
        let updater = Updater::new(Options {
            home: directory.0.clone(),
            product: Product::Cli,
            version: "0.1.0".into(),
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            executable: env::current_exe().unwrap(),
        });
        let fetcher = daemon_component_fetcher(&signing, daemon_archive());
        let launcher = updater
            .install_daemon_component(&fetcher, "cli", 1, 1, |path| {
                assert_eq!(fs::read(path)?, b"daemon");
                Ok(())
            })
            .unwrap();
        let fetcher = daemon_component_fetcher(&signing, daemon_archive());
        updater
            .install_daemon_component(&fetcher, "desktop", 1, 1, |_| Ok(()))
            .unwrap();
        let retained = updater
            .remove_daemon_component("cli", || panic!("must retain daemon"))
            .unwrap();
        assert!(retained.retained_for_other_owners);
        assert!(Path::new(&launcher).exists());
        assert!(
            directory
                .0
                .join("runtime/daemon/versions/1.0.0/filebeamd")
                .exists()
        );
        let restarted = Updater::new(Options {
            home: directory.0.clone(),
            product: Product::Cli,
            version: "0.1.0".into(),
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            executable: env::current_exe().unwrap(),
        });
        assert!(
            restarted
                .load_component_record_for_test()
                .owners
                .contains("desktop")
        );
        let removed = restarted
            .remove_daemon_component("desktop", || Ok(()))
            .unwrap();
        assert!(!removed.retained_for_other_owners);
        assert!(
            directory
                .0
                .join("runtime/daemon/versions/1.0.0/filebeamd")
                .exists()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn signed_daemon_component_rejects_bad_hash_and_archive_traversal() {
        let signing = SigningKey::from_bytes(&[4; 32]);
        let directory = TestDirectory::new();
        let updater = Updater::new(Options {
            home: directory.0.clone(),
            product: Product::Cli,
            version: "0.1.0".into(),
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            executable: env::current_exe().unwrap(),
        });
        let mut fetcher = daemon_component_fetcher(&signing, daemon_archive());
        fetcher.responses.insert(
            "https://releases.filebeam.io/cli/versions/v1.0.0/filebeamd.tar.gz".into(),
            b"not the catalogued archive".to_vec(),
        );
        assert!(
            updater
                .install_daemon_component(&fetcher, "cli", 1, 1, |_| Ok(()))
                .is_err()
        );
        assert!(extract_daemon_binary(&traversal_daemon_archive()).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn failed_daemon_activation_restores_the_previous_launcher() {
        struct FailingService {
            health_checks: AtomicUsize,
            rollbacks: AtomicUsize,
        }
        impl DaemonService for FailingService {
            fn health(&self, _: &Path) -> Result<()> {
                if self.health_checks.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    bail!("new daemon did not become healthy")
                }
            }
            fn rollback(&self, _: Option<&Path>) -> Result<()> {
                self.rollbacks.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let directory = TestDirectory::new();
        let root = directory.0.join("runtime/daemon");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("component.json"),
            br#"{"schema":1,"current":"0.9.0","owners":{"cli":true}}"#,
        )
        .unwrap();
        fs::write(root.join("filebeamd"), b"old launcher").unwrap();
        let signing = SigningKey::from_bytes(&[5; 32]);
        let updater = Updater::new(Options {
            home: directory.0.clone(),
            product: Product::Cli,
            version: "0.1.0".into(),
            public_key: STANDARD.encode(signing.verifying_key().to_bytes()),
            executable: env::current_exe().unwrap(),
        });
        let service = FailingService {
            health_checks: AtomicUsize::new(0),
            rollbacks: AtomicUsize::new(0),
        };
        assert!(
            updater
                .install_daemon_component_with(
                    &daemon_component_fetcher(&signing, daemon_archive()),
                    "cli",
                    1,
                    1,
                    &service,
                )
                .is_err()
        );
        assert_eq!(fs::read(root.join("filebeamd")).unwrap(), b"old launcher");
        assert_eq!(service.rollbacks.load(Ordering::SeqCst), 1);
        assert_eq!(
            load_component_record(&root).unwrap().current.as_deref(),
            Some("0.9.0")
        );
    }

    #[cfg(target_os = "linux")]
    impl Updater {
        fn load_component_record_for_test(&self) -> ComponentRecord {
            load_component_record(&self.options.home.join("runtime/daemon")).unwrap()
        }
    }
}
