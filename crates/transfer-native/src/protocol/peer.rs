//! Registered-device jobs reuse the existing preparation, WebRTC, and receive
//! workers while keeping descriptor and receipt material peer-only.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_encryption::{
    decrypt_manifest, encrypt_manifest, generate_nonce_prefix, generate_transfer_key,
};
use filebeam_transfer::manifest::{DigestValue, Manifest, ManifestItem, ManifestMetadataItem};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use webrtc::data_channel::DataChannel;

use crate::{
    checkpoint::Store,
    control::{Control, Phase},
    device::{
        DeviceOffer, DeviceOfferDecision, DeviceOfferMetadata, DeviceReceipt, DeviceReceiptItem,
        DeviceReceiptRootOutcome, DeviceSigner, LocalDevice, VerifiedDeviceAuthority,
        load_device_receipt, persist_device_receipt, receive_device_offer, receive_device_receipt,
        receive_device_receipt_ack, receive_offer_decision, send_device_offer, send_device_receipt,
        send_device_receipt_ack, send_offer_decision,
    },
    source::{
        SourceSpec,
        tree::{EnumeratedSourceTree, TreeEnumerationOptions, enumerate_source_tree},
    },
    webrtc::{DeviceChannels, request_chunk},
};

pub use super::download::{PeerChunkPosition, PeerReceiveItem, PeerReceiveStaging};
use super::upload::PeerChunkKey;

/// Durable peer preparation. The artifact is persisted before the caller can
/// attach it to the unchanged WebRTC chunk worker, making a prepared job
/// independent of its source path.
pub(crate) fn prepare_chunk(
    store: &Store,
    item: &super::upload::PeerPreparedItem,
    key: &PeerChunkKey<'_>,
    position: u64,
    chunk_bytes: u64,
    control: Control,
) -> Result<PreparedChunk> {
    let prepared = super::upload::prepare_peer_chunk(item, key, position, chunk_bytes, control)?;
    let name = format!("peer-chunk-{}-{position}", key.item_id);
    let path = store.persist_immutable(&name, &prepared.ciphertext)?;
    Ok(PreparedChunk {
        path,
        plaintext_bytes: prepared.plaintext_bytes,
        ciphertext_bytes: prepared.ciphertext.len() as u64,
        checksum: hex::encode(Sha256::digest(&prepared.ciphertext)),
    })
}

pub struct PreparedChunk {
    pub path: PathBuf,
    pub plaintext_bytes: u64,
    pub ciphertext_bytes: u64,
    pub checksum: String,
}

#[derive(Clone)]
pub struct PeerSource {
    pub name: String,
    pub spec: SourceSpec,
}

#[derive(Clone, Debug)]
pub struct PeerPreparedJob {
    checkpoint: PeerCheckpoint,
    artifacts: HashMap<(String, u64), super::upload::rtc_upload::CiphertextArtifact>,
    checkpoint_id: String,
}

#[derive(Clone)]
pub struct PeerReceiveGrant {
    pub id: String,
    pub revision: u64,
    /// Existing receive helpers write verified plaintext below this private
    /// staging base. `targets` must point to commit-owned staging names.
    pub staging_base: PathBuf,
    pub targets: Vec<PathBuf>,
    /// A receiver may retain the private staging directory as a native handle
    /// instead of exposing it through a platform-specific descriptor path.
    pub staging: Option<Arc<dyn super::download::PeerReceiveStaging>>,
}

