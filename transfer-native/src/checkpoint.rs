use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[cfg(not(windows))]
use std::fs::OpenOptions;

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Serialize, de::DeserializeOwned};

#[cfg(windows)]
#[path = "checkpoint/windows.rs"]
mod windows;

const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMMUTABLE_BYTES: u64 = 25_000_000;
const LOCK_NAME: &str = "lock";
const CHECKPOINT_NAME: &str = "checkpoint.json";
const CHECKPOINT_TEMP_NAME: &str = "checkpoint.json.new";

/// A private job directory. The advisory lock is released by the OS after a
/// crash, unlike a create-new sentinel, so a restarted command can resume.
pub struct Store {
    directory: PathBuf,
    _lock: File,
}

impl Store {
    /// Create a new job only. Existing jobs, including incomplete ones, must be
    /// opened explicitly so callers cannot overwrite resumable state by mistake.
    pub fn create(root: &Path, id: &str) -> Result<Self> {
        validate_name(id, "transfer job id")?;
        ensure_platform_supported()?;
        ensure_private_root(root, true)?;

        let directory = root.join(id);
        create_private_dir(&directory)
            .with_context(|| format!("create {}", directory.display()))?;
        open_locked(directory, id)
    }

    /// Open an existing job only. This never creates a root or job directory.
    pub fn open(root: &Path, id: &str) -> Result<Self> {
        validate_name(id, "transfer job id")?;
        ensure_platform_supported()?;
        ensure_private_root(root, false)?;

        let directory = root.join(id);
        ensure_private_dir(&directory)
            .with_context(|| format!("validate {}", directory.display()))?;
        open_locked(directory, id)
    }

    pub fn path(&self) -> &Path {
        &self.directory
    }

    /// Atomically replace the checkpoint. A write or sync failure leaves the
    /// prior checkpoint untouched; a stale complete temporary file is discarded.
    pub fn save<T: Serialize>(&self, state: &T) -> Result<()> {
        self.save_named(CHECKPOINT_NAME, state)
    }

    /// Atomically replace an owner-only JSON record in this locked job.
    pub fn save_named<T: Serialize>(&self, name: &str, state: &T) -> Result<()> {
        validate_checkpoint_name(name)?;
        let encoded = serde_json::to_vec(state).context("serialize transfer checkpoint")?;
        if encoded.len() as u64 > MAX_CHECKPOINT_BYTES {
            bail!("transfer checkpoint exceeds {MAX_CHECKPOINT_BYTES} byte limit");
        }

        let temporary = self.directory.join(format!("{name}.new"));
        let path = self.directory.join(name);
        remove_stale_private_file(&temporary)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => validate_private_regular_file(&path, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
        }
        write_new_private_file(&temporary, &encoded)?;
        replace_private_file(&temporary, &path, true)
            .context("atomically save transfer checkpoint")?;
        sync_directory(&self.directory)?;
        Ok(())
    }

    pub fn load<T: DeserializeOwned>(&self) -> Result<Option<T>> {
        self.load_named(CHECKPOINT_NAME)
    }

    /// Load an owner-only JSON record from this locked job.
    pub fn load_named<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        validate_checkpoint_name(name)?;
        let path = self.directory.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
        };
        validate_private_regular_file(&path, &metadata)?;
        if metadata.len() > MAX_CHECKPOINT_BYTES {
            bail!("transfer checkpoint exceeds {MAX_CHECKPOINT_BYTES} byte limit");
        }

        let file = File::open(&path).with_context(|| format!("read {}", path.display()))?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_CHECKPOINT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
            bail!("transfer checkpoint exceeds {MAX_CHECKPOINT_BYTES} byte limit");
        }
        Ok(Some(
            serde_json::from_slice(&bytes).context("parse transfer checkpoint")?,
        ))
    }

    /// Remove a named record after its replacement state is durable.
    pub fn remove_named(&self, name: &str) -> Result<()> {
        validate_checkpoint_name(name)?;
        let path = self.directory.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                validate_private_regular_file(&path, &metadata)?;
                fs::remove_file(&path)?;
                sync_directory(&self.directory)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
        }
        Ok(())
    }

    /// Store ciphertext exactly once before it is handed to the network. The
    /// source is read only; a changed source therefore cannot reuse a nonce.
    pub fn persist_immutable(&self, name: &str, ciphertext: &[u8]) -> Result<PathBuf> {
        validate_artifact_name(name)?;
        if ciphertext.len() as u64 > MAX_IMMUTABLE_BYTES {
            bail!("immutable ciphertext exceeds {MAX_IMMUTABLE_BYTES} byte limit");
        }

        let path = self.directory.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => bail!("immutable ciphertext already exists: {}", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
        }

        let temporary = self.directory.join(format!("{name}.new"));
        remove_stale_private_file(&temporary)?;
        write_new_private_file(&temporary, ciphertext)?;
        replace_private_file(&temporary, &path, false)
            .with_context(|| format!("atomically save immutable ciphertext {}", path.display()))?;
        sync_directory(&self.directory)?;
        Ok(path)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = self._lock.unlock();
    }
}

