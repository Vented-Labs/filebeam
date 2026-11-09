use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use filebeam_transfer::manifest::Manifest;
use filebeam_transfer_native::{
    checkpoint::{SecretStore, Store},
    device::{DeviceOffer, DeviceReceipt, DeviceReceiptRootOutcome},
    protocol::peer::PeerReceiveGrant,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RECORD: &str = "dropzone-commit-v1";
const RECEIPT: &str = "dropzone-receipt-v1";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ItemCommitment {
    id: String,
    digest: String,
    bytes: u64,
    chunks: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Identity {
    #[serde(default)]
    device: u64,
    #[serde(default)]
    inode: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    job_id: String,
    manifest_digest: String,
    items: Vec<ItemCommitment>,
    root: Option<Identity>,
    stage_name: String,
    destination_name: String,
    grant_id: String,
    grant_revision: u64,
    /// The object identity captured before the rename makes a later pathname
    /// replacement ambiguous rather than permission to create another copy.
    committed: Option<Identity>,
}

#[derive(Serialize, Deserialize)]
struct StoredReceipt {
    job_id: String,
    transfer_id: String,
    sender_device_id: String,
    sender_key_version: u32,
    receiver_device_id: String,
    receiver_key_version: u32,
    manifest_digest: String,
    grant_id: String,
    grant_revision: u64,
    commit_id: String,
    committed_at_unix_seconds: u64,
    items: Vec<filebeam_transfer_native::device::DeviceReceiptItem>,
    root_outcomes: Vec<DeviceReceiptRootOutcome>,
    signature: Vec<u8>,
}

pub(super) struct Journal {
    root: PathBuf,
    store: Store,
    record: Record,
}

impl Journal {
    pub(super) fn open(
        state_root: &Path,
        secrets: Option<Arc<dyn SecretStore>>,
        root: &Path,
        offer: &DeviceOffer,
        manifest: &Manifest,
    ) -> Result<Self> {
        ensure_private_root(root)?;
        let id = store_id(&offer.job_id);
        let store = open_store(state_root, secrets, &id)?;
        let expected = Record {
            job_id: offer.job_id.clone(),
            manifest_digest: offer.metadata.manifest_digest.clone(),
            items: manifest
                .items
                .iter()
                .map(|item| ItemCommitment {
                    id: item.id.to_string(),
                    digest: format!("{}:{}", item.digest.algorithm, item.digest.value),
                    bytes: item.size,
                    chunks: item.chunk_count,
                })
                .collect(),
            root: identity(root)?,
            stage_name: format!(".filebeam-stage-{}", uuid::Uuid::new_v4()),
            destination_name: offer.job_id.clone(),
            grant_id: uuid::Uuid::new_v4().to_string(),
            grant_revision: 1,
            committed: None,
        };
        let record = match store.load_named::<Record>(RECORD)? {
            Some(record) => {
                if record.job_id != expected.job_id
                    || record.manifest_digest != expected.manifest_digest
                    || record.items != expected.items
                    || record.root != expected.root
                {
                    bail!("peer retry does not match the durable dropzone admission")
                }
                record
            }
            None => {
                let stage = root.join(&expected.stage_name);
                create_private_stage(&stage)?;
                // The stage is visible only after its encrypted binding is durable.
                store.save_named(RECORD, &expected)?;
                expected
            }
        };
        let journal = Self {
            root: root.to_owned(),
            store,
            record,
        };
        journal.reconcile()?;
        Ok(journal)
    }

    pub(super) fn reopen(
        state_root: &Path,
        secrets: Option<Arc<dyn SecretStore>>,
        root: &Path,
        job_id: &str,
    ) -> Result<Self> {
        ensure_private_root(root)?;
        let store = open_store(state_root, secrets, &store_id(job_id))?;
        let record: Record = store
            .load_named(RECORD)?
            .context("dropzone journal is unavailable")?;
        if record.job_id != job_id || record.root != identity(root)? {
            bail!("dropzone journal does not match the current root")
        }
        let journal = Self {
            root: root.to_owned(),
            store,
            record,
        };
        journal.reconcile()?;
        Ok(journal)
    }

    pub(super) fn grant_id(&self) -> &str {
        &self.record.grant_id
    }
    pub(super) fn grant_revision(&self) -> u64 {
        self.record.grant_revision
    }

    pub(super) fn stage_path(&self) -> Result<PathBuf> {
        let path = if self.record.committed.is_some() {
            self.root.join(&self.record.destination_name)
        } else {
            self.root.join(&self.record.stage_name)
        };
        ensure_private_directory(&path)?;
        Ok(path)
    }

    pub(super) fn commit(&self, grant: &PeerReceiveGrant) -> Result<Vec<DeviceReceiptRootOutcome>> {
        self.check_grant(grant)?;
        self.reconcile()?;
        if self.record.committed.is_none() {
            let stage = self.root.join(&self.record.stage_name);
            let destination = self.root.join(&self.record.destination_name);
            let stage_identity =
                identity(&stage)?.context("dropzone stage has no stable identity")?;
            let mut intent = self.record.clone();
            intent.committed = Some(stage_identity);
            self.store.save_named(RECORD, &intent)?;
            rename_no_replace(&stage, &destination)?;
            self.store.save_named(RECORD, &intent)?;
        }
        Ok(vec![DeviceReceiptRootOutcome {
            source_root: "dropzone".into(),
            destination_components: vec![self.record.destination_name.clone()],
        }])
    }

    pub(super) fn persist_receipt(
        &self,
        grant: &PeerReceiveGrant,
        receipt: &DeviceReceipt,
    ) -> Result<()> {
        self.check_grant(grant)?;
        self.reconcile()?;
        if receipt.job_id != self.record.job_id
            || receipt.manifest_digest != self.record.manifest_digest
            || receipt.grant_id != self.record.grant_id
            || receipt.grant_revision != self.record.grant_revision
            || receipt.root_outcomes
                != vec![DeviceReceiptRootOutcome {
                    source_root: "dropzone".into(),
                    destination_components: vec![self.record.destination_name.clone()],
                }]
        {
            bail!("signed receipt does not match the durable dropzone commit")
        }
        // This encrypted historical receipt survives stage cleanup and is written
        // before the core worker can transmit its peer-only Committed message.
        let stored = StoredReceipt {
            job_id: receipt.job_id.clone(),
            transfer_id: receipt.transfer_id.clone(),
            sender_device_id: receipt.sender_device_id.clone(),
            sender_key_version: receipt.sender_key_version,
            receiver_device_id: receipt.receiver_device_id.clone(),
            receiver_key_version: receipt.receiver_key_version,
            manifest_digest: receipt.manifest_digest.clone(),
            grant_id: receipt.grant_id.clone(),
            grant_revision: receipt.grant_revision,
            commit_id: receipt.commit_id.clone(),
            committed_at_unix_seconds: receipt.committed_at_unix_seconds,
            items: receipt.items.clone(),
            root_outcomes: receipt.root_outcomes.clone(),
            signature: receipt.signature.to_vec(),
        };
        if let Some(existing) = self.store.load_named::<StoredReceipt>(RECEIPT)? {
            if receipt_bytes(&existing)? != receipt_bytes(&stored)? {
                bail!("dropzone journal already contains a different signed receipt")
            }
            return Ok(());
        }
        self.store.save_named(RECEIPT, &stored)
    }

    pub(super) fn recover_receipt(&self) -> Result<Option<DeviceReceipt>> {
        self.store
            .load_named::<StoredReceipt>(RECEIPT)?
            .map(stored_receipt)
            .transpose()
    }

    fn check_grant(&self, grant: &PeerReceiveGrant) -> Result<()> {
        if grant.id != self.record.grant_id || grant.revision != self.record.grant_revision {
            bail!("dropzone grant changed before commit")
        }
        Ok(())
    }

    fn reconcile(&self) -> Result<()> {
        let stage = self.root.join(&self.record.stage_name);
        let destination = self.root.join(&self.record.destination_name);
        match &self.record.committed {
            None => match fs::symlink_metadata(&stage) {
                Ok(metadata)
                    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() =>
                {
                    Ok(())
                }
                Ok(_) => bail!("dropzone stage was replaced"),
                Err(error) => Err(error).context("dropzone stage is missing"),
            },
            Some(expected) => {
                let actual =
                    identity(&destination)?.context("committed dropzone object is missing")?;
                if &actual != expected {
                    bail!("committed dropzone object was replaced")
                }
                Ok(())
            }
        }
    }
}

fn stored_receipt(receipt: StoredReceipt) -> Result<DeviceReceipt> {
    Ok(DeviceReceipt {
        job_id: receipt.job_id,
        transfer_id: receipt.transfer_id,
        sender_device_id: receipt.sender_device_id,
        sender_key_version: receipt.sender_key_version,
        receiver_device_id: receipt.receiver_device_id,
        receiver_key_version: receipt.receiver_key_version,
        manifest_digest: receipt.manifest_digest,
        grant_id: receipt.grant_id,
        grant_revision: receipt.grant_revision,
        commit_id: receipt.commit_id,
        committed_at_unix_seconds: receipt.committed_at_unix_seconds,
        items: receipt.items,
        root_outcomes: receipt.root_outcomes,
        signature: receipt
            .signature
            .try_into()
            .map_err(|_| anyhow::anyhow!("dropzone receipt signature is invalid"))?,
    })
}

fn receipt_bytes(receipt: &StoredReceipt) -> Result<Vec<u8>> {
    let receipt = stored_receipt(StoredReceipt {
        job_id: receipt.job_id.clone(),
        transfer_id: receipt.transfer_id.clone(),
        sender_device_id: receipt.sender_device_id.clone(),
        sender_key_version: receipt.sender_key_version,
        receiver_device_id: receipt.receiver_device_id.clone(),
        receiver_key_version: receipt.receiver_key_version,
        manifest_digest: receipt.manifest_digest.clone(),
        grant_id: receipt.grant_id.clone(),
        grant_revision: receipt.grant_revision,
        commit_id: receipt.commit_id.clone(),
        committed_at_unix_seconds: receipt.committed_at_unix_seconds,
        items: receipt.items.clone(),
        root_outcomes: receipt.root_outcomes.clone(),
        signature: receipt.signature.clone(),
    })?;
    let mut bytes = DeviceReceipt::canonical_bytes(&receipt)?;
    bytes.extend_from_slice(&receipt.signature);
    Ok(bytes)
}

fn store_id(job_id: &str) -> String {
    format!(
        "dropzone-journal-{}",
        hex::encode(Sha256::digest(job_id.as_bytes()))
    )
}

fn open_store(root: &Path, secrets: Option<Arc<dyn SecretStore>>, id: &str) -> Result<Store> {
    let exists = match fs::symlink_metadata(root.join(id)) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    match (exists, secrets) {
        (true, Some(secrets)) => Store::open_with_secret_store(root, id, secrets),
        (false, Some(secrets)) => Store::create_with_secret_store(root, id, secrets),
        (true, None) => Store::open(root, id),
        (false, None) => Store::create(root, id),
    }
}

fn create_private_stage(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
            ensure_private_directory(path)
        }
        Err(error) => Err(error).context("create private dropzone stage"),
    }
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("dropzone path is not a private directory")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        if metadata.uid() != unsafe { geteuid() } || metadata.mode() & 0o077 != 0 {
            bail!("dropzone directory is not owner-only")
        }
    }
    Ok(())
}