/// Receiver-owned policy and commit boundary. The engine never interprets a
/// remote destination path: the callback chooses staging and performs the
/// grant/revocation recheck plus durable commit journal operation.
pub trait PeerReceiver: Send + Sync {
    fn admit(&self, offer: &DeviceOffer, manifest: &Manifest) -> Result<PeerReceiveGrant>;
    /// Returns the receiver-owned historical receipt, if publication completed
    /// before the core checkpoint could record it.
    fn recover_receipt(&self, _offer: &DeviceOffer) -> Result<Option<DeviceReceipt>> {
        Ok(None)
    }
    fn commit(
        &self,
        grant: &PeerReceiveGrant,
        job_id: &str,
    ) -> Result<Vec<DeviceReceiptRootOutcome>>;
    fn persist_receipt(&self, grant: &PeerReceiveGrant, receipt: &DeviceReceipt) -> Result<()>;
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PeerCheckpoint {
    version: u8,
    job_id: String,
    transfer_id: String,
    sender_device_id: String,
    sender_key_version: u32,
    receiver_device_id: String,
    receiver_key_version: u32,
    chunk_bytes: u64,
    master_key: String,
    encrypted_manifest: String,
    manifest_digest: String,
    items: Vec<PeerCheckpointItem>,
}

impl std::fmt::Debug for PeerCheckpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerCheckpoint").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct PeerCheckpointItem {
    id: String,
    name: String,
    size: u64,
    nonce_prefix: String,
    chunk_count: u64,
    digest: String,
    artifacts: Vec<PeerArtifact>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct PeerArtifact {
    position: u64,
    path: PathBuf,
    bytes: u64,
    checksum: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct PeerReceiveCheckpoint {
    version: u8,
    job_id: String,
    transfer_id: String,
    sender_device_id: String,
    sender_key_version: u32,
    receiver_device_id: String,
    receiver_key_version: u32,
    manifest_digest: String,
    verified: Vec<Vec<bool>>,
    published: Vec<bool>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEnvelope {
    v: u8,
    nonce_prefix: String,
    ciphertext: String,
}

impl PeerPreparedJob {
    pub fn job_id(&self) -> &str {
        &self.checkpoint.job_id
    }

    pub fn checkpoint_id(&self) -> &str {
        &self.checkpoint_id
    }

    pub fn offer(
        &self,
        requested_grant_id: String,
        requested_subdirectory: Option<String>,
        expires_at_unix_seconds: u64,
    ) -> DeviceOffer {
        DeviceOffer {
            job_id: self.checkpoint.job_id.clone(),
            expires_at_unix_seconds,
            metadata: DeviceOfferMetadata {
                sender_device_id: self.checkpoint.sender_device_id.clone(),
                sender_key_version: self.checkpoint.sender_key_version,
                receiver_device_id: self.checkpoint.receiver_device_id.clone(),
                receiver_key_version: self.checkpoint.receiver_key_version,
                transfer_id: self.checkpoint.transfer_id.clone(),
                protocol_version: 1,
                chunk_bytes: self.checkpoint.chunk_bytes,
                encrypted_manifest: self.checkpoint.encrypted_manifest.clone(),
                items: self
                    .checkpoint
                    .items
                    .iter()
                    .enumerate()
                    .map(|(position, item)| ManifestMetadataItem {
                        id: item.id.clone(),
                        position: position as u64,
                        chunk_count: item.chunk_count,
                    })
                    .collect(),
                manifest_digest: self.checkpoint.manifest_digest.clone(),
            },
            transfer_key: self.checkpoint.master_key.clone(),
            requested_grant_id,
            requested_subdirectory,
        }
    }

    pub(crate) async fn serve_artifacts(&self, channel: Arc<dyn DataChannel>) -> Result<()> {
        super::upload::rtc_upload::serve_artifacts(channel, self.artifacts.clone()).await
    }
}

/// Registered sender and receiver identities bound into a prepared peer job.
#[derive(Clone, Debug)]
pub struct PeerParties {
    pub sender_device_id: String,
    pub sender_key_version: u32,
    pub receiver_device_id: String,
    pub receiver_key_version: u32,
}

/// Prepare every selected regular source into immutable artifacts and persist a
/// typed private job record before an offer can be created.
pub fn prepare_peer_job(
    store: &Store,
    sources: &[PeerSource],
    parties: PeerParties,
    chunk_bytes: u64,
    control: Control,
) -> Result<PeerPreparedJob> {
    prepare_peer_job_inner(store, sources, None, parties, chunk_bytes, control)
}

/// Enumerate selected filesystem roots without following links, then prepare
/// their files through the normal immutable artifact path. Empty directories
/// are retained in the encrypted tree manifest.
pub fn prepare_peer_tree_job(
    store: &Store,
    selected: &[PathBuf],
    options: TreeEnumerationOptions,
    parties: PeerParties,
    chunk_bytes: u64,
    control: Control,
) -> Result<PeerPreparedJob> {
    let tree = enumerate_source_tree(selected, options)?;
    let sources = tree
        .files
        .iter()
        .map(|file| PeerSource {
            name: file.source.name.clone(),
            spec: file.source.spec.clone(),
        })
        .collect::<Vec<_>>();
    prepare_peer_job_inner(store, &sources, Some(&tree), parties, chunk_bytes, control)
}

fn prepare_peer_job_inner(
    store: &Store,
    sources: &[PeerSource],
    source_tree: Option<&EnumeratedSourceTree>,
    parties: PeerParties,
    chunk_bytes: u64,
    control: Control,
) -> Result<PeerPreparedJob> {
    let PeerParties {
        sender_device_id,
        sender_key_version,
        receiver_device_id,
        receiver_key_version,
    } = parties;
    if (sources.is_empty() && source_tree.is_none())
        || chunk_bytes == 0
        || sender_device_id.is_empty()
        || receiver_device_id.is_empty()
        || sender_key_version == 0
        || receiver_key_version == 0
    {
        bail!("invalid peer preparation input")
    }
    let attached_note = control.attached_note();
    if let Some(note) = &attached_note {
        note.validate().map_err(anyhow::Error::msg)?;
    }
    control.phase(Phase::Preparing)?;
    let transfer_id = new_ulid()?;
    let master_key = generate_transfer_key()?;
    let mut items = Vec::with_capacity(sources.len());
    let mut artifacts = HashMap::new();
    let mut manifest_items = Vec::with_capacity(sources.len());
    let total = sources.iter().try_fold(0u64, |total, source| {
        total
            .checked_add(source.spec.bounds().1)
            .context("peer source bytes overflow")
    })?;
    control.totals(total, sources.len());
    let mut done = 0u64;
    for (index, source) in sources.iter().enumerate() {
        validate_source_name(&source.name)?;
        let prepared_item = super::upload::prepare_peer_item(&source.spec, chunk_bytes, &control)?;
        let (size, digest) = super::upload::peer_prepared_item_metadata(&prepared_item);
        let chunk_count = size
            .checked_add(chunk_bytes - 1)
            .context("peer chunk count overflow")?
            / chunk_bytes;
        let chunk_count = chunk_count.max(1);
        let item_id = new_ulid()?;
        let nonce_prefix = generate_nonce_prefix()?;
        control.item(source.name.clone(), index, size);
        let mut item_artifacts = Vec::with_capacity(usize::try_from(chunk_count)?);
        let key = PeerChunkKey {
            transfer_id: &transfer_id,
            item_id: &item_id,
            master_key: &master_key,
            nonce_prefix: &nonce_prefix,
        };
        for position in 0..chunk_count {
            let prepared = prepare_chunk(
                store,
                &prepared_item,
                &key,
                position,
                chunk_bytes,
                control.clone(),
            )?;
            done = done
                .checked_add(prepared.plaintext_bytes)
                .context("peer progress overflow")?;
            control.advance(done, prepared.plaintext_bytes, prepared.ciphertext_bytes);
            artifacts.insert(
                (item_id.clone(), position),
                super::upload::rtc_upload::CiphertextArtifact {
                    path: prepared.path.clone(),
                    bytes: prepared.ciphertext_bytes,
                    checksum: prepared.checksum.clone(),
                },
            );
            item_artifacts.push(PeerArtifact {
                position,
                path: prepared.path,
                bytes: prepared.ciphertext_bytes,
                checksum: prepared.checksum,
            });
        }
        let digest = digest.to_owned();
        let nonce_prefix = URL_SAFE_NO_PAD.encode(nonce_prefix);
        manifest_items.push(ManifestItem {
            id: item_id.clone(),
            name: source.name.clone(),
            mime: "application/octet-stream".into(),
            size,
            nonce_prefix: nonce_prefix.clone(),
            chunk_count,
            digest: DigestValue {
                algorithm: "sha256".into(),
                value: digest.clone(),
            },
        });
        items.push(PeerCheckpointItem {
            id: item_id,
            name: source.name.clone(),
            size,
            nonce_prefix,
            chunk_count,
            digest,
            artifacts: item_artifacts,
        });
    }
    let manifest = Manifest {
        attached_note,
        join_token: None,
        tree: source_tree
            .map(|tree| {
                tree.manifest_tree(
                    &manifest_items
                        .iter()
                        .map(|item| item.id.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .transpose()?,
        version: 1,
        items: manifest_items,
    };
    let manifest_plain = serde_json::to_vec(&manifest)?;
    let prefix = generate_nonce_prefix()?;
    let ciphertext = encrypt_manifest(
        &master_key,
        &prefix,
        &manifest_plain,
        format!("filebeam:v1:{transfer_id}:manifest:manifest").as_bytes(),
    )?;
    let encrypted_manifest = serde_json::to_string(
        &serde_json::json!({"v": 1, "nonce_prefix": URL_SAFE_NO_PAD.encode(prefix), "ciphertext": URL_SAFE_NO_PAD.encode(ciphertext)}),
    )?;
    let checkpoint = PeerCheckpoint {
        version: 1,
        job_id: Uuid::new_v4().to_string(),
        transfer_id,
        sender_device_id,
        sender_key_version,
        receiver_device_id,
        receiver_key_version,
        chunk_bytes,
        master_key: URL_SAFE_NO_PAD.encode(master_key),
        manifest_digest: hex::encode(Sha256::digest(&manifest_plain)),
        encrypted_manifest,
        items,
    };
    // A checkpoint exists only after every artifact is durable. Retrying an
    // interrupted preparation therefore starts with a fresh transfer key.
    store.save_named("peer-job-v1", &checkpoint)?;
    Ok(PeerPreparedJob {
        checkpoint,
        artifacts,
        checkpoint_id: store.id().to_owned(),
    })
}

/// Reopen a prepared peer job without touching its original sources. Missing
/// or altered artifacts are rejected by `serve_artifacts` before any chunk is
/// released.
pub fn load_peer_job(store: &Store) -> Result<PeerPreparedJob> {
    let checkpoint = store
        .load_named::<PeerCheckpoint>("peer-job-v1")?
        .context("peer job checkpoint is missing")?;
    if checkpoint.version != 1 || checkpoint.job_id.is_empty() || checkpoint.chunk_bytes == 0 {
        bail!("invalid peer job checkpoint")
    }
    let mut artifacts = HashMap::new();
    for item in &checkpoint.items {
        for artifact in &item.artifacts {
            if artifacts
                .insert(
                    (item.id.clone(), artifact.position),
                    super::upload::rtc_upload::CiphertextArtifact {
                        path: artifact.path.clone(),
                        bytes: artifact.bytes,
                        checksum: artifact.checksum.clone(),
                    },
                )
                .is_some()
            {
                bail!("peer job has duplicate artifact positions")
            }
        }
    }
    Ok(PeerPreparedJob {
        checkpoint,
        artifacts,
        checkpoint_id: store.id().to_owned(),
    })
}

/// Receiver destination and expiry requested in a peer offer.
pub(crate) struct PeerDelivery {
    pub grant_id: String,
    pub subdirectory: Option<String>,
    pub expires_at_unix_seconds: u64,
}

/// Send an already durable job after proof authentication. The only network
/// payload before `serve_artifacts` is the peer-only offer/accept metadata.
pub(crate) async fn send_prepared_peer_job(
    store: &Store,
    job: &PeerPreparedJob,
    channels: &DeviceChannels,
    authority: &VerifiedDeviceAuthority,
    delivery: PeerDelivery,
    receiver_key: &ed25519_dalek::VerifyingKey,
) -> Result<DeviceReceipt> {
    let PeerDelivery {
        grant_id: requested_grant_id,
        subdirectory: requested_subdirectory,
        expires_at_unix_seconds,
    } = delivery;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_secs();
    if expires_at_unix_seconds <= now {
        bail!("peer offer has expired")
    }
    let offer = job.offer(
        requested_grant_id,
        requested_subdirectory,
        expires_at_unix_seconds,
    );
    let manifest = decrypt_peer_manifest(&offer)?;
    filebeam_transfer::manifest::validate_verified_device_manifest_with_tree_capability(
        &manifest,
        offer.metadata.chunk_bytes,
        &offer.metadata.items,
        authority.tree_capability(),
    )
    .map_err(anyhow::Error::msg)?;
    let decision = tokio::join!(
        send_device_offer(channels.metadata.clone(), authority, &offer),
        receive_offer_decision(channels.metadata.clone()),
    );
    decision.0?;
    let decision = decision.1?;
    if !decision.accepted {
        bail!(
            "peer receiver rejected the offer: {}",
            decision.reason.unwrap_or_else(|| "unspecified".into())
        )
    }
    if Uuid::parse_str(&decision.grant_id).is_err() || decision.grant_revision == 0 {
        bail!("peer receiver accepted an invalid destination grant")
    }
    let artifacts = job.clone();
    let file = channels.file.clone();
    let worker = tokio::spawn(async move { artifacts.serve_artifacts(file).await });
    let receipt = receive_device_receipt(channels.metadata.clone(), authority).await?;
    receipt.verify(receiver_key)?;
    if receipt.job_id != job.checkpoint.job_id
        || receipt.transfer_id != job.checkpoint.transfer_id
        || receipt.sender_device_id != job.checkpoint.sender_device_id
        || receipt.sender_key_version != job.checkpoint.sender_key_version
        || receipt.manifest_digest != job.checkpoint.manifest_digest
        || receipt.grant_id != decision.grant_id
        || receipt.grant_revision != decision.grant_revision
    {
        bail!("peer receipt does not bind the offered job")
    }
    persist_device_receipt(store, &receipt)?;
    send_device_receipt_ack(channels.metadata.clone(), &receipt).await?;
    channels.file.close().await?;
    worker.await.context("peer artifact worker stopped")??;
    Ok(receipt)
}

/// Receive, decrypt, validate, and commit one authenticated peer offer. The
/// receiver callback owns grant authorization and the destination journal;
/// Filebeam's existing chunk writer and publisher still do all AEAD/hash work.
#[cfg(test)]
pub(crate) async fn receive_peer_job(
    store: &Store,
    channels: &DeviceChannels,
    authority: &VerifiedDeviceAuthority,
    local_device: &LocalDevice,
    receiver: &dyn PeerReceiver,
    receiver_signer: &dyn DeviceSigner,
) -> Result<DeviceReceipt> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_secs();
    let offer = receive_authenticated_peer_offer(channels, authority, local_device, now).await?;
    receive_admitted_offer(
        store,
        channels,
        authority,
        &offer,
        receiver,
        receiver_signer,
    )
    .await
}

/// Receive and validate the bounded offer before choosing a receiver store.
/// Runtime owners use `receiver_job_id` to reopen the same private checkpoint
/// for this exact authenticated sender/job on a new connection.
pub(crate) async fn receive_authenticated_peer_offer(
    channels: &DeviceChannels,
    authority: &VerifiedDeviceAuthority,
    local_device: &LocalDevice,
    now_unix_seconds: u64,
) -> Result<DeviceOffer> {
    receive_device_offer(
        channels.metadata.clone(),
        authority,
        local_device,
        now_unix_seconds,
    )
    .await
}

/// Stable, opaque checkpoint name for one authenticated sender job. This is
/// not a remote path and deliberately excludes mutable offer content: a
/// changed manifest must reopen the same store and fail its durable binding.
pub fn receiver_job_id(offer: &DeviceOffer) -> String {
    let mut hasher = Sha256::new();
    for value in [
        offer.metadata.sender_device_id.as_bytes(),
        &offer.metadata.sender_key_version.to_be_bytes(),
        offer.metadata.receiver_device_id.as_bytes(),
        &offer.metadata.receiver_key_version.to_be_bytes(),
        offer.job_id.as_bytes(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    format!("peer-recv-{}", &hex::encode(hasher.finalize())[..32])
}

/// Continue a previously authenticated offer using the stable store selected
/// by the runtime. The immutable offer binding is durable before staging; a
/// colliding or altered offer is rejected rather than overwriting that state.
pub(crate) async fn receive_admitted_offer(
    store: &Store,
    channels: &DeviceChannels,
    authority: &VerifiedDeviceAuthority,
    offer: &DeviceOffer,
    receiver: &dyn PeerReceiver,
    receiver_signer: &dyn DeviceSigner,
) -> Result<DeviceReceipt> {
    if let Some(receipt) = load_device_receipt(store)? {
        if receipt.job_id != offer.job_id
            || receipt.transfer_id != offer.metadata.transfer_id
            || receipt.sender_device_id != offer.metadata.sender_device_id
            || receipt.sender_key_version != offer.metadata.sender_key_version
            || receipt.receiver_device_id != offer.metadata.receiver_device_id
            || receipt.receiver_key_version != offer.metadata.receiver_key_version
            || receipt.manifest_digest != offer.metadata.manifest_digest
        {
            bail!("receiver checkpoint belongs to a different peer job")
        }
        send_offer_decision(
            channels.metadata.clone(),
            &DeviceOfferDecision {
                accepted: true,
                reason: None,
                grant_id: receipt.grant_id.clone(),
                grant_revision: receipt.grant_revision,
            },
        )
        .await?;
        send_device_receipt(channels.metadata.clone(), &receipt).await?;
        receive_device_receipt_ack(channels.metadata.clone(), &receipt).await?;
        return Ok(receipt);
    }
    if let Some(receipt) = receiver.recover_receipt(offer)? {
        if receipt.job_id != offer.job_id
            || receipt.transfer_id != offer.metadata.transfer_id
            || receipt.sender_device_id != offer.metadata.sender_device_id
            || receipt.sender_key_version != offer.metadata.sender_key_version
            || receipt.receiver_device_id != offer.metadata.receiver_device_id
            || receipt.receiver_key_version != offer.metadata.receiver_key_version
            || receipt.manifest_digest != offer.metadata.manifest_digest
        {
            bail!("receiver journal belongs to a different peer job")
        }
        receipt.verify(&receiver_signer.verifying_key())?;
        persist_device_receipt(store, &receipt)?;
        send_offer_decision(
            channels.metadata.clone(),
            &DeviceOfferDecision {
                accepted: true,
                reason: None,
                grant_id: receipt.grant_id.clone(),
                grant_revision: receipt.grant_revision,
            },
        )
        .await?;
        send_device_receipt(channels.metadata.clone(), &receipt).await?;
        receive_device_receipt_ack(channels.metadata.clone(), &receipt).await?;
        return Ok(receipt);
    }
    let manifest = decrypt_peer_manifest(offer)?;
    offer.validate_manifest(&manifest, authority)?;
    if hex::encode(Sha256::digest(serde_json::to_vec(&manifest)?)) != offer.metadata.manifest_digest
    {
        bail!("peer manifest digest does not match the offer")
    }
    let mut state = match store.load_named::<PeerReceiveCheckpoint>("peer-receive-v1")? {
        Some(state) => {
            validate_receive_checkpoint(&state, offer, &manifest)?;
            state
        }
        None => {
            let state = PeerReceiveCheckpoint {
                version: 1,
                job_id: offer.job_id.clone(),
                transfer_id: offer.metadata.transfer_id.clone(),
                sender_device_id: offer.metadata.sender_device_id.clone(),
                sender_key_version: offer.metadata.sender_key_version,
                receiver_device_id: offer.metadata.receiver_device_id.clone(),
                receiver_key_version: offer.metadata.receiver_key_version,
                manifest_digest: offer.metadata.manifest_digest.clone(),
                verified: manifest
                    .items
                    .iter()
                    .map(|item| vec![false; item.chunk_count as usize])
                    .collect(),
                published: vec![false; manifest.items.len()],
            };
            store.save_named("peer-receive-v1", &state)?;
            state
        }
    };
    let grant = receiver.admit(offer, &manifest)?;
    if Uuid::parse_str(&grant.id).is_err()
        || grant.revision == 0
        || grant.targets.len() != manifest.items.len()
    {
        bail!("receiver grant admission does not match the offer")
    }
    send_offer_decision(
        channels.metadata.clone(),
        &DeviceOfferDecision {
            accepted: true,
            reason: None,
            grant_id: grant.id.clone(),
            grant_revision: grant.revision,
        },
    )
    .await?;
    let master = URL_SAFE_NO_PAD.decode(&offer.transfer_key)?;
    let mut completed = Vec::with_capacity(manifest.items.len());
    let mut sequence = 0u32;
    for (item_index, manifest_item) in manifest.items.iter().enumerate() {
        let item = PeerReceiveItem {
            id: manifest_item.id.clone(),
            name: manifest_item.name.clone(),
            size: manifest_item.size,
            nonce_prefix: manifest_item.nonce_prefix.clone(),
            chunk_count: manifest_item.chunk_count,
            digest: manifest_item.digest.value.clone(),
            target: grant.targets[item_index].clone(),
        };
        for position in 0..manifest_item.chunk_count {
            sequence = sequence
                .checked_add(1)
                .context("peer request sequence overflow")?;
            let expected_plain = (manifest_item
                .size
                .saturating_sub(position.saturating_mul(offer.metadata.chunk_bytes)))
            .min(offer.metadata.chunk_bytes);
            let cipher_path = grant
                .staging_base
                .join(format!("cipher-{item_index}-{position}"));
            let ciphertext = if state.verified[item_index][position as usize] {
                // A saved bit is only a hint after a crash: authenticate the
                // retained record again before skipping a network request.
                match grant.staging.as_ref().map_or_else(
                    || std::fs::read(&cipher_path).map_err(Into::into),
                    |staging| staging.read_ciphertext(item_index, position),
                ) {
                    Ok(ciphertext) => ciphertext,
                    Err(error)
                        if error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
                    {
                        state.verified[item_index][position as usize] = false;
                        store.save_named("peer-receive-v1", &state)?;
                        request_chunk(
                            channels.file.clone(),
                            sequence,
                            item.id.clone(),
                            position,
                            expected_plain
                                .checked_add(filebeam_transfer::AEAD_TAG_BYTES)
                                .context("peer ciphertext length overflow")?,
                        )
                        .await?
                    }
                    Err(error) => return Err(error).context("read retained peer ciphertext"),
                }
            } else {
                request_chunk(
                    channels.file.clone(),
                    sequence,
                    item.id.clone(),
                    position,
                    expected_plain
                        .checked_add(filebeam_transfer::AEAD_TAG_BYTES)
                        .context("peer ciphertext length overflow")?,
                )
                .await?
            };
            let chunk = PeerChunkPosition {
                item_index,
                index: position,
                chunk_bytes: offer.metadata.chunk_bytes,
            };
            if let Some(staging) = &grant.staging {
                super::download::receive_peer_chunk_staged(
                    staging.as_ref(),
                    chunk,
                    &offer.metadata.transfer_id,
                    &item,
                    &master,
                    ciphertext,
                )?;
            } else {
                receive_chunk(
                    &grant.staging_base,
                    chunk,
                    &offer.metadata.transfer_id,
                    &item,
                    &master,
                    ciphertext,
                )?;
            }
            state.verified[item_index][position as usize] = true;
            store.save_named("peer-receive-v1", &state)?;
        }
        if !state.published[item_index] {
            if let Some(staging) = &grant.staging {
                super::download::publish_peer_item_staged(staging.as_ref(), item_index, &item)?;
            } else {
                publish_item(&grant.staging_base, item_index, &item)?;
            }
            state.published[item_index] = true;
            store.save_named("peer-receive-v1", &state)?;
        }
        completed.push(DeviceReceiptItem {
            item_id: item.id,
            digest: item.digest,
            plaintext_bytes: item.size,
        });
    }
    let root_outcomes = receiver.commit(&grant, &offer.job_id)?;
    let receipt = DeviceReceipt {
        job_id: offer.job_id.clone(),
        transfer_id: offer.metadata.transfer_id.clone(),
        sender_device_id: offer.metadata.sender_device_id.clone(),
        sender_key_version: offer.metadata.sender_key_version,
        receiver_device_id: offer.metadata.receiver_device_id.clone(),
        receiver_key_version: offer.metadata.receiver_key_version,
        manifest_digest: offer.metadata.manifest_digest.clone(),
        grant_id: grant.id.clone(),
        grant_revision: grant.revision,
        commit_id: Uuid::new_v4().to_string(),
        committed_at_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock predates Unix epoch")?
            .as_secs(),
        items: completed,
        root_outcomes,
        signature: [0; 64],
    }
    .sign_with(receiver_signer)?;
    receiver.persist_receipt(&grant, &receipt)?;
    persist_device_receipt(store, &receipt)?;
    send_device_receipt(channels.metadata.clone(), &receipt).await?;
    receive_device_receipt_ack(channels.metadata.clone(), &receipt).await?;
    Ok(receipt)
}

fn decrypt_peer_manifest(offer: &DeviceOffer) -> Result<Manifest> {
    let envelope: ManifestEnvelope = serde_json::from_str(&offer.metadata.encrypted_manifest)
        .context("parse peer manifest envelope")?;
    if envelope.v != 1 {
        bail!("unsupported peer manifest envelope")
    }
    let key = URL_SAFE_NO_PAD
        .decode(&offer.transfer_key)
        .context("decode peer transfer key")?;
    let plain = decrypt_manifest(
        &key,
        &URL_SAFE_NO_PAD.decode(envelope.nonce_prefix)?,
        &URL_SAFE_NO_PAD.decode(envelope.ciphertext)?,
        format!(
            "filebeam:v1:{}:manifest:manifest",
            offer.metadata.transfer_id
        )
        .as_bytes(),
    )?;
    serde_json::from_slice(&plain).context("parse peer manifest")
}

fn validate_receive_checkpoint(
    state: &PeerReceiveCheckpoint,
    offer: &DeviceOffer,
    manifest: &Manifest,
) -> Result<()> {
    if state.version != 1
        || state.job_id != offer.job_id
        || state.transfer_id != offer.metadata.transfer_id
        || state.sender_device_id != offer.metadata.sender_device_id
        || state.sender_key_version != offer.metadata.sender_key_version
        || state.receiver_device_id != offer.metadata.receiver_device_id
        || state.receiver_key_version != offer.metadata.receiver_key_version
        || state.manifest_digest != offer.metadata.manifest_digest
        || state.verified.len() != manifest.items.len()
        || state.published.len() != manifest.items.len()
        || state
            .verified
            .iter()
            .zip(&manifest.items)
            .any(|(verified, item)| verified.len() != item.chunk_count as usize)
    {
        bail!("receiver checkpoint does not match the authenticated peer offer")
    }
    Ok(())
}

fn validate_source_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 255
        || name.contains(['/', '\\', '\0'])
        || name == "."
        || name == ".."
    {
        bail!("peer source name is invalid")
    }
    Ok(())
}

fn new_ulid() -> Result<String> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_millis();
    if timestamp >= (1u128 << 48) {
        bail!("system time cannot be represented as a ULID")
    }
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&(timestamp as u64).to_be_bytes()[2..]);
    getrandom::getrandom(&mut bytes[6..])?;
    let value = u128::from_be_bytes(bytes);
    Ok((0..26)
        .map(|shift| ALPHABET[((value >> (125 - shift * 5)) & 31) as usize] as char)
        .collect())
}

pub fn receive_chunk(
    base: &std::path::Path,
    position: PeerChunkPosition,
    transfer_id: &str,
    item: &PeerReceiveItem,
    master: &[u8],
    ciphertext: Vec<u8>,
) -> Result<u64> {
    super::download::receive_peer_chunk(base, position, transfer_id, item, master, ciphertext)
}

pub fn receive_chunk_staged(
    staging: &dyn PeerReceiveStaging,
    position: PeerChunkPosition,
    transfer_id: &str,
    item: &PeerReceiveItem,
    master: &[u8],
    ciphertext: Vec<u8>,
) -> Result<u64> {
    super::download::receive_peer_chunk_staged(
        staging,
        position,
        transfer_id,
        item,
        master,
        ciphertext,
    )
}

pub fn publish_item(
    base: &std::path::Path,
    item_index: usize,
    item: &PeerReceiveItem,
) -> Result<()> {
    super::download::publish_peer_item(base, item_index, item)
}

pub fn publish_item_staged(
    staging: &dyn PeerReceiveStaging,
    item_index: usize,
    item: &PeerReceiveItem,
) -> Result<()> {
    super::download::publish_peer_item_staged(staging, item_index, item)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use filebeam_encryption::{generate_nonce_prefix, generate_transfer_key};
    use sha2::Digest;

    use crate::{
        checkpoint::Store,
        control::Control,
        device::{
            DeviceOffer, DeviceOfferDecision, DeviceOfferMetadata, DeviceProofContext,
            DeviceReceipt, DeviceReceiptItem, PinnedDevice, authenticate_metadata_channel,
            load_device_receipt, persist_device_receipt, receive_device_offer,
            receive_device_receipt, receive_offer_decision, send_device_offer,
            send_device_receipt_ack, send_offer_decision,
        },
        protocol::{
            download::receive_peer_chunk,
            peer::PeerReceiveItem,
            upload::{PeerChunkKey, prepare_peer_chunk},
        },
        source::SourceSpec,
        webrtc::{NativePeer, request_chunk, serve_channel, wait_for_open},
    };

    const TEST_CHUNK: PeerChunkPosition = PeerChunkPosition {
        item_index: 0,
        index: 0,
        chunk_bytes: 1024,
    };

    fn test_parties() -> PeerParties {
        PeerParties {
            sender_device_id: "sender-1".into(),
            sender_key_version: 1,
            receiver_device_id: "receiver-1".into(),
            receiver_key_version: 1,
        }
    }

    struct TestReceiver {
        base: PathBuf,
        admits: AtomicUsize,
        commits: AtomicUsize,
        recovered: Option<DeviceReceipt>,
    }

    impl PeerReceiver for TestReceiver {
        fn admit(&self, _: &DeviceOffer, manifest: &Manifest) -> Result<PeerReceiveGrant> {
            self.admits.fetch_add(1, Ordering::Relaxed);
            Ok(PeerReceiveGrant {
                id: "1d8a0700-173a-4ed3-936c-952cb43cc1b7".into(),
                revision: 1,
                staging_base: self.base.clone(),
                targets: manifest
                    .items
                    .iter()
                    .map(|item| self.base.join(&item.name))
                    .collect(),
                staging: None,
            })
        }

        fn commit(&self, _: &PeerReceiveGrant, _: &str) -> Result<Vec<DeviceReceiptRootOutcome>> {
            self.commits.fetch_add(1, Ordering::Relaxed);
            Ok(vec![])
        }

        fn persist_receipt(&self, _: &PeerReceiveGrant, _: &DeviceReceipt) -> Result<()> {
            Ok(())
        }

        fn recover_receipt(&self, _: &DeviceOffer) -> Result<Option<DeviceReceipt>> {
            Ok(self.recovered.clone())
        }
    }

    #[tokio::test]
    async fn peer_path_reuses_public_prepare_webrtc_receive_and_save_workers() -> anyhow::Result<()>
    {
        let source_dir = tempfile::tempdir()?;
        let source = source_dir.path().join("source.txt");
        let plain = b"same worker bytes".to_vec();
        fs::write(&source, &plain)?;
        let transfer_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let item_id = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
        let master = generate_transfer_key()?;
        let nonce = generate_nonce_prefix()?;
        let artifacts = tempfile::tempdir()?;
        let store = Store::create(artifacts.path(), "peer-job")?;
        let source = SourceSpec::Path {
            path: source.clone(),
            offset: 0,
            length: plain.len() as u64,
        };
        let item =
            super::super::upload::prepare_peer_item(&source, 1024, &Control::test_factory())?;
        let key = PeerChunkKey {
            transfer_id,
            item_id,
            master_key: &master,
            nonce_prefix: &nonce,
        };
        let durable = super::prepare_chunk(&store, &item, &key, 0, 1024, Control::test_factory())?;
        let prepared = prepare_peer_chunk(&item, &key, 0, 1024, Control::test_factory())?;
        assert_eq!(fs::read(&durable.path)?, prepared.ciphertext);

        let receiver = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".into()]).await?;
        let sender = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".into()]).await?;
        let (offer, receiving) = receiver.offer_channel().await?;
        let answer = sender.answer(&offer).await?;
        receiver.accept_answer(&answer).await?;
        let serving = sender.receiver_channel().await?;
        wait_for_open(&receiving).await?;
        wait_for_open(&serving).await?;
        let worker = tokio::spawn(serve_channel(
            serving,
            HashMap::from([((item_id.into(), 0), prepared.ciphertext.clone())]),
        ));
        let ciphertext = request_chunk(
            receiving.clone(),
            1,
            item_id.into(),
            0,
            prepared.ciphertext.len() as u64,
        )
        .await?;

        let output = tempfile::tempdir()?;
        let target = output.path().join("received.txt");
        let item = PeerReceiveItem {
            id: item_id.into(),
            name: "received.txt".into(),
            size: plain.len() as u64,
            nonce_prefix: URL_SAFE_NO_PAD.encode(nonce),
            chunk_count: 1,
            digest: hex::encode(sha2::Sha256::digest(&plain)),
            target: target.clone(),
        };
        assert_eq!(
            receive_peer_chunk(
                output.path(),
                TEST_CHUNK,
                transfer_id,
                &item,
                &master,
                ciphertext
            )?,
            prepared.plaintext_bytes
        );
        super::publish_item(output.path(), 0, &item)?;
        assert_eq!(fs::read(target)?, plain);
        receiving.close().await?;
        worker.await??;
        receiver.close().await;
        sender.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn authenticated_peer_offer_transfers_bytes_and_replays_persisted_receipt()
    -> anyhow::Result<()> {
        let sender = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".into()]).await?;
        let receiver = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".into()]).await?;
        let (offer, sender_channels) = sender.offer_device_channels().await?;
        let answer = receiver.answer(&offer).await?;
        sender.accept_answer(&answer).await?;
        let receiver_channels = receiver.device_receiver_channels().await?;
        wait_for_open(&sender_channels.file).await?;
        wait_for_open(&sender_channels.metadata).await?;
        wait_for_open(&receiver_channels.file).await?;
        wait_for_open(&receiver_channels.metadata).await?;

        let sender_key = ed25519_dalek::SigningKey::from_bytes(&[3; 32]);
        let receiver_key = ed25519_dalek::SigningKey::from_bytes(&[4; 32]);
        let sender_context = DeviceProofContext {
            instance_origin: "https://filebeam.test".into(),
            deployment_id: "test-1".into(),
            account_id: "account-1".into(),
            device_generation: 1,
            signer_device_id: "sender-1".into(),
            signer_key_version: 1,
            peer_device_id: "receiver-1".into(),
            peer_key_version: 1,
            attempt_id: "attempt-1".into(),
            signer_challenge: [1; 32],
            peer_challenge: [2; 32],
            signer_fingerprint: [0; 32],
            peer_fingerprint: [0; 32],
            metadata_capabilities: vec!["metadata-v1".into()],
        }
        .bind_live_transport(&sender)
        .await?;
        let receiver_context = DeviceProofContext {
            instance_origin: "https://filebeam.test".into(),
            deployment_id: "test-1".into(),
            account_id: "account-1".into(),
            device_generation: 1,
            signer_device_id: "receiver-1".into(),
            signer_key_version: 1,
            peer_device_id: "sender-1".into(),
            peer_key_version: 1,
            attempt_id: "attempt-1".into(),
            signer_challenge: [2; 32],
            peer_challenge: [1; 32],
            signer_fingerprint: [0; 32],
            peer_fingerprint: [0; 32],
            metadata_capabilities: vec!["metadata-v1".into()],
        }
        .bind_live_transport(&receiver)
        .await?;
        let sender_pin = PinnedDevice::new("sender-1".into(), 1, sender_key.verifying_key())?;
        let receiver_pin = PinnedDevice::new("receiver-1".into(), 1, receiver_key.verifying_key())?;
        let (sender_authority, receiver_authority) = tokio::join!(
            authenticate_metadata_channel(
                sender_channels.metadata.clone(),
                &sender_context,
                &sender_key,
                &receiver_pin
            ),
            authenticate_metadata_channel(
                receiver_channels.metadata.clone(),
                &receiver_context,
                &receiver_key,
                &sender_pin
            ),
        );
        let sender_authority = sender_authority?;
        let receiver_authority = receiver_authority?;

        let transfer_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let item_id = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
        let master = generate_transfer_key()?;
        let metadata = DeviceOfferMetadata {
            sender_device_id: "sender-1".into(),
            sender_key_version: 1,
            receiver_device_id: "receiver-1".into(),
            receiver_key_version: 1,
            transfer_id: transfer_id.into(),
            protocol_version: 1,
            chunk_bytes: 1024,
            encrypted_manifest: "opaque-existing-envelope".into(),
            items: vec![filebeam_transfer::manifest::ManifestMetadataItem {
                id: item_id.into(),
                position: 0,
                chunk_count: 1,
            }],
            manifest_digest: "a".repeat(64),
        };
        let offer = DeviceOffer {
            job_id: "job-1".into(),
            expires_at_unix_seconds: 4_000_000_000,
            metadata,
            transfer_key: URL_SAFE_NO_PAD.encode(&master),
            requested_grant_id: "dropzone-1".into(),
            requested_subdirectory: None,
        };
        let offer_authority = receiver_authority.clone();
        let receiver_metadata = receiver_channels.metadata.clone();
        let receive_offer = tokio::spawn(async move {
            receive_device_offer(
                receiver_metadata,
                &offer_authority,
                &LocalDevice::new("receiver-1".into(), 1)?,
                1,
            )
            .await
        });
        send_device_offer(sender_channels.metadata.clone(), &sender_authority, &offer).await?;
        let received_offer = receive_offer.await??;
        assert_eq!(received_offer.job_id, "job-1");
        received_offer.validate_manifest(
            &filebeam_transfer::manifest::Manifest {
                attached_note: None,
                join_token: None,
                tree: None,
                version: 1,
                items: vec![filebeam_transfer::manifest::ManifestItem {
                    id: item_id.into(),
                    name: "received.txt".into(),
                    mime: "text/plain".into(),
                    size: 19,
                    nonce_prefix: URL_SAFE_NO_PAD.encode([0; 16]),
                    chunk_count: 1,
                    digest: filebeam_transfer::manifest::DigestValue {
                        algorithm: "sha256".into(),
                        value: "0".repeat(64),
                    },
                }],
            },
            &receiver_authority,
        )?;
        let receive_decision =
            tokio::spawn(receive_offer_decision(sender_channels.metadata.clone()));
        send_offer_decision(
            receiver_channels.metadata.clone(),
            &DeviceOfferDecision {
                accepted: true,
                reason: None,
                grant_id: "dropzone-1".into(),
                grant_revision: 1,
            },
        )
        .await?;
        assert!(receive_decision.await??.accepted);

        let source_dir = tempfile::tempdir()?;
        let source = source_dir.path().join("source.txt");
        let plain = b"verified peer bytes".to_vec();
        fs::write(&source, &plain)?;
        let nonce = generate_nonce_prefix()?;
        let source = SourceSpec::Path {
            path: source,
            offset: 0,
            length: plain.len() as u64,
        };
        let item =
            super::super::upload::prepare_peer_item(&source, 1024, &Control::test_factory())?;
        let key = PeerChunkKey {
            transfer_id,
            item_id,
            master_key: &master,
            nonce_prefix: &nonce,
        };
        let prepared = prepare_peer_chunk(&item, &key, 0, 1024, Control::test_factory())?;
        let worker = tokio::spawn(serve_channel(
            sender_channels.file.clone(),
            HashMap::from([((item_id.into(), 0), prepared.ciphertext.clone())]),
        ));
        let ciphertext = request_chunk(
            receiver_channels.file.clone(),
            1,
            item_id.into(),
            0,
            prepared.ciphertext.len() as u64,
        )
        .await?;
        let output = tempfile::tempdir()?;
        let item = PeerReceiveItem {
            id: item_id.into(),
            name: "received.txt".into(),
            size: plain.len() as u64,
            nonce_prefix: URL_SAFE_NO_PAD.encode(nonce),
            chunk_count: 1,
            digest: hex::encode(sha2::Sha256::digest(&plain)),
            target: output.path().join("received.txt"),
        };
        receive_peer_chunk(
            output.path(),
            TEST_CHUNK,
            transfer_id,
            &item,
            &master,
            ciphertext,
        )?;
        super::publish_item(output.path(), 0, &item)?;
        assert_eq!(fs::read(&item.target)?, plain);

        let receipt = DeviceReceipt::sign(
            DeviceReceipt {
                job_id: "job-1".into(),
                transfer_id: transfer_id.into(),
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                manifest_digest: "a".repeat(64),
                grant_id: "dropzone-1".into(),
                grant_revision: 1,
                commit_id: "commit-1".into(),
                committed_at_unix_seconds: 1,
                items: vec![DeviceReceiptItem {
                    item_id: item_id.into(),
                    digest: hex::encode(sha2::Sha256::digest(&plain)),
                    plaintext_bytes: plain.len() as u64,
                }],
                root_outcomes: vec![],
                signature: [0; 64],
            },
            &receiver_key,
        )?;
        let receipts = tempfile::tempdir()?;
        let store = Store::create(receipts.path(), "peer-job")?;
        persist_device_receipt(&store, &receipt)?;
        let replay = load_device_receipt(&store)?.expect("receipt is durable");
        replay.verify(&receiver_key.verifying_key())?;
        assert_eq!(replay, receipt);
        receiver_channels.file.close().await?;
        worker.await??;
        sender.close().await;
        receiver.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn production_sender_and_receiver_run_the_durable_peer_job() -> anyhow::Result<()> {
        let sender = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".into()]).await?;
        let receiver = NativePeer::with_udp_addrs(&[], vec!["127.0.0.1:0".into()]).await?;
        let (offer, sender_channels) = sender.offer_device_channels().await?;
        let answer = receiver.answer(&offer).await?;
        sender.accept_answer(&answer).await?;
        let receiver_channels = receiver.device_receiver_channels().await?;
        wait_for_open(&sender_channels.file).await?;
        wait_for_open(&sender_channels.metadata).await?;
        wait_for_open(&receiver_channels.file).await?;
        wait_for_open(&receiver_channels.metadata).await?;
        let sender_key = ed25519_dalek::SigningKey::from_bytes(&[13; 32]);
        let receiver_key = ed25519_dalek::SigningKey::from_bytes(&[14; 32]);
        let sender_context = DeviceProofContext {
            instance_origin: "https://filebeam.test".into(),
            deployment_id: "test-1".into(),
            account_id: "account-1".into(),
            device_generation: 1,
            signer_device_id: "sender-1".into(),
            signer_key_version: 1,
            peer_device_id: "receiver-1".into(),
            peer_key_version: 1,
            attempt_id: "attempt-2".into(),
            signer_challenge: [1; 32],
            peer_challenge: [2; 32],
            signer_fingerprint: [0; 32],
            peer_fingerprint: [0; 32],
            metadata_capabilities: vec!["metadata-v1".into()],
        }
        .bind_live_transport(&sender)
        .await?;
        let receiver_context = DeviceProofContext {
            instance_origin: "https://filebeam.test".into(),
            deployment_id: "test-1".into(),
            account_id: "account-1".into(),
            device_generation: 1,
            signer_device_id: "receiver-1".into(),
            signer_key_version: 1,
            peer_device_id: "sender-1".into(),
            peer_key_version: 1,
            attempt_id: "attempt-2".into(),
            signer_challenge: [2; 32],
            peer_challenge: [1; 32],
            signer_fingerprint: [0; 32],
            peer_fingerprint: [0; 32],
            metadata_capabilities: vec!["metadata-v1".into()],
        }
        .bind_live_transport(&receiver)
        .await?;
        let sender_pin = PinnedDevice::new("sender-1".into(), 1, sender_key.verifying_key())?;
        let receiver_pin = PinnedDevice::new("receiver-1".into(), 1, receiver_key.verifying_key())?;
        let (sender_authority, receiver_authority) = tokio::join!(
            authenticate_metadata_channel(
                sender_channels.metadata.clone(),
                &sender_context,
                &sender_key,
                &receiver_pin
            ),
            authenticate_metadata_channel(
                receiver_channels.metadata.clone(),
                &receiver_context,
                &receiver_key,
                &sender_pin
            ),
        );
        let sender_authority = sender_authority?;
        let receiver_authority = receiver_authority?;
        let source_dir = tempfile::tempdir()?;
        let source = source_dir.path().join("source.txt");
        fs::write(&source, b"durable production bytes")?;
        let sender_home = tempfile::tempdir()?;
        let sender_store = Store::create(sender_home.path(), "sender-job")?;
        let job = prepare_peer_job(
            &sender_store,
            &[PeerSource {
                name: "received.txt".into(),
                spec: SourceSpec::Path {
                    path: source,
                    offset: 0,
                    length: 24,
                },
            }],
            test_parties(),
            1024,
            Control::test_factory(),
        )?;
        let receiver_home = tempfile::tempdir()?;
        let receiver_store = Store::create(receiver_home.path(), "receiver-job")?;
        let output = tempfile::tempdir()?;
        let handler = TestReceiver {
            base: output.path().to_owned(),
            admits: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            recovered: None,
        };
        let receiver_identity = LocalDevice::new("receiver-1".into(), 1)?;
        let receiver_public = receiver_key.verifying_key();
        let (sent, received) = tokio::join!(
            send_prepared_peer_job(
                &sender_store,
                &job,
                &sender_channels,
                &sender_authority,
                PeerDelivery {
                    grant_id: "dropzone-1".into(),
                    subdirectory: None,
                    expires_at_unix_seconds: 4_000_000_000,
                },
                &receiver_public
            ),
            receive_peer_job(
                &receiver_store,
                &receiver_channels,
                &receiver_authority,
                &receiver_identity,
                &handler,
                &receiver_key
            ),
        );
        let sent = sent?;
        let received = received?;
        assert_eq!(sent, received);
        assert_eq!(
            fs::read(output.path().join("received.txt"))?,
            b"durable production bytes"
        );
        assert!(load_device_receipt(&sender_store)?.is_some());
        assert!(load_device_receipt(&receiver_store)?.is_some());
        let replay_offer = job.offer("dropzone-1".into(), None, 4_000_000_000);
        let replay_sender = async {
            send_device_offer(
                sender_channels.metadata.clone(),
                &sender_authority,
                &replay_offer,
            )
            .await?;
            assert!(
                receive_offer_decision(sender_channels.metadata.clone())
                    .await?
                    .accepted
            );
            let receipt =
                receive_device_receipt(sender_channels.metadata.clone(), &sender_authority).await?;
            send_device_receipt_ack(sender_channels.metadata.clone(), &receipt).await?;
            Ok::<_, anyhow::Error>(receipt)
        };
        let (replayed, sender_replay) = tokio::join!(
            receive_admitted_offer(
                &receiver_store,
                &receiver_channels,
                &receiver_authority,
                &replay_offer,
                &handler,
                &receiver_key,
            ),
            replay_sender,
        );
        assert_eq!(replayed?, sender_replay?);
        assert_eq!(handler.admits.load(Ordering::Relaxed), 1);
        let recovered_store = Store::create(receiver_home.path(), "receiver-recovered")?;
        let recovered_handler = TestReceiver {
            base: output.path().to_owned(),
            admits: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            recovered: Some(received.clone()),
        };
        let recovered_sender = async {
            send_device_offer(
                sender_channels.metadata.clone(),
                &sender_authority,
                &replay_offer,
            )
            .await?;
            assert!(
                receive_offer_decision(sender_channels.metadata.clone())
                    .await?
                    .accepted
            );
            let receipt =
                receive_device_receipt(sender_channels.metadata.clone(), &sender_authority).await?;
            send_device_receipt_ack(sender_channels.metadata.clone(), &receipt).await?;
            Ok::<_, anyhow::Error>(receipt)
        };
        let (recovered, sender_recovered) = tokio::join!(
            receive_admitted_offer(
                &recovered_store,
                &receiver_channels,
                &receiver_authority,
                &replay_offer,
                &recovered_handler,
                &receiver_key,
            ),
            recovered_sender,
        );
        assert_eq!(recovered?, sender_recovered?);
        assert_eq!(recovered_handler.admits.load(Ordering::Relaxed), 0);
        assert_eq!(recovered_handler.commits.load(Ordering::Relaxed), 0);
        assert_eq!(load_device_receipt(&recovered_store)?, Some(received));
        sender.close().await;
        receiver.close().await;
        Ok(())
    }

    #[test]
    fn peer_item_rejects_source_mutation_between_chunks() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("source.bin");
        fs::write(&path, b"abcdefgh")?;
        let source = SourceSpec::Path {
            path: path.clone(),
            offset: 0,
            length: 8,
        };
        let control = Control::test_factory();
        let item = super::super::upload::prepare_peer_item(&source, 4, &control)?;
        let master = generate_transfer_key()?;
        let nonce = generate_nonce_prefix()?;

        let key = PeerChunkKey {
            transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            item_id: "01ARZ3NDEKTSV4RRFFQ69G5FAW",
            master_key: &master,
            nonce_prefix: &nonce,
        };
        prepare_peer_chunk(&item, &key, 0, 4, control.clone())?;
        fs::write(&path, b"abcdWXYZ")?;

        assert!(prepare_peer_chunk(&item, &key, 1, 4, control).is_err());
        Ok(())
    }

    #[test]
    fn prepared_peer_job_reopens_after_source_is_deleted() -> anyhow::Result<()> {
        let source_directory = tempfile::tempdir()?;
        let source_path = source_directory.path().join("source.bin");
        fs::write(&source_path, b"prepared bytes")?;
        let checkpoint_directory = tempfile::tempdir()?;
        let store = Store::create(checkpoint_directory.path(), "peer-job")?;
        let job = prepare_peer_job(
            &store,
            &[PeerSource {
                name: "source.bin".into(),
                spec: SourceSpec::Path {
                    path: source_path.clone(),
                    offset: 0,
                    length: 14,
                },
            }],
            test_parties(),
            4,
            Control::test_factory(),
        )?;
        fs::remove_file(source_path)?;

        let reopened = load_peer_job(&store)?;
        assert_eq!(reopened.job_id(), job.job_id());
        Ok(())
    }

    #[test]
    fn peer_manifest_preserves_the_control_note() -> anyhow::Result<()> {
        let source_directory = tempfile::tempdir()?;
        let source_path = source_directory.path().join("source.bin");
        let bytes = b"note bytes";
        fs::write(&source_path, bytes)?;
        let checkpoint_directory = tempfile::tempdir()?;
        let store = Store::create(checkpoint_directory.path(), "peer-job")?;
        let control = Control::test_factory();
        let note = filebeam_transfer::manifest::AttachedNote {
            text: "Read this first".into(),
            title: Some("Note".into()),
            language: "plain".into(),
        };
        control.set_attached_note(Some(note.clone()));
        let job = prepare_peer_job(
            &store,
            &[PeerSource {
                name: "source.bin".into(),
                spec: SourceSpec::Path {
                    path: source_path,
                    offset: 0,
                    length: bytes.len() as u64,
                },
            }],
            test_parties(),
            4,
            control,
        )?;

        assert_eq!(
            decrypt_peer_manifest(&job.offer("grant".into(), None, 4_000_000_000))?.attached_note,
            Some(note)
        );
        Ok(())
    }

    #[test]
    fn receiver_checkpoint_identity_keeps_manifest_conflicts_in_one_store() {
        let mut offer = DeviceOffer {
            job_id: "job-1".into(),
            expires_at_unix_seconds: 4_000_000_000,
            metadata: DeviceOfferMetadata {
                sender_device_id: "sender-1".into(),
                sender_key_version: 1,
                receiver_device_id: "receiver-1".into(),
                receiver_key_version: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                protocol_version: 1,
                chunk_bytes: 1024,
                encrypted_manifest: "opaque".into(),
                items: vec![],
                manifest_digest: "a".repeat(64),
            },
            transfer_key: URL_SAFE_NO_PAD.encode([0; 32]),
            requested_grant_id: "dropzone-1".into(),
            requested_subdirectory: None,
        };
        let id = receiver_job_id(&offer);
        assert_eq!(id, receiver_job_id(&offer));
        offer.metadata.manifest_digest = "b".repeat(64);
        assert_eq!(id, receiver_job_id(&offer));
    }
}