fn open_locked(directory: PathBuf, id: &str) -> Result<Store> {
    let lock_path = directory.join(LOCK_NAME);
    match fs::symlink_metadata(&lock_path) {
        Ok(metadata) => validate_private_regular_file(&lock_path, &metadata)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect {}", lock_path.display()));
        }
    }
    let lock = open_private_file(&lock_path, true, false)
        .with_context(|| format!("open lock for transfer {id}"))?;
    let metadata = fs::symlink_metadata(&lock_path)?;
    validate_private_regular_file(&lock_path, &metadata)?;
    lock.try_lock_exclusive()
        .with_context(|| format!("transfer {id} is already active"))?;
    Ok(Store {
        directory,
        _lock: lock,
    })
}

fn validate_name(name: &str, kind: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && !name.ends_with(['.', ' '])
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !is_windows_device_name(name);
    if !valid {
        bail!("invalid {kind}");
    }
    Ok(())
}

fn validate_artifact_name(name: &str) -> Result<()> {
    validate_name(name, "immutable ciphertext name")?;
    if matches!(name, LOCK_NAME | CHECKPOINT_NAME | CHECKPOINT_TEMP_NAME) || name.ends_with(".new")
    {
        bail!("reserved immutable ciphertext name");
    }
    Ok(())
}

fn validate_checkpoint_name(name: &str) -> Result<()> {
    validate_name(name, "checkpoint name")?;
    if name == LOCK_NAME || name.ends_with(".new") {
        bail!("reserved checkpoint name");
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_platform_supported() -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn ensure_platform_supported() -> Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_platform_supported() -> Result<()> {
    bail!("checkpoint persistence is unsupported on this platform")
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default();
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn ensure_private_root(root: &Path, create: bool) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => validate_private_root(root, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            create_private_root(root)?;
            ensure_private_dir(root)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("transfer root does not exist: {}", root.display())
        }
        Err(error) => {
            Err(error).with_context(|| format!("inspect transfer root {}", root.display()))
        }
    }
}

fn create_private_root(root: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut path = root;
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() {
                    bail!("private path is not a directory: {}", path.display());
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(path);
                path = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!("cannot create transfer root: {}", root.display())
                    })?;
            }
            Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
        }
    }

    for path in missing.into_iter().rev() {
        create_private_dir(path)?;
        ensure_private_dir(path)?;
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<()> {
    #[cfg(windows)]
    return windows::create_private_dir(path).map_err(Into::into);

    #[cfg(not(windows))]
    {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)?;
        Ok(())
    }
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    validate_private_directory(path, &metadata)
}

fn validate_private_root(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if !metadata.file_type().is_dir() {
        bail!("private path is not a directory: {}", path.display());
    }
    #[cfg(unix)]
    return validate_platform_owner(path, metadata);

    #[cfg(not(unix))]
    validate_platform_private(path, metadata)
}

fn remove_stale_private_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_private_regular_file(path, &metadata)?;
            fs::remove_file(path)
                .with_context(|| format!("remove incomplete temporary file {}", path.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    }
    Ok(())
}

