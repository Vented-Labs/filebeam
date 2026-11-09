use std::{
    collections::{HashMap, HashSet},
    fs::File,
    os::unix::io::{AsRawFd, FromRawFd},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use filebeam_transfer::manifest::Manifest;
use filebeam_transfer_native::{
    device::{DeviceOffer, DeviceReceipt, DeviceReceiptRootOutcome},
    protocol::peer::{PeerReceiveGrant, PeerReceiver},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ApprovalMode, CommitJournal, DestinationGrant, DestinationGrants, StagingDirectory,
    grants::grant_lock,
};

const MAX_PENDING_STAGES: usize = 32;
const MAX_STAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const FREE_SPACE_FLOOR: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
struct StageRecord {
    job_id: String,
    grant_id: Uuid,
    grant_revision: u64,
    staging_name: String,
    #[serde(default)]
    destination_components: Vec<String>,
    roots: Vec<String>,
    chunk_counts: Vec<u64>,
    manifest_digest: String,
}

pub struct DaemonPeerReceiver {
    home: PathBuf,
    receive_enabled: Arc<AtomicBool>,
    stages: Mutex<HashMap<String, Stage>>,
}

struct Stage {
    grant: DestinationGrant,
    staging_name: String,
    destination_components: Vec<String>,
    roots: Vec<String>,
    chunk_counts: Vec<u64>,
    // Keep this descriptor open until the journaled rename. Native receive
    // helpers address targets through it rather than through a mutable path.
    directory: File,
    staging: Arc<StagingDirectory>,
}

#[derive(Serialize, Deserialize)]
struct JournalReceipt {
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

impl JournalReceipt {
    fn from_receipt(receipt: &DeviceReceipt) -> Self {
        Self {
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
        }
    }
    fn into_receipt(self) -> Result<DeviceReceipt> {
        Ok(DeviceReceipt {
            job_id: self.job_id,
            transfer_id: self.transfer_id,
            sender_device_id: self.sender_device_id,
            sender_key_version: self.sender_key_version,
            receiver_device_id: self.receiver_device_id,
            receiver_key_version: self.receiver_key_version,
            manifest_digest: self.manifest_digest,
            grant_id: self.grant_id,
            grant_revision: self.grant_revision,
            commit_id: self.commit_id,
            committed_at_unix_seconds: self.committed_at_unix_seconds,
            items: self.items,
            root_outcomes: self.root_outcomes,
            signature: self
                .signature
                .try_into()
                .map_err(|_| anyhow::anyhow!("journal receipt signature is invalid"))?,
        })
    }
}

fn signed_receipt_bytes(receipt: &DeviceReceipt) -> Result<Vec<u8>> {
    let mut bytes = DeviceReceipt::canonical_bytes(receipt)?;
    bytes.extend_from_slice(&receipt.signature);
    Ok(bytes)
}

impl DaemonPeerReceiver {
    pub fn new(home: PathBuf, receive_enabled: bool) -> Self {
        Self::with_policy(home, Arc::new(AtomicBool::new(receive_enabled)))
    }

    pub fn with_policy(home: PathBuf, receive_enabled: Arc<AtomicBool>) -> Self {
        Self {
            home,
            receive_enabled,
            stages: Mutex::new(HashMap::new()),
        }
    }
}

impl PeerReceiver for DaemonPeerReceiver {
    fn admit(&self, offer: &DeviceOffer, manifest: &Manifest) -> Result<PeerReceiveGrant> {
        if !self.receive_enabled.load(Ordering::Acquire) {
            bail!("device receiving is paused");
        }
        let destination_components =
            requested_subdirectory(&offer.requested_grant_id, &offer.requested_subdirectory)?
                .unwrap_or_default();
        let grants = DestinationGrants::open(&self.home)?;
        let grant = grants.resolve_enabled(&offer.requested_grant_id)?;
        if grant.approval != ApprovalMode::Auto {
            bail!("destination requires local approval");
        }
        if let Some(stage) = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?
            .get(&offer.job_id)
        {
            if stage.grant.id != grant.id
                || stage.grant.revision != grant.revision
                || stage.destination_components != destination_components
                || stage.roots != selected_roots(manifest)?
            {
                bail!("peer job retry does not match its existing staging");
            }
            let base = PathBuf::from("private-peer-stage");
            return Ok(PeerReceiveGrant {
                id: stage.grant.id.to_string(),
                revision: stage.grant.revision,
                staging_base: base.clone(),
                targets: targets(manifest, &stage.directory, &base)?,
                staging: Some(stage.staging.clone()),
            });
        }
        let root = grants
            .open_grant(grant.id, grant.revision)?
            .open_or_create_subdirectory(&destination_components)?;
        let staging_name = format!(".filebeam-stage-{}", offer.job_id);
        let roots = selected_roots(manifest)?;
        let chunk_counts: Vec<_> = manifest.items.iter().map(|item| item.chunk_count).collect();
        let expected_bytes = manifest.items.iter().try_fold(0u64, |total, item| {
            total
                .checked_add(item.size)
                .context("peer receive size overflows")
        })?;
        if expected_bytes > MAX_STAGE_BYTES {
            bail!("peer receive exceeds the local staging limit");
        }
        let record_name = stage_record_name(&offer.job_id);
        let store = crate::daemon_store(&self.home)?;
        let record = StageRecord {
            job_id: offer.job_id.clone(),
            grant_id: grant.id,
            grant_revision: grant.revision,
            staging_name: staging_name.clone(),
            destination_components: destination_components.clone(),
            roots: roots.clone(),
            chunk_counts: chunk_counts.clone(),
            manifest_digest: offer.metadata.manifest_digest.clone(),
        };
        let directory = match store.load_named::<StageRecord>(&record_name)? {
            Some(existing) => {
                if existing.job_id != record.job_id
                    || existing.grant_id != record.grant_id
                    || existing.grant_revision != record.grant_revision
                    || existing.destination_components != record.destination_components
                    || existing.roots != record.roots
                    || existing.chunk_counts != record.chunk_counts
                    || existing.manifest_digest != record.manifest_digest
                {
                    bail!("peer staging record does not match the offered job");
                }
                root.open_directory(&existing.staging_name)?
            }
            None => {
                let stages = self
                    .stages
                    .lock()
                    .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
                if stages.len() >= MAX_PENDING_STAGES {
                    bail!("too many pending peer receives");
                }
                drop(stages);
                let directory = root.create_directory(&staging_name)?;
                store.save_named(&record_name, &record)?;
                directory
            }
        };
        ensure_stage_space(&directory, expected_bytes)?;
        let base = PathBuf::from("private-peer-stage");
        ensure_tree_directories(manifest, &directory)?;
        let targets = targets(manifest, &directory, &base)?;
        let staging = Arc::new(StagingDirectory::new(
            directory.try_clone()?,
            target_components(manifest)?,
        ));
        let mut stages = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
        if stages.contains_key(&offer.job_id) {
            bail!("peer job is already receiving");
        }
        stages.insert(
            offer.job_id.clone(),
            Stage {
                grant: grant.clone(),
                staging_name,
                destination_components,
                roots,
                chunk_counts,
                directory,
                staging: staging.clone(),
            },
        );
        Ok(PeerReceiveGrant {
            id: grant.id.to_string(),
            revision: grant.revision,
            staging_base: base,
            targets,
            staging: Some(staging),
        })
    }

    fn recover_receipt(&self, offer: &DeviceOffer) -> Result<Option<DeviceReceipt>> {
        let store = crate::daemon_store(&self.home)?;
        let Some(record) =
            store.load_named::<crate::CommitRecord>(&format!("commit-{}-0.json", offer.job_id))?
        else {
            return Ok(None);
        };
        if record.state != crate::CommitState::ReceiptPersisted {
            return Ok(None);
        }
        let bytes = store
            .load_secret(&format!("commit-receipt-{}-0", offer.job_id))?
            .context("committed receiver receipt is missing")?;
        let receipt = serde_json::from_slice::<JournalReceipt>(&bytes)?.into_receipt()?;
        if receipt.grant_id != record.grant_id.to_string()
            || receipt.grant_revision != record.grant_revision
        {
            bail!("journal receipt does not match its commit grant")
        }
        Ok(Some(receipt))
    }

    fn commit(
        &self,
        grant: &PeerReceiveGrant,
        job_id: &str,
    ) -> Result<Vec<DeviceReceiptRootOutcome>> {
        if !self.receive_enabled.load(Ordering::Acquire) {
            bail!("destination grant changed before commit");
        }
        let stages = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
        let stage = stages
            .get(job_id)
            .context("peer staging directory is missing")?;
        if stage.grant.id.to_string() != grant.id || stage.grant.revision != grant.revision {
            bail!("peer staging grant does not match receipt");
        }
        let _grant_lock = grant_lock(&self.home)?;
        let root = DestinationGrants::open(&self.home)?
            .open_grant(stage.grant.id, stage.grant.revision)?
            .open_or_create_subdirectory(&stage.destination_components)?;
        let mut outcomes = Vec::with_capacity(stage.roots.len());
        for (index, source_root) in stage.roots.iter().enumerate() {
            let record = CommitJournal::new(&self.home).commit_from_allocated(
                &root,
                &stage.directory,
                &format!("{job_id}-{index}"),
                &stage.grant,
                source_root,
            )?;
            let mut destination_components = stage.destination_components.clone();
            destination_components.push(record.destination_name);
            outcomes.push(DeviceReceiptRootOutcome {
                source_root: source_root.clone(),
                destination_components,
            });
        }
        Ok(outcomes)
    }

    fn persist_receipt(&self, grant: &PeerReceiveGrant, receipt: &DeviceReceipt) -> Result<()> {
        if !self.receive_enabled.load(Ordering::Acquire)
            || receipt.grant_id != grant.id
            || receipt.grant_revision != grant.revision
        {
            bail!("destination grant changed before receipt persistence");
        }
        let mut stages = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
        let stage = stages
            .get(&receipt.job_id)
            .context("peer staging directory is missing")?;
        if stage.grant.id.to_string() != grant.id || stage.grant.revision != grant.revision {
            bail!("peer staging grant does not match receipt");
        }
        let _grant_lock = grant_lock(&self.home)?;
        let root = DestinationGrants::open(&self.home)?
            .open_grant(stage.grant.id, stage.grant.revision)?
            .open_or_create_subdirectory(&stage.destination_components)?;
        let receipt_bytes = serde_json::to_vec(&JournalReceipt::from_receipt(receipt))?;
        let mut expected_outcomes = Vec::with_capacity(stage.roots.len());
        let mut records = Vec::with_capacity(stage.roots.len());
        for (index, source_root) in stage.roots.iter().enumerate() {
            let record = CommitJournal::new(&self.home).commit_from_allocated(
                &root,
                &stage.directory,
                &format!("{}-{index}", receipt.job_id),
                &stage.grant,
                source_root,
            )?;
            let mut destination_components = stage.destination_components.clone();
            destination_components.push(record.destination_name.clone());
            expected_outcomes.push(DeviceReceiptRootOutcome {
                source_root: source_root.clone(),
                destination_components,
            });
            records.push(record);
        }
        if receipt.root_outcomes != expected_outcomes {
            bail!("signed receipt does not match committed root outcomes");
        }
        for record in records {
            let persisted =
                CommitJournal::new(&self.home).persist_commit_receipt(record, &receipt_bytes)?;
            let existing = serde_json::from_slice::<JournalReceipt>(&persisted)?.into_receipt()?;
            if signed_receipt_bytes(&existing)? != signed_receipt_bytes(receipt)? {
                bail!("receiver journal already contains a different signed receipt")
            }
        }
        cleanup_receive_artifacts(&stage.directory, &stage.chunk_counts)?;
        root.remove_directory(&stage.staging_name)?;
        crate::daemon_store(&self.home)?.remove_named(&stage_record_name(&receipt.job_id))?;
        stages.remove(&receipt.job_id);
        Ok(())
    }
}

fn ensure_stage_space(directory: &File, expected_bytes: u64) -> Result<()> {
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatvfs(directory.as_raw_fd(), &mut stats) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let available = u64::try_from(stats.f_bavail)
        .ok()
        .and_then(|blocks| blocks.checked_mul(stats.f_frsize))
        .context("destination free space overflows")?;
    if expected_bytes > available.saturating_sub(FREE_SPACE_FLOOR) {
        bail!("destination does not have enough free space for this peer receive");
    }
    Ok(())
}

fn stage_record_name(job_id: &str) -> String {
    format!("peer-stage-{job_id}.json")
}

fn cleanup_receive_artifacts(directory: &File, chunk_counts: &[u64]) -> Result<()> {
    for (item_index, chunk_count) in chunk_counts.iter().enumerate() {
        remove_stage_file(directory, &format!("plain-{item_index}"))
            .context("remove verified peer plaintext staging")?;
        for chunk_index in 0..*chunk_count {
            remove_stage_file(directory, &format!("cipher-{item_index}-{chunk_index}"))
                .context("remove verified peer ciphertext staging")?;
        }
    }
    Ok(())
}

fn remove_stage_file(directory: &File, name: &str) -> Result<()> {
    let name = std::ffi::CString::new(name)?;
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(error.into())
    }
}

fn selected_roots(manifest: &Manifest) -> Result<Vec<String>> {
    let mut roots = HashSet::new();
    if let Some(tree) = &manifest.tree {
        for entry in &tree.entries {
            let root = entry.path.first().context("tree item path is empty")?;
            if !safe_name(root) {
                bail!("tree has an unsafe selected root");
            }
            roots.insert(root.clone());
        }
    } else {
        for item in &manifest.items {
            if !safe_name(&item.name) {
                bail!("peer manifest has unsafe destination names");
            }
            roots.insert(item.name.clone());
        }
    }
    if roots.is_empty() {
        bail!("peer manifest has no selected roots");
    }
    let mut roots: Vec<_> = roots.into_iter().collect();
    roots.sort();
    Ok(roots)
}

fn ensure_tree_directories(manifest: &Manifest, directory: &File) -> Result<()> {
    let Some(tree) = &manifest.tree else {
        return Ok(());
    };
    for entry in &tree.entries {
        if entry.item_id.is_none() {
            if entry.path.is_empty() {
                bail!("tree directory path is empty");
            }
            ensure_directories(directory, &entry.path)?;
        }
    }
    Ok(())
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

fn requested_subdirectory(
    requested_grant_id: &str,
    requested_subdirectory: &Option<String>,
) -> Result<Option<Vec<String>>> {
    let Some(subdirectory) = requested_subdirectory else {
        return Ok(None);
    };
    Uuid::parse_str(requested_grant_id)
        .context("destination subdirectory requires an explicit grant UUID")?;
    if subdirectory.contains(['\\', '\0']) {
        bail!("destination subdirectory has unsafe components");
    }
    let components = std::path::Path::new(subdirectory)
        .components()
        .map(|component| match component {
            std::path::Component::Normal(component) => component
                .to_str()
                .filter(|component| safe_name(component))
                .map(str::to_owned)
                .context("destination subdirectory has unsafe components"),
            _ => bail!("destination subdirectory must be a relative component path"),
        })
        .collect::<Result<Vec<_>>>()?;
    if components.is_empty() {
        bail!("destination subdirectory must not be empty");
    }
    Ok(Some(components))
}

fn targets(manifest: &Manifest, _directory: &File, base: &std::path::Path) -> Result<Vec<PathBuf>> {
    Ok(target_components(manifest)?
        .into_iter()
        .map(|components| base.join(components.iter().collect::<PathBuf>()))
        .collect())
}

fn target_components(manifest: &Manifest) -> Result<Vec<Vec<String>>> {
    if let Some(tree) = &manifest.tree {
        let files = tree
            .entries
            .iter()
            .filter_map(|entry| entry.item_id.as_ref().map(|id| (id, &entry.path)))
            .collect::<HashMap<_, _>>();
        manifest
            .items
            .iter()
            .map(|item| {
                let path = files.get(&item.id).context("tree omits a manifest item")?;
                if path.is_empty() || path.iter().any(|component| !safe_name(component)) {
                    bail!("tree has unsafe destination components");
                }
                Ok((*path).clone())
            })
            .collect()
    } else {
        let mut names = HashSet::with_capacity(manifest.items.len());
        manifest
            .items
            .iter()
            .map(|item| {
                if !safe_name(&item.name) || !names.insert(item.name.clone()) {
                    bail!("peer manifest has unsafe destination names");
                }
                Ok(vec![item.name.clone()])
            })
            .collect()
    }
}

fn ensure_directories(base: &File, components: &[String]) -> Result<()> {
    let mut directory = base.try_clone()?;
    for component in components {
        if !safe_name(component) {
            bail!("tree has unsafe destination components");
        }
        let name = std::ffi::CString::new(component.as_bytes())?;
        let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
        if result != 0
            && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use filebeam_encryption::{derive_item_key, encrypt_chunk};
    use filebeam_transfer::manifest::{DigestValue, ManifestItem, ManifestTree, ManifestTreeEntry};
    use filebeam_transfer_native::protocol::peer::{
        PeerReceiveItem, publish_item_staged, receive_chunk_staged,
    };
    use sha2::{Digest, Sha256};
    use std::{
        io::{Seek, Write},
        os::unix::fs::PermissionsExt,
    };

    fn flat_manifest() -> Manifest {
        Manifest {
            attached_note: None,
            join_token: None,
            tree: None,
            version: 1,
            items: vec![ManifestItem {
                id: "item-1".into(),
                name: "report.txt".into(),
                mime: "text/plain".into(),
                size: 0,
                nonce_prefix: "AAAAAAAAAAAAAAAAAAAAAA".into(),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: "0".repeat(64),
                },
            }],
        }
    }

    #[test]
    fn selected_roots_preserve_each_selected_top_level_name() {
        let manifest = Manifest {
            attached_note: None,
            join_token: None,
            tree: Some(ManifestTree {
                roots: vec!["photos".into(), "report.txt".into()],
                entries: vec![
                    ManifestTreeEntry {
                        path: vec!["photos".into(), "nested".into()],
                        item_id: None,
                    },
                    ManifestTreeEntry {
                        path: vec!["report.txt".into()],
                        item_id: Some("item-1".into()),
                    },
                ],
            }),
            version: 1,
            items: vec![],
        };
        assert_eq!(
            selected_roots(&manifest).unwrap(),
            vec!["photos".to_owned(), "report.txt".to_owned()]
        );
    }

    #[test]
    fn selected_roots_reject_unsafe_tree_name() {
        let manifest = Manifest {
            attached_note: None,
            join_token: None,
            tree: Some(ManifestTree {
                roots: vec!["../outside".into()],
                entries: vec![ManifestTreeEntry {
                    path: vec!["../outside".into()],
                    item_id: None,
                }],
            }),
            version: 1,
            items: vec![],
        };
        assert!(selected_roots(&manifest).is_err());
    }

    #[test]
    fn restart_reopens_the_durable_stage_for_the_same_offer() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let path = destination.path().join("dropzone");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &path, ApprovalMode::Auto)?;
        let offer = DeviceOffer {
            job_id: "job-1".into(),
            expires_at_unix_seconds: u64::MAX,
            metadata: filebeam_transfer_native::device::DeviceOfferMetadata {
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "opaque".into(),
                items: vec![],
                manifest_digest: "0".repeat(64),
            },
            transfer_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            requested_grant_id: grant.id.to_string(),
            requested_subdirectory: None,
        };
        let first = DaemonPeerReceiver::new(home.path().into(), true);
        let admitted = first.admit(&offer, &flat_manifest())?;
        drop(first);
        let restarted = DaemonPeerReceiver::new(home.path().into(), true);
        let retry = restarted.admit(&offer, &flat_manifest())?;
        assert_eq!(admitted.id, retry.id);
        assert_eq!(admitted.revision, retry.revision);
        assert!(retry.targets[0].ends_with("report.txt"));
        assert_eq!(retry.targets.len(), 1);
        Ok(())
    }

    #[test]
    fn alias_resolves_to_the_local_grant_uuid() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let path = destination.path().join("dropzone");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &path, ApprovalMode::Auto)?;
        let offer = DeviceOffer {
            job_id: "job-alias".into(),
            expires_at_unix_seconds: u64::MAX,
            metadata: filebeam_transfer_native::device::DeviceOfferMetadata {
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "opaque".into(),
                items: vec![],
                manifest_digest: "0".repeat(64),
            },
            transfer_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            requested_grant_id: "dropzone".into(),
            requested_subdirectory: None,
        };

        let first = DaemonPeerReceiver::new(home.path().into(), true);
        let admitted = first.admit(&offer, &flat_manifest())?;
        assert_eq!(admitted.id, grant.id.to_string());
        drop(first);
        let retry =
            DaemonPeerReceiver::new(home.path().into(), true).admit(&offer, &flat_manifest())?;
        assert_eq!(retry.id, grant.id.to_string());
        Ok(())
    }

    #[test]
    fn commits_a_requested_subdirectory_with_a_durable_collision_mapping() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let path = destination.path().join("dropzone");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &path, ApprovalMode::Auto)?;
        std::fs::create_dir(path.join("reports"))?;
        std::fs::set_permissions(path.join("reports"), std::fs::Permissions::from_mode(0o700))?;
        std::fs::write(path.join("reports/report.txt"), b"existing")?;
        let offer = DeviceOffer {
            job_id: "job-subdirectory".into(),
            expires_at_unix_seconds: u64::MAX,
            metadata: filebeam_transfer_native::device::DeviceOfferMetadata {
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "opaque".into(),
                items: vec![],
                manifest_digest: "0".repeat(64),
            },
            transfer_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            requested_grant_id: grant.id.to_string(),
            requested_subdirectory: Some("reports".into()),
        };
        let receiver = DaemonPeerReceiver::new(home.path().into(), true);
        let admitted = receiver.admit(&offer, &flat_manifest())?;
        let mut source = tempfile::NamedTempFile::new()?;
        source.write_all(b"new")?;
        source.as_file_mut().rewind()?;
        admitted
            .staging
            .as_ref()
            .expect("Unix receiver supplies a staging capability")
            .write_target(0, source.as_file_mut())?;

        let outcomes = receiver.commit(&admitted, &offer.job_id)?;
        assert_eq!(std::fs::read(path.join("reports/report.txt"))?, b"existing");
        assert_eq!(std::fs::read(path.join("reports/report.txt (1)"))?, b"new");
        assert_eq!(
            outcomes,
            vec![DeviceReceiptRootOutcome {
                source_root: "report.txt".into(),
                destination_components: vec!["reports".into(), "report.txt (1)".into()],
            }]
        );
        let receipt = DeviceReceipt::sign(
            DeviceReceipt {
                job_id: offer.job_id.clone(),
                transfer_id: offer.metadata.transfer_id.clone(),
                sender_device_id: offer.metadata.sender_device_id.clone(),
                sender_key_version: offer.metadata.sender_key_version,
                receiver_device_id: offer.metadata.receiver_device_id.clone(),
                receiver_key_version: offer.metadata.receiver_key_version,
                manifest_digest: offer.metadata.manifest_digest.clone(),
                grant_id: admitted.id.clone(),
                grant_revision: admitted.revision,
                commit_id: "commit-subdirectory".into(),
                committed_at_unix_seconds: 1,
                items: vec![],
                root_outcomes: outcomes,
                signature: [0; 64],
            },
            &ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        )?;
        receiver.persist_receipt(&admitted, &receipt)?;
        let mut conflicting = receipt.clone();
        conflicting.commit_id = "different-commit".into();
        assert!(receiver.persist_receipt(&admitted, &conflicting).is_err());
        std::fs::write(path.join("reports/report.txt (1)"), b"edited after receipt")?;
        drop(receiver);
        let recovered = DaemonPeerReceiver::new(home.path().into(), false)
            .recover_receipt(&offer)?
            .expect("receiver journal retains its signed receipt");
        assert_eq!(
            signed_receipt_bytes(&recovered)?,
            signed_receipt_bytes(&receipt)?
        );
        assert_eq!(
            std::fs::read(path.join("reports/report.txt (1)"))?,
            b"edited after receipt"
        );
        assert!(
            !path
                .join("reports/.filebeam-stage-job-subdirectory")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn subdirectory_requires_a_uuid_and_safe_components() {
        assert!(requested_subdirectory("dropzone", &Some("reports".into())).is_err());
        let id = Uuid::new_v4().to_string();
        assert!(requested_subdirectory(&id, &Some("../reports".into())).is_err());
        assert!(requested_subdirectory(&id, &Some("/reports".into())).is_err());
        assert_eq!(
            requested_subdirectory(&id, &Some("reports/2026".into())).unwrap(),
            Some(vec!["reports".into(), "2026".into()])
        );
    }

    #[test]
    fn staged_worker_stays_on_its_held_directory_after_a_symlink_swap() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let root = destination.path().join("dropzone");
        let attacker = destination.path().join("attacker");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&attacker)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        std::fs::set_permissions(&attacker, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &root, ApprovalMode::Auto)?;
        let offer = DeviceOffer {
            job_id: "stage-swap".into(),
            expires_at_unix_seconds: u64::MAX,
            metadata: filebeam_transfer_native::device::DeviceOfferMetadata {
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "opaque".into(),
                items: vec![],
                manifest_digest: "0".repeat(64),
            },
            transfer_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            requested_grant_id: grant.id.to_string(),
            requested_subdirectory: None,
        };
        let plain = b"descriptor-only bytes";
        let mut manifest = flat_manifest();
        manifest.items[0].id = "01ARZ3NDEKTSV4RRFFQ69G5FAW".into();
        manifest.items[0].size = plain.len() as u64;
        manifest.items[0].nonce_prefix = URL_SAFE_NO_PAD.encode([9; 16]);
        manifest.items[0].digest.value = hex::encode(Sha256::digest(plain));
        let receiver = DaemonPeerReceiver::new(home.path().into(), true);
        let admitted = receiver.admit(&offer, &manifest)?;
        let held_root = destination.path().join("held-root");
        std::fs::rename(&root, &held_root)?;
        std::fs::create_dir(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        let held = held_root.join("held-stage");
        std::fs::rename(held_root.join(".filebeam-stage-stage-swap"), &held)?;
        std::os::unix::fs::symlink(&attacker, held_root.join(".filebeam-stage-stage-swap"))?;
        let master = [7; 32];
        let item = PeerReceiveItem {
            id: manifest.items[0].id.clone(),
            name: manifest.items[0].name.clone(),
            size: plain.len() as u64,
            nonce_prefix: manifest.items[0].nonce_prefix.clone(),
            chunk_count: 1,
            digest: manifest.items[0].digest.value.clone(),
            target: admitted.targets[0].clone(),
        };
        let key = derive_item_key(&master, &offer.metadata.transfer_id, &item.id)?;
        let ciphertext = encrypt_chunk(
            &key,
            &[9; 16],
            0,
            plain,
            format!("filebeam:v1:{}:{}:0", offer.metadata.transfer_id, item.id).as_bytes(),
        )?;
        let staging = admitted.staging.as_ref().expect("Unix staging capability");
        receive_chunk_staged(
            staging.as_ref(),
            0,
            0,
            1024,
            &offer.metadata.transfer_id,
            &item,
            &master,
            ciphertext,
        )?;
        publish_item_staged(staging.as_ref(), 0, &item)?;
        assert_eq!(std::fs::read(held.join("report.txt"))?, plain);
        assert!(!attacker.join("report.txt").exists());
        assert!(!root.join("report.txt").exists());
        assert!(!admitted.staging_base.exists());
        Ok(())
    }

    #[test]
    fn disabled_policy_rejects_an_admitted_stage_before_commit_or_receipt() -> Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700))?;
        let destination = tempfile::tempdir()?;
        let root = destination.path().join("dropzone");
        std::fs::create_dir(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        let grant =
            DestinationGrants::open(home.path())?.add("dropzone", &root, ApprovalMode::Auto)?;
        let offer = DeviceOffer {
            job_id: "policy-revoke".into(),
            expires_at_unix_seconds: u64::MAX,
            metadata: filebeam_transfer_native::device::DeviceOfferMetadata {
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "opaque".into(),
                items: vec![],
                manifest_digest: "0".repeat(64),
            },
            transfer_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            requested_grant_id: grant.id.to_string(),
            requested_subdirectory: None,
        };
        let policy = Arc::new(AtomicBool::new(true));
        let receiver = DaemonPeerReceiver::with_policy(home.path().into(), policy.clone());
        let admitted = receiver.admit(&offer, &flat_manifest())?;
        policy.store(false, Ordering::Release);
        assert!(receiver.commit(&admitted, &offer.job_id).is_err());
        let receipt = DeviceReceipt::sign(
            DeviceReceipt {
                job_id: offer.job_id.clone(),
                transfer_id: offer.metadata.transfer_id.clone(),
                sender_device_id: offer.metadata.sender_device_id.clone(),
                sender_key_version: 1,
                receiver_device_id: offer.metadata.receiver_device_id.clone(),
                receiver_key_version: 1,
                manifest_digest: offer.metadata.manifest_digest.clone(),
                grant_id: admitted.id.clone(),
                grant_revision: admitted.revision,
                commit_id: "policy-revoke".into(),
                committed_at_unix_seconds: 1,
                items: vec![],
                root_outcomes: vec![],
                signature: [0; 64],
            },
            &ed25519_dalek::SigningKey::from_bytes(&[8; 32]),
        )?;
        assert!(receiver.persist_receipt(&admitted, &receipt).is_err());
        assert!(root.join(".filebeam-stage-policy-revoke").is_dir());
        Ok(())
    }
}
