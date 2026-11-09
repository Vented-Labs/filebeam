//! Windows receiver integration seam.
//!
//! This module deliberately does not implement a second transfer loop. The
//! eventual platform receiver must implement the shared peer verifier and use
//! `WindowsReceiverRoot` for all filesystem effects.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::{Path, PathBuf},
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

use super::{PlatformRoot, StagedObjectIdentity};
use crate::{ApprovalMode, DestinationGrant, DestinationGrants, daemon_store, grants::grant_lock};

/// Marker for the existing shared nonce, grant, manifest, and receipt
/// verification contract. Windows adds filesystem capability only.
pub(crate) trait SharedPeerVerification: PeerReceiver {}

impl<T: PeerReceiver + ?Sized> SharedPeerVerification for T {}

/// Handle-oriented root operations consumed by the shared receiver once its
/// platform dispatch is wired. No operation accepts a destination path.
pub(crate) struct WindowsReceiverRoot {
    root: PlatformRoot,
}

const MAX_PENDING_STAGES: usize = 32;
const MAX_STAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
struct StageRecord {
    job_id: String,
    #[serde(default)]
    requested_grant_id: String,
    grant_id: Uuid,
    grant_revision: u64,
    staging_name: String,
    destination_components: Vec<String>,
    roots: Vec<String>,
    chunk_counts: Vec<u64>,
    manifest_digest: String,
}
struct Stage {
    grant: DestinationGrant,
    requested_grant_id: String,
    root: WindowsReceiverRoot,
    destination: PlatformRoot,
    stage: PlatformRoot,
    staging_name: String,
    destination_components: Vec<String>,
    roots: Vec<String>,
    chunk_counts: Vec<u64>,
    manifest_digest: String,
    guards: Vec<PlatformRoot>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
enum CommitState {
    Intent,
    Renamed,
    ReceiptPersisted,
}
#[derive(Clone, Serialize, Deserialize)]
struct CommitRecord {
    job_id: String,
    grant_id: Uuid,
    grant_revision: u64,
    manifest_digest: String,
    destination_components: Vec<String>,
    source_name: String,
    destination_name: String,
    identity: StagedObjectIdentity,
    state: CommitState,
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
    fn from_receipt(value: &DeviceReceipt) -> Self {
        Self {
            job_id: value.job_id.clone(),
            transfer_id: value.transfer_id.clone(),
            sender_device_id: value.sender_device_id.clone(),
            sender_key_version: value.sender_key_version,
            receiver_device_id: value.receiver_device_id.clone(),
            receiver_key_version: value.receiver_key_version,
            manifest_digest: value.manifest_digest.clone(),
            grant_id: value.grant_id.clone(),
            grant_revision: value.grant_revision,
            commit_id: value.commit_id.clone(),
            committed_at_unix_seconds: value.committed_at_unix_seconds,
            items: value.items.clone(),
            root_outcomes: value.root_outcomes.clone(),
            signature: value.signature.to_vec(),
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

/// The Windows implementation delegates protocol verification to the existing
/// PeerReceiver worker and supplies only confined staging and publication.
pub(crate) struct DaemonPeerReceiver {
    home: PathBuf,
    receive_enabled: Arc<AtomicBool>,
    stages: Mutex<HashMap<String, Stage>>,
}

impl DaemonPeerReceiver {
    pub(crate) fn new(home: PathBuf, receive_enabled: bool) -> Self {
        Self::with_policy(home, Arc::new(AtomicBool::new(receive_enabled)))
    }

    pub(crate) fn with_policy(home: PathBuf, receive_enabled: Arc<AtomicBool>) -> Self {
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
            bail!("device receiving is paused")
        }
        let components =
            requested_subdirectory(&offer.requested_grant_id, &offer.requested_subdirectory)?
                .unwrap_or_default();
        let grants = DestinationGrants::open(&self.home)?;
        let grant = grants.resolve_enabled(&offer.requested_grant_id)?;
        if grant.approval != ApprovalMode::Auto {
            bail!("destination requires local approval")
        }
        let roots = selected_roots(manifest)?;
        let record_name = stage_record_name(&offer.job_id);
        let record = StageRecord {
            job_id: offer.job_id.clone(),
            requested_grant_id: offer.requested_grant_id.clone(),
            grant_id: grant.id,
            grant_revision: grant.revision,
            staging_name: format!(".filebeam-stage-{}", offer.job_id),
            destination_components: components.clone(),
            roots: roots.clone(),
            chunk_counts: manifest.items.iter().map(|item| item.chunk_count).collect(),
            manifest_digest: offer.metadata.manifest_digest.clone(),
        };
        let expected = manifest.items.iter().try_fold(0u64, |sum, item| {
            sum.checked_add(item.size)
                .context("peer receive size overflows")
        })?;
        if expected > MAX_STAGE_BYTES {
            bail!("peer receive exceeds the local staging limit")
        }
        let mut stages = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
        if let Some(stage) = stages.get(&offer.job_id) {
            return peer_grant(&stage.grant, &stage.stage, manifest);
        }
        if stages.len() >= MAX_PENDING_STAGES {
            bail!("too many pending peer receives")
        }
        let store = daemon_store(&self.home)?;
        let existing = store.load_named::<StageRecord>(&record_name)?;
        if let Some(existing) = &existing {
            if existing.job_id != record.job_id
                || !stage_matches_offer(existing, offer)?
                || existing.grant_id != record.grant_id
                || existing.grant_revision != record.grant_revision
                || existing.destination_components != record.destination_components
                || existing.roots != record.roots
                || existing.manifest_digest != record.manifest_digest
            {
                bail!("peer staging record does not match the offered job")
            }
        }
        let root = grants.open_grant(grant.id, grant.revision)?;
        let mut destination = root.destination_root()?;
        let mut guards = Vec::new();
        for component in &components {
            let child = destination.stage_child(component)?;
            destination = child.try_clone()?;
            guards.push(child);
        }
        let stage = match existing {
            Some(existing) => {
                reconcile_before_prepare(&store, &existing, &destination)?;
                destination.open_stage(&existing.staging_name)?
            }
            None => {
                let stage = destination.create_stage(&record.staging_name)?;
                store.save_named(&record_name, &record)?;
                stage
            }
        };
        let mut stage_guards = tree_guards(&stage, manifest)?;
        guards.append(&mut stage_guards);
        let grant_result = peer_grant(&grant, &stage, manifest)?;
        stages.insert(
            offer.job_id.clone(),
            Stage {
                grant,
                requested_grant_id: record.requested_grant_id,
                root,
                destination,
                stage,
                staging_name: record.staging_name,
                destination_components: components,
                roots,
                chunk_counts: record.chunk_counts,
                manifest_digest: record.manifest_digest,
                guards,
            },
        );
        Ok(grant_result)
    }

    fn recover_receipt(&self, offer: &DeviceOffer) -> Result<Option<DeviceReceipt>> {
        let store = daemon_store(&self.home)?;
        let Some(stage) = store.load_named::<StageRecord>(&stage_record_name(&offer.job_id))?
        else {
            return Ok(None);
        };
        if !stage_matches_offer(&stage, offer)? {
            bail!("peer commit journal does not match the offered job")
        }
        let mut records = commit_records(&store, &stage)?;
        let Some(bytes) = store.load_secret(&format!("windows-commit-receipt-{}", offer.job_id))?
        else {
            return Ok(None);
        };
        let receipt = serde_json::from_slice::<JournalReceipt>(&bytes)?.into_receipt()?;
        validate_receipt(&receipt, offer, &stage, &records)?;
        // A receipt write is the durable point of no return. Finish journal
        // advancement after a crash here rather than asking the signer again.
        for record in &mut records {
            if record.state == CommitState::Intent {
                bail!("receipt exists before every committed root was renamed")
            }
            if record.state != CommitState::ReceiptPersisted {
                record.state = CommitState::ReceiptPersisted;
                store.save_named(&commit_record_name(&record.job_id), record)?;
            }
        }
        Ok(Some(receipt))
    }

    fn commit(
        &self,
        grant: &PeerReceiveGrant,
        job_id: &str,
    ) -> Result<Vec<DeviceReceiptRootOutcome>> {
        if !self.receive_enabled.load(Ordering::Acquire) {
            bail!("destination grant changed before commit")
        }
        let stages = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
        let stage = stages
            .get(job_id)
            .context("peer staging directory is missing")?;
        if stage.grant.id.to_string() != grant.id || stage.grant.revision != grant.revision {
            bail!("peer staging grant does not match receipt")
        }
        let _lock = grant_lock(&self.home)?;
        let current = DestinationGrants::open(&self.home)?
            .open_grant(stage.grant.id, stage.grant.revision)?;
        if current.root_identity() != stage.root.root_identity() {
            bail!("destination changed since staging")
        }
        let mut outcomes = Vec::new();
        for (index, source) in stage.roots.iter().enumerate() {
            let destination_name = commit_root(
                &self.home,
                &stage.destination,
                &stage.stage,
                &stage.grant,
                &format!("{job_id}-{index}"),
                source,
                &stage.destination_components,
                &stage.manifest_digest,
            )?;
            outcomes.push(DeviceReceiptRootOutcome {
                source_root: source.clone(),
                destination_components: stage
                    .destination_components
                    .iter()
                    .cloned()
                    .chain([destination_name])
                    .collect(),
            });
        }
        Ok(outcomes)
    }

    fn persist_receipt(&self, grant: &PeerReceiveGrant, receipt: &DeviceReceipt) -> Result<()> {
        let mut stages = self
            .stages
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver state is poisoned"))?;
        let stage = stages
            .get(&receipt.job_id)
            .context("peer staging directory is missing")?;
        if !self.receive_enabled.load(Ordering::Acquire)
            || stage.grant.id.to_string() != grant.id
            || stage.grant.revision != grant.revision
        {
            bail!("destination grant changed before receipt persistence")
        }
        let store = daemon_store(&self.home)?;
        let stage_record = StageRecord {
            job_id: receipt.job_id.clone(),
            requested_grant_id: stage.requested_grant_id.clone(),
            grant_id: stage.grant.id,
            grant_revision: stage.grant.revision,
            staging_name: stage.staging_name.clone(),
            destination_components: stage.destination_components.clone(),
            roots: stage.roots.clone(),
            chunk_counts: stage.chunk_counts.clone(),
            manifest_digest: stage.manifest_digest.clone(),
        };
        let mut records = commit_records(&store, &stage_record)?;
        if receipt.manifest_digest != stage.manifest_digest
            || receipt.grant_id != stage.grant.id.to_string()
            || receipt.grant_revision != stage.grant.revision
        {
            bail!("signed receipt does not match the staged grant")
        }
        if receipt.root_outcomes != expected_outcomes(&stage.destination_components, &records) {
            bail!("signed receipt does not match committed root outcomes")
        }
        if records.iter().any(|record| {
            record.state != CommitState::Renamed && record.state != CommitState::ReceiptPersisted
        }) {
            bail!("receipt persistence was requested before every root was renamed")
        }
        let receipt_name = format!("windows-commit-receipt-{}", receipt.job_id);
        let proposed = signed_receipt_bytes(receipt)?;
        if let Some(bytes) = store.load_secret(&receipt_name)? {
            let existing = serde_json::from_slice::<JournalReceipt>(&bytes)?.into_receipt()?;
            if signed_receipt_bytes(&existing)? != proposed {
                bail!("receiver journal already contains a different signed receipt")
            }
        } else {
            store.save_secret(
                &receipt_name,
                &serde_json::to_vec(&JournalReceipt::from_receipt(receipt))?,
            )?;
        }
        for record in &mut records {
            if record.state != CommitState::ReceiptPersisted {
                record.state = CommitState::ReceiptPersisted;
                store.save_named(&commit_record_name(&record.job_id), record)?;
            }
        }
        cleanup_receive_artifacts(&stage.stage, &stage.chunk_counts)?;
        stage.destination.remove_stage(&stage.staging_name)?;
        stages.remove(&receipt.job_id);
        Ok(())
    }
}

fn peer_grant(
    grant: &DestinationGrant,
    stage: &PlatformRoot,
    manifest: &Manifest,
) -> Result<PeerReceiveGrant> {
    let base = stage.tethered_path()?;
    Ok(PeerReceiveGrant {
        id: grant.id.to_string(),
        revision: grant.revision,
        targets: targets(manifest, &base)?,
        staging_base: base,
        staging: None,
    })
}
fn tree_guards(stage: &PlatformRoot, manifest: &Manifest) -> Result<Vec<PlatformRoot>> {
    let mut guards = Vec::new();
    if let Some(tree) = &manifest.tree {
        for entry in &tree.entries {
            let mut parent = stage.try_clone()?;
            for component in &entry.path[..entry
                .path
                .len()
                .saturating_sub(usize::from(entry.item_id.is_some()))]
            {
                let child = parent.stage_child(component)?;
                parent = child.try_clone()?;
                guards.push(child);
            }
        }
    }
    Ok(guards)
}
fn targets(manifest: &Manifest, base: &Path) -> Result<Vec<PathBuf>> {
    if let Some(tree) = &manifest.tree {
        let paths = tree
            .entries
            .iter()
            .filter_map(|entry| entry.item_id.as_ref().map(|id| (id, &entry.path)))
            .collect::<HashMap<_, _>>();
        manifest
            .items
            .iter()
            .map(|item| {
                let path = paths.get(&item.id).context("tree omits a manifest item")?;
                if path.is_empty()
                    || path.iter().any(|name| {
                        !safe_name(name) || PlatformRoot::validate_component(name).is_err()
                    })
                {
                    bail!("tree has unsafe destination components")
                }
                Ok(base.join(path.iter().collect::<PathBuf>()))
            })
            .collect()
    } else {
        let mut names = HashSet::new();
        manifest
            .items
            .iter()
            .map(|item| {
                if !safe_name(&item.name)
                    || PlatformRoot::validate_component(&item.name).is_err()
                    || !names.insert(item.name.clone())
                {
                    bail!("peer manifest has unsafe destination names")
                }
                Ok(base.join(&item.name))
            })
            .collect()
    }
}
fn selected_roots(manifest: &Manifest) -> Result<Vec<String>> {
    let mut roots = HashSet::new();
    if let Some(tree) = &manifest.tree {
        for entry in &tree.entries {
            roots.insert(
                entry
                    .path
                    .first()
                    .context("tree item path is empty")?
                    .clone(),
            );
        }
    } else {
        for item in &manifest.items {
            roots.insert(item.name.clone());
        }
    }
    if roots.is_empty()
        || roots
            .iter()
            .any(|name| !safe_name(name) || PlatformRoot::validate_component(name).is_err())
    {
        bail!("peer manifest has unsafe destination names")
    }
    let mut roots: Vec<_> = roots.into_iter().collect();
    roots.sort();
    Ok(roots)
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}
fn requested_subdirectory(id: &str, value: &Option<String>) -> Result<Option<Vec<String>>> {
    let Some(value) = value else { return Ok(None) };
    Uuid::parse_str(id).context("destination subdirectory requires an explicit grant UUID")?;
    let components = value
        .split('/')
        .map(|component| {
            if safe_name(component) && PlatformRoot::validate_component(component).is_ok() {
                Ok(component.into())
            } else {
                bail!("destination subdirectory has unsafe components")
            }
        })
        .collect::<Result<Vec<_>>>()?;
    if components.is_empty() {
        bail!("destination subdirectory must not be empty")
    }
    Ok(Some(components))
}
fn stage_record_name(job_id: &str) -> String {
    format!("peer-stage-{job_id}.json")
}
fn commit_record_name(job_id: &str) -> String {
    format!("windows-commit-{job_id}.json")
}
fn signed_receipt_bytes(receipt: &DeviceReceipt) -> Result<Vec<u8>> {
    let mut bytes = DeviceReceipt::canonical_bytes(receipt)?;
    bytes.extend_from_slice(&receipt.signature);
    Ok(bytes)
}
fn stage_matches_offer(stage: &StageRecord, offer: &DeviceOffer) -> Result<bool> {
    Ok(stage.job_id == offer.job_id
        && if stage.requested_grant_id.is_empty() {
            stage.grant_id.to_string() == offer.requested_grant_id
        } else {
            stage.requested_grant_id == offer.requested_grant_id
        }
        && stage.manifest_digest == offer.metadata.manifest_digest
        && requested_subdirectory(&offer.requested_grant_id, &offer.requested_subdirectory)?
            .unwrap_or_default()
            == stage.destination_components)
}
fn commit_records(
    store: &filebeam_transfer_native::checkpoint::Store,
    stage: &StageRecord,
) -> Result<Vec<CommitRecord>> {
    stage
        .roots
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let job_id = format!("{}-{index}", stage.job_id);
            let record = store
                .load_named::<CommitRecord>(&commit_record_name(&job_id))?
                .context("committed root journal is missing")?;
            if record.job_id != job_id
                || record.grant_id != stage.grant_id
                || record.grant_revision != stage.grant_revision
                || record.manifest_digest != stage.manifest_digest
                || record.destination_components != stage.destination_components
                || record.source_name != *source
            {
                bail!("committed root journal does not match the offered job")
            }
            Ok(record)
        })
        .collect()
}
fn expected_outcomes(
    destination_components: &[String],
    records: &[CommitRecord],
) -> Vec<DeviceReceiptRootOutcome> {
    records
        .iter()
        .map(|record| DeviceReceiptRootOutcome {
            source_root: record.source_name.clone(),
            destination_components: destination_components
                .iter()
                .cloned()
                .chain([record.destination_name.clone()])
                .collect(),
        })
        .collect()
}
fn validate_receipt(
    receipt: &DeviceReceipt,
    offer: &DeviceOffer,
    stage: &StageRecord,
    records: &[CommitRecord],
) -> Result<()> {
    if receipt.job_id != offer.job_id
        || receipt.transfer_id != offer.metadata.transfer_id
        || receipt.sender_device_id != offer.metadata.sender_device_id
        || receipt.sender_key_version != offer.metadata.sender_key_version
        || receipt.receiver_device_id != offer.metadata.receiver_device_id
        || receipt.receiver_key_version != offer.metadata.receiver_key_version
        || receipt.manifest_digest != stage.manifest_digest
        || receipt.grant_id != stage.grant_id.to_string()
        || receipt.grant_revision != stage.grant_revision
        || receipt.root_outcomes != expected_outcomes(&stage.destination_components, records)
    {
        bail!("journal receipt does not match the committed offer")
    }
    Ok(())
}
fn reconcile_before_prepare(
    store: &filebeam_transfer_native::checkpoint::Store,
    stage: &StageRecord,
    destination: &PlatformRoot,
) -> Result<()> {
    for (index, source) in stage.roots.iter().enumerate() {
        let name = commit_record_name(&format!("{}-{index}", stage.job_id));
        let Some(mut record) = store.load_named::<CommitRecord>(&name)? else {
            continue;
        };
        if record.job_id != format!("{}-{index}", stage.job_id)
            || record.grant_id != stage.grant_id
            || record.grant_revision != stage.grant_revision
            || record.destination_components != stage.destination_components
            || record.source_name != *source
            || record.manifest_digest != stage.manifest_digest
        {
            bail!("committed root journal does not match the offered job")
        }
        if record.state == CommitState::Intent
            && destination.stage_identity(&record.destination_name)?
                == Some(record.identity.clone())
        {
            record.state = CommitState::Renamed;
            store.save_named(&name, &record)?;
        }
        if record.state == CommitState::Renamed {
            if destination.stage_identity(&record.destination_name)?
                != Some(record.identity.clone())
            {
                bail!("committed destination identity is ambiguous")
            }
        }
    }
    Ok(())
}
fn cleanup_receive_artifacts(stage: &PlatformRoot, chunks: &[u64]) -> Result<()> {
    let base = stage.tethered_path()?;
    for (item, count) in chunks.iter().enumerate() {
        let _ = std::fs::remove_file(base.join(format!("plain-{item}")));
        for chunk in 0..*count {
            let _ = std::fs::remove_file(base.join(format!("cipher-{item}-{chunk}")));
        }
    }
    Ok(())
}
fn commit_root(
    home: &Path,
    destination: &PlatformRoot,
    stage: &PlatformRoot,
    grant: &DestinationGrant,
    job_id: &str,
    source: &str,
    destination_components: &[String],
    manifest_digest: &str,
) -> Result<String> {
    let store = daemon_store(home)?;
    let name = commit_record_name(job_id);
    let mut record = match store.load_named::<CommitRecord>(&name)? {
        Some(record) => record,
        None => CommitRecord {
            job_id: job_id.into(),
            grant_id: grant.id,
            grant_revision: grant.revision,
            manifest_digest: manifest_digest.into(),
            destination_components: destination_components.into(),
            source_name: source.into(),
            destination_name: source.into(),
            identity: stage
                .stage_identity(source)?
                .context("staged root is missing")?,
            state: CommitState::Intent,
        },
    };
    if record.grant_id != grant.id
        || record.grant_revision != grant.revision
        || record.source_name != source
        || record.destination_components != destination_components
        || record.manifest_digest != manifest_digest
    {
        bail!("commit journal conflicts with an existing job")
    }
    if record.state == CommitState::Intent {
        let mut suffix = 0u32;
        while destination
            .stage_identity(&record.destination_name)?
            .is_some()
        {
            suffix += 1;
            record.destination_name = collision_name(source, suffix)?;
        }
        store.save_named(&name, &record)?;
        destination.publish_stage(stage, source, &record.destination_name, &record.identity)?;
        destination.sync()?;
        record.state = CommitState::Renamed;
        store.save_named(&name, &record)?;
    }
    Ok(record.destination_name)
}
fn collision_name(source: &str, suffix: u32) -> Result<String> {
    if suffix == 0 {
        return Ok(source.into());
    }
    let suffix = format!(" ({suffix})");
    let limit = 255usize
        .checked_sub(suffix.len())
        .context("destination name is too long")?;
    let mut end = source.len().min(limit);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    Ok(format!("{}{}", &source[..end], suffix))
}

impl WindowsReceiverRoot {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            root: PlatformRoot::open(path)?,
        })
    }

    pub(crate) fn root_identity(&self) -> (u64, [u8; 16]) {
        self.root.identity()
    }

    pub(crate) fn destination_root(&self) -> Result<PlatformRoot> {
        self.root.try_clone()
    }

    pub(crate) fn create_stage(&self, name: &str) -> Result<PlatformRoot> {
        self.root.create_stage(name)
    }

    pub(crate) fn reopen_stage(&self, name: &str) -> Result<PlatformRoot> {
        self.root.open_stage(name)
    }

    pub(crate) fn create_tree_target(&self, stage: &PlatformRoot, relative: &Path) -> Result<File> {
        stage.create_target(relative)
    }

    pub(crate) fn stage_tree(&self, components: &[String]) -> Result<PlatformRoot> {
        self.root.stage_subdirectory(components)
    }

    pub(crate) fn journal_stage_identity(
        &self,
        stage: &PlatformRoot,
        source_name: &str,
    ) -> Result<Option<StagedObjectIdentity>> {
        stage.stage_identity(source_name)
    }

    pub(crate) fn publish_no_replace(
        &self,
        stage: &PlatformRoot,
        source_name: &str,
        destination_name: &str,
        identity: &StagedObjectIdentity,
    ) -> Result<()> {
        self.root
            .publish_stage(stage, source_name, destination_name, identity)
    }

    pub(crate) fn remove_stage(&self, name: &str) -> Result<()> {
        self.root.remove_stage(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_names_preserve_short_names_and_utf8_boundaries() {
        assert_eq!(collision_name("photos", 1).unwrap(), "photos (1)");
        let source = "界".repeat(85);
        let name = collision_name(&source, 1).unwrap();
        assert!(name.len() <= 255);
        assert!(name.ends_with(" (1)"));
        assert!(source.starts_with(name.trim_end_matches(" (1)")));
    }

    #[test]
    fn historical_receipt_binding_preserves_the_requested_alias() {
        let mut stage = StageRecord {
            job_id: "job".into(),
            requested_grant_id: "dropzone".into(),
            grant_id: Uuid::new_v4(),
            grant_revision: 1,
            staging_name: "stage".into(),
            destination_components: vec![],
            roots: vec!["file".into()],
            chunk_counts: vec![1],
            manifest_digest: "digest".into(),
        };
        let mut offer = DeviceOffer {
            job_id: stage.job_id.clone(),
            expires_at_unix_seconds: u64::MAX,
            metadata: filebeam_transfer_native::device::DeviceOfferMetadata {
                sender_device_id: "sender".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver".into(),
                receiver_key_version: 1,
                transfer_id: "transfer".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "envelope".into(),
                items: vec![],
                manifest_digest: stage.manifest_digest.clone(),
            },
            transfer_key: "key".into(),
            requested_grant_id: "dropzone".into(),
            requested_subdirectory: None,
        };
        assert!(stage_matches_offer(&stage, &offer).unwrap());
        offer.requested_grant_id = "work".into();
        assert!(!stage_matches_offer(&stage, &offer).unwrap());
        stage.requested_grant_id.clear();
        offer.requested_grant_id = stage.grant_id.to_string();
        assert!(stage_matches_offer(&stage, &offer).unwrap());
    }

    fn record(source: &str, destination: &str) -> CommitRecord {
        CommitRecord {
            job_id: format!("job-{}", source),
            grant_id: Uuid::nil(),
            grant_revision: 1,
            manifest_digest: "manifest".into(),
            destination_components: vec!["received".into()],
            source_name: source.into(),
            destination_name: destination.into(),
            identity: StagedObjectIdentity {
                volume_serial: 1,
                file_id: [2; 16],
            },
            state: CommitState::Renamed,
        }
    }

    #[test]
    fn recovered_root_outcomes_keep_the_original_collision_name() {
        let records = vec![record("photos", "photos (1)"), record("notes", "notes")];
        assert_eq!(
            expected_outcomes(&["received".into()], &records),
            vec![
                DeviceReceiptRootOutcome {
                    source_root: "photos".into(),
                    destination_components: vec!["received".into(), "photos (1)".into()],
                },
                DeviceReceiptRootOutcome {
                    source_root: "notes".into(),
                    destination_components: vec!["received".into(), "notes".into()],
                },
            ]
        );
    }
}
