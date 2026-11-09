use std::{
    collections::HashMap,
    fs,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use uuid::Uuid;

use crate::{
    ApprovalMode, DestinationGrant, RootIdentity, WindowsReceiverRoot, daemon_store, write_json,
};

pub struct DestinationGrants {
    home: PathBuf,
    grants: Vec<DestinationGrant>,
    guards: std::sync::Mutex<
        HashMap<Uuid, filebeam_transfer_native::checkpoint::TrustedDirectoryGuard>,
    >,
}

impl DestinationGrants {
    pub fn open(home: &Path) -> Result<Self> {
        let directory = home.join("runtime/daemon");
        fs::create_dir_all(&directory)?;
        let path = directory.join("destinations.json");
        let grants = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("read destination grants")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let registry = Self {
            home: home.into(),
            grants,
            guards: std::sync::Mutex::new(HashMap::new()),
        };
        registry.validate()?;
        Ok(registry)
    }

    pub fn list(&self) -> &[DestinationGrant] {
        &self.grants
    }

    pub fn resolve_enabled(&self, alias_or_id: &str) -> Result<DestinationGrant> {
        let id = Uuid::parse_str(alias_or_id).ok();
        let grant = self
            .grants
            .iter()
            .find(|grant| Some(grant.id) == id || (id.is_none() && grant.alias == alias_or_id))
            .context("destination grant was not found")?;
        if !grant.enabled {
            bail!("destination grant is disabled")
        }
        Ok(grant.clone())
    }

    pub fn add(
        &mut self,
        alias: &str,
        root: &Path,
        approval: ApprovalMode,
    ) -> Result<DestinationGrant> {
        validate_alias(alias)?;
        if self.grants.iter().any(|grant| grant.alias == alias) {
            bail!("destination alias already exists")
        }
        let root = fs::canonicalize(root)
            .with_context(|| format!("resolve destination {}", root.display()))?;
        validate_destination_root(&root, &self.home)?;
        let guard = if approval == ApprovalMode::Auto {
            Some(
                filebeam_transfer_native::checkpoint::guard_trusted_directory(&root)
                    .context("validate automatic destination ACL")?,
            )
        } else {
            None
        };
        let confined = WindowsReceiverRoot::open(&root)?;
        let (volume_serial, file_id) = confined.root_identity();
        let grant = DestinationGrant {
            id: Uuid::new_v4(),
            alias: alias.into(),
            revision: 1,
            root_identity: RootIdentity::Windows {
                volume_serial,
                file_id,
            },
            approval,
            enabled: true,
        };
        daemon_store(&self.home)?.save_secret(&secret_name(grant.id), &encode_path(&root))?;
        self.grants.push(grant.clone());
        self.persist()?;
        if let Some(guard) = guard {
            self.retain_guard(grant.id, guard)?;
        }
        Ok(grant)
    }

    pub fn revoke(&mut self, alias_or_id: &str) -> Result<DestinationGrant> {
        let _lock = grant_lock(&self.home)?;
        let index = self
            .grants
            .iter()
            .position(|grant| grant.alias == alias_or_id || grant.id.to_string() == alias_or_id)
            .context("destination grant was not found")?;
        if self.grants[index].enabled {
            self.grants[index].enabled = false;
            self.grants[index].revision = self.grants[index]
                .revision
                .checked_add(1)
                .context("destination revision overflow")?;
            self.persist()?;
        }
        Ok(self.grants[index].clone())
    }

    pub(crate) fn open_grant(&self, id: Uuid, revision: u64) -> Result<WindowsReceiverRoot> {
        let grant = self
            .grants
            .iter()
            .find(|grant| grant.id == id)
            .context("destination grant was not found")?;
        if !grant.enabled || grant.revision != revision {
            bail!("destination grant changed or is disabled")
        }
        let bytes = daemon_store(&self.home)?
            .load_secret(&secret_name(id))?
            .context("destination root reference is missing")?;
        let path = decode_path(&bytes)?;
        let guard = filebeam_transfer_native::checkpoint::guard_trusted_directory(&path)
            .context("validate destination ACL")?;
        let root = WindowsReceiverRoot::open(&path)?;
        let (volume_serial, file_id) = root.root_identity();
        if grant.root_identity
            != (RootIdentity::Windows {
                volume_serial,
                file_id,
            })
        {
            bail!("destination changed since it was granted")
        }
        self.retain_guard(id, guard)?;
        Ok(root)
    }

    fn persist(&self) -> Result<()> {
        write_json(
            &self.home.join("runtime/daemon/destinations.json"),
            &self.grants,
        )
    }

    fn retain_guard(
        &self,
        id: Uuid,
        guard: filebeam_transfer_native::checkpoint::TrustedDirectoryGuard,
    ) -> Result<()> {
        self.guards
            .lock()
            .map_err(|_| anyhow::anyhow!("destination guard registry is poisoned"))?
            .insert(id, guard);
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        for (index, grant) in self.grants.iter().enumerate() {
            validate_alias(&grant.alias)?;
            if grant.revision == 0
                || self.grants[..index]
                    .iter()
                    .any(|other| other.alias == grant.alias)
            {
                bail!("destination grant registry is invalid")
            }
        }
        Ok(())
    }
}

pub(crate) fn grant_lock(home: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(home.join("runtime/daemon/grants.lock"))?;
    file.lock_exclusive()?;
    Ok(file)
}

fn validate_alias(alias: &str) -> Result<()> {
    if alias.is_empty()
        || alias.len() > 64
        || !alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!("destination aliases must be 1-64 ASCII letters, digits, - or _")
    }
    Ok(())
}
fn validate_destination_root(root: &Path, profile: &Path) -> Result<()> {
    if root.parent().is_none() {
        bail!("destination must not be a filesystem root")
    }
    let profile = fs::canonicalize(profile).context("resolve Filebeam profile")?;
    let root_identity = WindowsReceiverRoot::open(root)?.root_identity();
    let profile_identity = WindowsReceiverRoot::open(&profile)?.root_identity();
    // Canonical paths identify containment, but an alias can name the profile
    // itself without comparing equal as a path. The security decision for that
    // case must use the opened directory object's identity.
    if root_identity == profile_identity || root.starts_with(&profile) || profile.starts_with(root)
    {
        bail!("destination must be outside Filebeam state")
    }
    if let Some(home) = dirs::home_dir() {
        let home = fs::canonicalize(home).context("resolve Windows user home")?;
        if root_identity == WindowsReceiverRoot::open(&home)?.root_identity() {
            bail!("destination must not be the Windows user home")
        }
    }
    Ok(())
}
fn secret_name(id: Uuid) -> String {
    format!("destination-root-{id}")
}
fn encode_path(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
fn decode_path(bytes: &[u8]) -> Result<PathBuf> {
    let chunks = bytes.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        bail!("destination root reference is invalid")
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &chunks
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    )))
}
