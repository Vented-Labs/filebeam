//! Encrypted, origin-scoped native account state shared by command-line hosts.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use filebeam_transfer_native::checkpoint::Store;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::{ServiceClient, export_self_key, import_self_key};

#[derive(Serialize, Deserialize)]
struct Session {
    origin: String,
    cookies: String,
    #[serde(default)]
    account_id: u64,
}
#[derive(Serialize, Deserialize)]
struct PrivateKey {
    value: String,
}
#[derive(Serialize, Deserialize)]
struct PendingReceivingKey {
    id: String,
    private_key: String,
    public_key: String,
    fingerprint: String,
    backup_exported: bool,
    replace: bool,
}

pub struct PendingKey {
    pub id: String,
    pub private_key: Zeroizing<Vec<u8>>,
    pub public_key: String,
    pub fingerprint: String,
    pub backup_exported: bool,
    pub replace: bool,
}

/// Native encrypted state rooted below a single Filebeam home. Every record is
/// additionally bound to its canonical service origin before it is returned.
pub struct LocalState {
    home: PathBuf,
    origin: String,
}

impl LocalState {
    pub fn new(home: impl Into<PathBuf>, origin: &str) -> Result<Self> {
        let mut parsed = Url::parse(origin).context("instance URL is invalid")?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            bail!("instance must be an HTTP origin");
        }
        parsed.set_path("");
        Ok(Self {
            home: home.into(),
            origin: parsed.into(),
        })
    }
    pub fn load_client(&self) -> Result<ServiceClient> {
        let session = match self.open(false)? {
            Some(store) => store.load_named::<Session>("session")?,
            None => None,
        };
        let cookies = session
            .filter(|value| value.origin == self.origin)
            .map(|value| value.cookies);
        ServiceClient::new_with_cookie_context(&self.origin, cookies.as_deref())
    }
    pub fn save_session(&self, client: &ServiceClient) -> Result<()> {
        let account_id = client.account().session()?.id;
        let cookies = client
            .cookie_context()
            .context("login did not return a session cookie")?;
        self.store()?.save_named(
            "session",
            &Session {
                origin: self.origin.clone(),
                cookies,
                account_id,
            },
        )
    }
    pub fn clear_session(&self) -> Result<()> {
        if let Some(store) = self.open(false)? {
            store.remove_named("session")?;
        }
        Ok(())
    }
    pub fn cached_account_id(&self) -> Result<Option<u64>> {
        Ok(self
            .open(false)?
            .and_then(|store| store.load_named::<Session>("session").transpose())
            .transpose()?
            .filter(|session| session.origin == self.origin && session.account_id != 0)
            .map(|session| session.account_id))
    }
    pub fn load_private_key(&self) -> Result<Zeroizing<Vec<u8>>> {
        let key = self
            .open(false)?
            .context("no locally stored receiving key")?
            .load_named::<PrivateKey>("private-key")?
            .context("no locally stored receiving key")?;
        import_self_key(&key.value)
    }
    pub fn save_private_key(&self, key: &[u8]) -> Result<()> {
        self.store()?.save_named(
            "private-key",
            &PrivateKey {
                value: export_self_key(key)?,
            },
        )
    }
    pub fn clear_private_key(&self) -> Result<()> {
        if let Some(store) = self.open(false)? {
            store.remove_named("private-key")?;
        }
        Ok(())
    }
    pub fn load_pending_key(&self) -> Result<Option<PendingKey>> {
        let Some(pending) = self
            .open(false)?
            .and_then(|store| {
                store
                    .load_named::<PendingReceivingKey>("pending-receiving-key")
                    .transpose()
            })
            .transpose()?
        else {
            return Ok(None);
        };
        Ok(Some(PendingKey {
            id: pending.id,
            private_key: import_self_key(&pending.private_key)?,
            public_key: pending.public_key,
            fingerprint: pending.fingerprint,
            backup_exported: pending.backup_exported,
            replace: pending.replace,
        }))
    }
    pub fn save_pending_key(&self, key: &PendingKey) -> Result<()> {
        self.store()?.save_named(
            "pending-receiving-key",
            &PendingReceivingKey {
                id: key.id.clone(),
                private_key: export_self_key(&key.private_key)?,
                public_key: key.public_key.clone(),
                fingerprint: key.fingerprint.clone(),
                backup_exported: key.backup_exported,
                replace: key.replace,
            },
        )
    }
    pub fn mark_pending_key_exported(&self, id: &str) -> Result<()> {
        let mut key = self
            .load_pending_key()?
            .context("no pending receiving key")?;
        if key.id != id {
            bail!("pending receiving key does not match export");
        }
        key.backup_exported = true;
        self.save_pending_key(&key)
    }
    pub fn clear_pending_key(&self) -> Result<()> {
        if let Some(store) = self.open(false)? {
            store.remove_named("pending-receiving-key")?;
        }
        Ok(())
    }
    fn id(&self) -> String {
        format!(
            "account-{}",
            Sha256::digest(self.origin.as_bytes())
                .iter()
                .take(12)
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }
    fn root(&self) -> PathBuf {
        self.home.join("accounts")
    }
    fn open(&self, create: bool) -> Result<Option<Store>> {
        let root = self.root();
        let id = self.id();
        if create {
            Ok(Some(Store::create(&root, &id)?))
        } else if root.join(&id).exists() {
            Ok(Some(Store::open(&root, &id)?))
        } else {
            Ok(None)
        }
    }
    fn store(&self) -> Result<Store> {
        match self.open(false)? {
            Some(store) => Ok(store),
            None => self.open(true)?.context("create local account state"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_key_survives_reopen_and_is_origin_scoped() {
        let home = std::env::temp_dir().join(format!(
            "filebeam-local-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&home).unwrap();
        let first = LocalState::new(&home, "https://one.example").unwrap();
        first.save_private_key(&[7; 32]).unwrap();
        assert_eq!(
            LocalState::new(&home, "https://one.example")
                .unwrap()
                .load_private_key()
                .unwrap()
                .as_slice(),
            &[7; 32]
        );
        assert!(
            LocalState::new(&home, "https://two.example")
                .unwrap()
                .load_private_key()
                .is_err()
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
