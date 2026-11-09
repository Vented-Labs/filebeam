use std::{
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use uuid::Uuid;

use crate::{
    ApprovalMode, ConfinedRoot, DestinationGrant, RootIdentity, daemon_store, secure_directory,
    write_json,
};

pub struct DestinationGrants {
    home: PathBuf,
    grants: Vec<DestinationGrant>,
}

impl DestinationGrants {
    pub fn open(home: &Path) -> Result<Self> {
        let directory = home.join("runtime/daemon");
        secure_directory(&directory)?;
        let path = directory.join("destinations.json");
        let grants = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("read destination grants")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let registry = Self {
            home: home.into(),
            grants,
        };
        registry.validate()?;
        Ok(registry)
    }

    pub fn list(&self) -> &[DestinationGrant] {
        &self.grants
    }

    /// Resolves the sender's local-only destination reference. Aliases are
    /// receiver policy names, never filesystem paths or a fallback target.
    pub fn resolve_enabled(&self, alias_or_id: &str) -> Result<DestinationGrant> {
        let id = Uuid::parse_str(alias_or_id).ok();
        let grant = self
            .grants
            .iter()
            .find(|grant| Some(grant.id) == id || (id.is_none() && grant.alias == alias_or_id))
            .context("destination grant was not found")?;
        if !grant.enabled {
            bail!("destination grant is disabled");
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
            bail!("destination alias already exists");
        }
        let root = fs::canonicalize(root)
            .with_context(|| format!("resolve destination {}", root.display()))?;
        let confined = ConfinedRoot::open(&root, &self.home)?;
        let (device, inode) = confined.identity();
        let grant = DestinationGrant {
            id: Uuid::new_v4(),
            alias: alias.into(),
            revision: 1,
            root_identity: RootIdentity::Unix { device, inode },
            approval,
            enabled: true,
        };
        daemon_store(&self.home)?
            .save_secret(&secret_name(grant.id), root.as_os_str().as_encoded_bytes())?;
        self.grants.push(grant.clone());
        self.persist()?;
        Ok(grant)
    }

    pub fn revoke(&mut self, alias_or_id: &str) -> Result<DestinationGrant> {
        let _lock = grant_lock(&self.home)?;
        let index = self
            .grants
            .iter()
            .position(|grant| grant.alias == alias_or_id || grant.id.to_string() == alias_or_id)
            .context("destination grant was not found")?;
        let changed = {
            let grant = &mut self.grants[index];
            if grant.enabled {
                grant.enabled = false;
                grant.revision = grant
                    .revision
                    .checked_add(1)
                    .context("destination revision overflow")?;
                true
            } else {
                false
            }
        };
        if changed {
            self.persist()?;
        }
        Ok(self.grants[index].clone())
    }

    pub fn open_grant(&self, id: Uuid, revision: u64) -> Result<ConfinedRoot> {
        let grant = self
            .grants
            .iter()
            .find(|grant| grant.id == id)
            .context("destination grant was not found")?;
        if !grant.enabled || grant.revision != revision {
            bail!("destination grant changed or is disabled");
        }
        let path = daemon_store(&self.home)?
            .load_secret(&secret_name(id))?
            .context("destination root reference is missing")?;
        let path = PathBuf::from(std::ffi::OsString::from_vec(path));
        let root = ConfinedRoot::open(&path, &self.home)?;
        let (device, inode) = root.identity();
        if grant.root_identity != (RootIdentity::Unix { device, inode }) {
            bail!("destination changed since it was granted");
        }
        Ok(root)
    }

    fn persist(&self) -> Result<()> {
        write_json(
            &self.home.join("runtime/daemon/destinations.json"),
            &self.grants,
        )
    }

    fn validate(&self) -> Result<()> {
        for (index, grant) in self.grants.iter().enumerate() {
            validate_alias(&grant.alias)?;
            if grant.revision == 0
                || self.grants[..index]
                    .iter()
                    .any(|other| other.alias == grant.alias)
            {
                bail!("destination grant registry is invalid");
            }
        }
        Ok(())
    }
}

/// Serializes destination mutation with the receiver's final identity check and
/// no-replace rename. A completed revoke therefore always precedes or follows
/// a commit; it cannot be observed in the middle of one.
pub(crate) fn grant_lock(home: &Path) -> Result<fs::File> {
    let path = home.join("runtime/daemon/grants.lock");
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
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
        bail!("destination aliases must be 1-64 ASCII letters, digits, - or _");
    }
    Ok(())
}

fn secret_name(id: Uuid) -> String {
    format!("destination-root-{id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn resolves_an_enabled_alias_but_never_falls_back() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let path = destination.path().join("dropzone");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &path, ApprovalMode::Auto)?;

        let grants = DestinationGrants::open(home.path())?;
        assert_eq!(grants.resolve_enabled("dropzone")?.id, grant.id);
        assert!(grants.resolve_enabled("missing").is_err());
        assert!(grants.resolve_enabled("/tmp/dropzone").is_err());
        Ok(())
    }

    #[test]
    fn disabled_grant_cannot_be_resolved_by_alias_or_uuid() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let path = destination.path().join("dropzone");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let mut grants = DestinationGrants::open(home.path())?;
        let grant = grants.add("dropzone", &path, ApprovalMode::Auto)?;
        grants.revoke("dropzone")?;

        let grants = DestinationGrants::open(home.path())?;
        assert!(grants.resolve_enabled("dropzone").is_err());
        assert!(grants.resolve_enabled(&grant.id.to_string()).is_err());
        Ok(())
    }

    #[test]
    fn rejects_a_replaced_grant_root() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let path = destination.path().join("dropzone");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &path, ApprovalMode::Auto)?;
        std::fs::rename(&path, destination.path().join("old-dropzone"))?;
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;

        assert!(
            DestinationGrants::open(home.path())?
                .open_grant(grant.id, grant.revision)
                .is_err()
        );
        Ok(())
    }
}