fn write_new_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = open_private_file(path, false, true)
        .with_context(|| format!("create private file {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn open_private_file(path: &Path, append: bool, create_new: bool) -> std::io::Result<File> {
    #[cfg(windows)]
    return windows::open_private_file(path, append, create_new);

    #[cfg(not(windows))]
    {
        let mut options = OpenOptions::new();
        options.read(append).write(true);
        if create_new {
            options.create_new(true);
        } else {
            options.create(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }
}

#[cfg(windows)]
fn replace_private_file(
    source: &Path,
    destination: &Path,
    replace_existing: bool,
) -> std::io::Result<()> {
    windows::replace_file(source, destination, replace_existing)
}

#[cfg(not(windows))]
fn replace_private_file(
    source: &Path,
    destination: &Path,
    replace_existing: bool,
) -> std::io::Result<()> {
    if replace_existing {
        fs::rename(source, destination)
    } else {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    }
}

fn validate_private_regular_file(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if !metadata.file_type().is_file() {
        bail!("private path is not a regular file: {}", path.display());
    }
    validate_platform_private(path, metadata)
}

fn validate_private_directory(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if !metadata.file_type().is_dir() {
        bail!("private path is not a directory: {}", path.display());
    }
    validate_platform_private(path, metadata)
}

#[cfg(unix)]
fn validate_platform_private(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    if metadata.uid() != current_euid() || metadata.mode() & 0o077 != 0 {
        bail!("private path is not owner-only: {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn validate_platform_owner(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    if metadata.uid() != current_euid() {
        bail!(
            "private path is not owned by the current user: {}",
            path.display()
        );
    }
    Ok(())
}

// libc is intentionally not a dependency of this small crate. POSIX exposes
// getuid through the standard C ABI on the Unix targets supported by the CLI.
#[cfg(unix)]
fn current_euid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

#[cfg(windows)]
fn validate_platform_private(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    windows::validate_private_path(path, metadata.file_type().is_dir(), true)
}

#[cfg(not(any(unix, windows)))]
fn validate_platform_private(_: &Path, _: &fs::Metadata) -> Result<()> {
    bail!("checkpoint persistence is unsupported on this platform")
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct State {
        offset: u64,
        checksum: String,
    }

    #[test]
    fn checkpoint_is_reloadable_and_lock_is_exclusive_then_released() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job-1").unwrap();
        store
            .save(&State {
                offset: 12,
                checksum: "abc".into(),
            })
            .unwrap();
        assert_eq!(
            store.load::<State>().unwrap(),
            Some(State {
                offset: 12,
                checksum: "abc".into()
            })
        );
        assert!(Store::open(root.path(), "job-1").is_err());
        drop(store);
        assert!(Store::open(root.path(), "job-1").is_ok());
    }

    #[test]
    fn create_does_not_open_existing_and_open_does_not_create_missing() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job").unwrap();
        drop(store);
        assert!(Store::create(root.path(), "job").is_err());
        assert!(Store::open(root.path(), "missing").is_err());
        assert!(!root.path().join("missing").exists());
    }

    #[test]
    fn create_makes_missing_root_parents_private() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary
            .path()
            .join("missing")
            .join("home")
            .join("transfers");
        let store = Store::create(&root, "job").unwrap();
        assert_eq!(store.path(), root.join("job"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            for path in [
                temporary.path().join("missing"),
                temporary.path().join("missing/home"),
                root,
            ] {
                assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o700);
            }
        }
    }

    #[test]
    fn rejects_traversal_windows_devices_and_unsafe_names() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "../secret",
            "a/b",
            "a\\b",
            "",
            ".",
            "..",
            "a:b",
            "CON",
            "nul.txt",
            "name. ",
        ] {
            assert!(Store::create(root.path(), name).is_err(), "{name}");
        }
    }

    #[test]
    fn checkpoint_replaces_existing_data_and_discards_stale_temp() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job").unwrap();
        store
            .save(&State {
                offset: 1,
                checksum: "old".into(),
            })
            .unwrap();
        write_new_private_file(&store.path().join(CHECKPOINT_TEMP_NAME), b"incomplete").unwrap();
        store
            .save(&State {
                offset: 2,
                checksum: "new".into(),
            })
            .unwrap();
        assert_eq!(
            store.load::<State>().unwrap(),
            Some(State {
                offset: 2,
                checksum: "new".into()
            })
        );
        assert!(!store.path().join(CHECKPOINT_TEMP_NAME).exists());
    }

    #[test]
    fn ciphertext_is_durable_never_overwritten_and_recovers_stale_temp() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job").unwrap();
        write_new_private_file(&store.path().join("chunk-0.new"), b"partial").unwrap();
        let path = store.persist_immutable("chunk-0", b"ciphertext").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"ciphertext");
        assert!(store.persist_immutable("chunk-0", b"changed").is_err());
        assert!(store.persist_immutable("lock", b"reserved").is_err());
    }

    #[test]
    fn checkpoint_read_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job").unwrap();
        let checkpoint = store.path().join(CHECKPOINT_NAME);
        let file = open_private_file(&checkpoint, false, true).unwrap();
        file.set_len(MAX_CHECKPOINT_BYTES + 1).unwrap();
        file.sync_all().unwrap();
        assert!(store.load::<State>().is_err());
    }

    #[test]
    fn named_checkpoints_are_atomic_and_share_the_existing_lock() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job").unwrap();
        store
            .save_named(
                "stage-0-0.json",
                &State {
                    offset: 1,
                    checksum: "old".into(),
                },
            )
            .unwrap();
        write_new_private_file(&store.path().join("stage-0-0.json.new"), b"partial").unwrap();
        store
            .save_named(
                "stage-0-0.json",
                &State {
                    offset: 2,
                    checksum: "new".into(),
                },
            )
            .unwrap();
        assert_eq!(
            store.load_named::<State>("stage-0-0.json").unwrap(),
            Some(State {
                offset: 2,
                checksum: "new".into()
            })
        );
        assert!(Store::open(root.path(), "job").is_err());
        store.remove_named("stage-0-0.json").unwrap();
        assert_eq!(store.load_named::<State>("stage-0-0.json").unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn unix_creation_uses_owner_only_permissions_and_rejects_symlinks() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), "job").unwrap();
        assert_eq!(fs::metadata(store.path()).unwrap().mode() & 0o777, 0o700);
        store
            .save(&State {
                offset: 1,
                checksum: "metadata".into(),
            })
            .unwrap();
        assert_eq!(
            fs::metadata(store.path().join(CHECKPOINT_NAME))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        let path = store.persist_immutable("chunk", b"ciphertext").unwrap();
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);

        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::remove_file(store.path().join(CHECKPOINT_NAME)).unwrap();
        symlink(outside.path(), store.path().join(CHECKPOINT_NAME)).unwrap();
        assert!(store.load::<State>().is_err());
        fs::set_permissions(store.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Store::open(root.path(), "job").is_err());
    }
}