fn ensure_private_root(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => ensure_private_directory(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
            ensure_private_directory(path)
        }
        Err(error) => Err(error.into()),
    }
}

fn identity(path: &Path) -> Result<Option<Identity>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("dropzone object is not a directory")
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Some(Identity {
            device: metadata.dev(),
            inode: metadata.ino(),
        }))
    }
    #[cfg(not(unix))]
    Ok(None)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_no_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renameat2(oldfd: i32, old: *const i8, newfd: i32, new: *const i8, flags: u32) -> i32;
    }
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    let result = unsafe {
        renameat2(
            -100,
            source.as_ptr().cast(),
            -100,
            destination.as_ptr().cast(),
            1,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn rename_no_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn renamex_np(source: *const i8, destination: *const i8, flags: u32) -> i32;
    }
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // Darwin's RENAME_EXCL is its atomic no-replace publication primitive.
    let result = unsafe { renamex_np(source.as_ptr(), destination.as_ptr(), 0x0000_0004) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
)))]
fn rename_no_replace(_: &Path, _: &Path) -> Result<()> {
    bail!("dropzone no-replace commit is unavailable on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    use filebeam_transfer::manifest::{DigestValue, ManifestItem};
    use filebeam_transfer_native::device::DeviceOfferMetadata;

    fn paths() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("filebeam-dropzone-{}", uuid::Uuid::new_v4()));
        let state = base.join("state");
        let root = base.join("dropzone");
        fs::create_dir_all(&state).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        }
        (state, root)
    }

    fn offer(job_id: &str) -> DeviceOffer {
        DeviceOffer {
            job_id: job_id.into(),
            expires_at_unix_seconds: u64::MAX,
            metadata: DeviceOfferMetadata {
                sender_device_id: "sender".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver".into(),
                receiver_key_version: 1,
                transfer_id: "transfer".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "ciphertext".into(),
                items: vec![],
                manifest_digest: "a".repeat(64),
            },
            transfer_key: "key".into(),
            requested_grant_id: "dropzone".into(),
            requested_subdirectory: None,
        }
    }

    fn manifest() -> Manifest {
        Manifest {
            attached_note: None,
            join_token: None,
            tree: None,
            version: 1,
            items: vec![ManifestItem {
                id: "item".into(),
                name: "item".into(),
                mime: "text/plain".into(),
                size: 3,
                nonce_prefix: "nonce".into(),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: "digest".into(),
                },
            }],
        }
    }

    fn grant(journal: &Journal) -> PeerReceiveGrant {
        PeerReceiveGrant {
            id: journal.grant_id().into(),
            revision: journal.grant_revision(),
            staging_base: journal.stage_path().unwrap(),
            targets: vec![],
            staging: None,
        }
    }

    #[test]
    fn restart_after_rename_reuses_the_committed_root() -> Result<()> {
        let (state, root) = paths();
        let offer = offer("job-a");
        let journal = Journal::open(&state, None, &root, &offer, &manifest())?;
        fs::write(journal.stage_path()?.join("item-0"), b"new")?;
        let grant = grant(&journal);
        let expected = journal.commit(&grant)?;
        drop(journal); // Simulate a process death before the signed receipt is persisted.

        let restarted = Journal::reopen(&state, None, &root, &offer.job_id)?;
        assert_eq!(restarted.commit(&grant)?, expected);
        assert_eq!(fs::read(root.join("job-a").join("item-0"))?, b"new");
        fs::remove_dir_all(state.parent().unwrap())?;
        Ok(())
    }

    #[test]
    fn existing_destination_is_never_clobbered() -> Result<()> {
        let (state, root) = paths();
        let offer = offer("job-b");
        let journal = Journal::open(&state, None, &root, &offer, &manifest())?;
        fs::write(journal.stage_path()?.join("item-0"), b"incoming")?;
        fs::create_dir(root.join("job-b"))?;
        fs::write(root.join("job-b").join("user-file"), b"keep")?;
        let grant = grant(&journal);
        assert!(journal.commit(&grant).is_err());
        assert_eq!(fs::read(root.join("job-b").join("user-file"))?, b"keep");
        fs::remove_dir_all(state.parent().unwrap())?;
        Ok(())
    }

    #[test]
    fn persisted_receipt_remains_historical_after_user_modification() -> Result<()> {
        let (state, root) = paths();
        let offer = offer("job-c");
        let journal = Journal::open(&state, None, &root, &offer, &manifest())?;
        fs::write(journal.stage_path()?.join("item-0"), b"new")?;
        let grant = grant(&journal);
        let outcomes = journal.commit(&grant)?;
        let receipt = DeviceReceipt {
            job_id: offer.job_id.clone(),
            transfer_id: "transfer".into(),
            sender_device_id: "sender".into(),
            sender_key_version: 1,
            receiver_device_id: "receiver".into(),
            receiver_key_version: 1,
            manifest_digest: offer.metadata.manifest_digest.clone(),
            grant_id: grant.id.clone(),
            grant_revision: grant.revision,
            commit_id: "historical-commit".into(),
            committed_at_unix_seconds: 1,
            items: vec![],
            root_outcomes: outcomes,
            signature: [7; 64],
        };
        drop(journal);
        let journal = Journal::reopen(&state, None, &root, &offer.job_id)?;
        journal.persist_receipt(&grant, &receipt)?;
        let mut conflicting = receipt.clone();
        conflicting.commit_id = "different-commit".into();
        assert!(journal.persist_receipt(&grant, &conflicting).is_err());
        fs::write(root.join("job-c").join("item-0"), b"edited")?;
        drop(journal);

        let restarted = Journal::reopen(&state, None, &root, &offer.job_id)?;
        assert_eq!(
            restarted
                .recover_receipt()?
                .expect("historical receipt")
                .commit_id,
            "historical-commit"
        );
        let stored: StoredReceipt = restarted.store.load_named(RECEIPT)?.unwrap();
        assert_eq!(stored.commit_id, "historical-commit");
        assert_eq!(stored.signature, vec![7; 64]);
        assert_eq!(fs::read(root.join("job-c").join("item-0"))?, b"edited");
        fs::remove_dir_all(state.parent().unwrap())?;
        Ok(())
    }
}
