//! Device-only metadata admission layered on the existing transfer format.
//! This module never signs arbitrary caller-provided byte strings: proofs bind
//! typed connection context before metadata or ciphertext is released.

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use filebeam_transfer::manifest::{Manifest, ManifestMetadataItem};
#[cfg(test)]
use sha2::{Digest, Sha256};

const PROOF_DOMAIN: &[u8] = b"filebeam:device-proof:v1";
const PROOF_VERSION: u8 = 1;
const MAX_CAPABILITIES: usize = 16;
const MAX_CAPABILITY_BYTES: usize = 64;
const MAX_IDENTIFIER_BYTES: usize = 255;
const MAX_ENCRYPTED_MANIFEST_BYTES: usize = 16 * 1024 * 1024 * 2;
const AUTH_STREAM_ID: u32 = 0;
const OFFER_STREAM_ID: u32 = 1;
const ACCEPT_STREAM_ID: u32 = 2;
const RECEIPT_STREAM_ID: u32 = 3;
const HELLO_STREAM_ID: u32 = 4;
const RECEIPT_ACK_STREAM_ID: u32 = 5;

/// Private peer offer sent only after device proof admission. `transfer_key`
/// is the existing transfer master key encoded as base64url; it never reaches
/// rendezvous, checkpoints outside the encrypted Store, or a public API.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceOffer {
    pub job_id: String,
    pub expires_at_unix_seconds: u64,
    pub metadata: DeviceOfferMetadata,
    pub transfer_key: String,
    pub requested_grant_id: String,
    pub requested_subdirectory: Option<String>,
}

impl std::fmt::Debug for DeviceOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceOffer").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceOfferMetadata {
    pub sender_device_id: String,
    pub sender_key_version: u32,
    pub receiver_device_id: String,
    pub receiver_key_version: u32,
    pub transfer_id: String,
    pub protocol_version: u8,
    pub chunk_bytes: u64,
    pub encrypted_manifest: String,
    pub items: Vec<ManifestMetadataItem>,
    pub manifest_digest: String,
}

impl DeviceOffer {
    pub fn resolved_metadata(&self) -> PeerResolvedMetadata {
        PeerResolvedMetadata {
            sender_device_id: self.metadata.sender_device_id.clone(),
            sender_key_version: self.metadata.sender_key_version,
            receiver_device_id: self.metadata.receiver_device_id.clone(),
            receiver_key_version: self.metadata.receiver_key_version,
            transfer_id: self.metadata.transfer_id.clone(),
            protocol_version: self.metadata.protocol_version,
            chunk_bytes: self.metadata.chunk_bytes,
            encrypted_manifest: self.metadata.encrypted_manifest.clone(),
            items: self.metadata.items.clone(),
        }
    }

    /// The existing manifest crypto owner decrypts the envelope. This method
    /// then applies the same content/binding/admission validator as a public
    /// transfer before the peer worker may request any ciphertext.
    pub fn validate_manifest(
        &self,
        manifest: &Manifest,
        authority: &VerifiedDeviceAuthority,
    ) -> Result<()> {
        self.resolved_metadata()
            .validate_manifest(manifest, authority)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceOfferDecision {
    pub accepted: bool,
    pub reason: Option<String>,
    pub grant_id: String,
    pub grant_revision: u64,
}

/// Signed receiver acknowledgement for a fully verified peer item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceReceipt {
    pub job_id: String,
    pub transfer_id: String,
    pub sender_device_id: String,
    pub sender_key_version: u32,
    pub receiver_device_id: String,
    pub receiver_key_version: u32,
    pub manifest_digest: String,
    pub grant_id: String,
    pub grant_revision: u64,
    pub commit_id: String,
    pub committed_at_unix_seconds: u64,
    pub items: Vec<DeviceReceiptItem>,
    pub root_outcomes: Vec<DeviceReceiptRootOutcome>,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceReceiptItem {
    pub item_id: String,
    pub digest: String,
    pub plaintext_bytes: u64,
}

/// Receiver-selected names for committed top-level roots, relative to the
/// grant. It never contains a receiver filesystem path.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceReceiptRootOutcome {
    pub source_root: String,
    pub destination_components: Vec<String>,
}

impl DeviceReceipt {
    pub fn canonical_bytes(receipt: &Self) -> Result<Vec<u8>> {
        let mut bytes = b"filebeam:device-receipt:v1".to_vec();
        for value in [
            &receipt.job_id,
            &receipt.transfer_id,
            &receipt.sender_device_id,
            &receipt.receiver_device_id,
            &receipt.manifest_digest,
            &receipt.grant_id,
            &receipt.commit_id,
        ] {
            append_string(&mut bytes, value)?;
        }
        bytes.extend_from_slice(&receipt.sender_key_version.to_be_bytes());
        bytes.extend_from_slice(&receipt.receiver_key_version.to_be_bytes());
        bytes.extend_from_slice(&receipt.grant_revision.to_be_bytes());
        bytes.extend_from_slice(&receipt.committed_at_unix_seconds.to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(receipt.items.len())?.to_be_bytes());
        for item in &receipt.items {
            append_string(&mut bytes, &item.item_id)?;
            append_string(&mut bytes, &item.digest)?;
            bytes.extend_from_slice(&item.plaintext_bytes.to_be_bytes());
        }
        bytes.extend_from_slice(&u32::try_from(receipt.root_outcomes.len())?.to_be_bytes());
        for outcome in &receipt.root_outcomes {
            append_string(&mut bytes, &outcome.source_root)?;
            bytes.extend_from_slice(
                &u32::try_from(outcome.destination_components.len())?.to_be_bytes(),
            );
            for component in &outcome.destination_components {
                append_string(&mut bytes, component)?;
            }
        }
        Ok(bytes)
    }

    pub fn sign(receipt: Self, key: &SigningKey) -> Result<Self> {
        receipt.sign_with(key)
    }

    /// Requests a signature only for this validated receipt and verifies it
    /// locally before the receipt can reach the peer.
    pub fn sign_with(mut self, signer: &dyn DeviceSigner) -> Result<Self> {
        self.validate()?;
        self.signature = signer.sign_receipt(&self)?;
        self.verify(&signer.verifying_key())?;
        Ok(self)
    }

    pub fn verify(&self, key: &VerifyingKey) -> Result<()> {
        key.verify(
            &Self::canonical_bytes(self)?,
            &Signature::from_bytes(&self.signature),
        )
        .context("verify device receipt")
    }

    pub fn validate(&self) -> Result<()> {
        if !valid_identifier(&self.job_id)
            || !canonical_ulid(&self.transfer_id)
            || !valid_identifier(&self.sender_device_id)
            || !valid_identifier(&self.receiver_device_id)
            || self.sender_key_version == 0
            || self.receiver_key_version == 0
            || !valid_digest(&self.manifest_digest)
            || !valid_identifier(&self.grant_id)
            || !valid_identifier(&self.commit_id)
            || self.items.len() > 100_000
        {
            bail!("invalid device receipt")
        }
        for item in &self.items {
            if !canonical_ulid(&item.item_id) || !valid_digest(&item.digest) {
                bail!("invalid device receipt item")
            }
        }
        if self.root_outcomes.len() > 100_000
            || self.root_outcomes.iter().any(|outcome| {
                !valid_destination_component(&outcome.source_root)
                    || outcome.destination_components.is_empty()
                    || outcome.destination_components.len() > 64
                    || outcome
                        .destination_components
                        .iter()
                        .any(|component| !valid_destination_component(component))
            })
        {
            bail!("invalid device receipt root outcome")
        }
        Ok(())
    }
}

fn valid_destination_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', '\0'])
        && !value.chars().any(char::is_control)
}

/// The locally constructed transcript for a single WebRTC connection attempt.
/// Fingerprints must come from the active DTLS connection, not merely the SDP
/// values received through signaling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceProofContext {
    pub instance_origin: String,
    pub deployment_id: String,
    pub account_id: String,
    pub device_generation: u64,
    pub signer_device_id: String,
    pub signer_key_version: u32,
    pub peer_device_id: String,
    pub peer_key_version: u32,
    pub attempt_id: String,
    pub signer_challenge: [u8; 32],
    pub peer_challenge: [u8; 32],
    pub signer_fingerprint: [u8; 32],
    pub peer_fingerprint: [u8; 32],
    pub metadata_capabilities: Vec<String>,
}

/// Custody boundary for registered-device keys. Implementations receive typed,
/// Rust-validated protocol objects rather than arbitrary bytes.
pub trait DeviceSigner: Send + Sync {
    fn verifying_key(&self) -> VerifyingKey;
    fn sign_proof(&self, context: &DeviceProofContext) -> Result<[u8; 64]>;
    fn sign_receipt(&self, receipt: &DeviceReceipt) -> Result<[u8; 64]>;
}

impl DeviceSigner for SigningKey {
    fn verifying_key(&self) -> VerifyingKey {
        self.verifying_key()
    }

    fn sign_proof(&self, context: &DeviceProofContext) -> Result<[u8; 64]> {
        Ok(self.sign(&context.canonical_bytes()?).to_bytes())
    }

    fn sign_receipt(&self, receipt: &DeviceReceipt) -> Result<[u8; 64]> {
        Ok(self
            .sign(&DeviceReceipt::canonical_bytes(receipt)?)
            .to_bytes())
    }
}

impl DeviceProofContext {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(PROOF_DOMAIN);
        bytes.push(PROOF_VERSION);
        for value in [
            &self.instance_origin,
            &self.deployment_id,
            &self.account_id,
            &self.signer_device_id,
            &self.peer_device_id,
            &self.attempt_id,
        ] {
            append_string(&mut bytes, value)?;
        }
        bytes.extend_from_slice(&self.device_generation.to_be_bytes());
        bytes.extend_from_slice(&self.signer_key_version.to_be_bytes());
        bytes.extend_from_slice(&self.peer_key_version.to_be_bytes());
        bytes.extend_from_slice(&self.signer_challenge);
        bytes.extend_from_slice(&self.peer_challenge);
        bytes.extend_from_slice(&self.signer_fingerprint);
        bytes.extend_from_slice(&self.peer_fingerprint);
        bytes.push(u8::try_from(self.metadata_capabilities.len())?);
        for capability in &self.metadata_capabilities {
            append_string(&mut bytes, capability)?;
        }
        Ok(bytes)
    }

    fn validate(&self) -> Result<()> {
        if !valid_origin(&self.instance_origin)
            || !valid_identifier(&self.deployment_id)
            || !valid_identifier(&self.account_id)
            || !valid_identifier(&self.signer_device_id)
            || !valid_identifier(&self.peer_device_id)
            || !valid_identifier(&self.attempt_id)
            || self.signer_key_version == 0
            || self.peer_key_version == 0
            || !valid_capabilities(&self.metadata_capabilities)
        {
            bail!("invalid device proof context")
        }
        Ok(())
    }

    /// Return the exact context the other endpoint must sign. Both sides use
    /// this transformation rather than trusting role labels from a message.
    pub fn peer_view(&self) -> Self {
        Self {
            instance_origin: self.instance_origin.clone(),
            deployment_id: self.deployment_id.clone(),
            account_id: self.account_id.clone(),
            device_generation: self.device_generation,
            signer_device_id: self.peer_device_id.clone(),
            signer_key_version: self.peer_key_version,
            peer_device_id: self.signer_device_id.clone(),
            peer_key_version: self.signer_key_version,
            attempt_id: self.attempt_id.clone(),
            signer_challenge: self.peer_challenge,
            peer_challenge: self.signer_challenge,
            signer_fingerprint: self.peer_fingerprint,
            peer_fingerprint: self.signer_fingerprint,
            metadata_capabilities: self.metadata_capabilities.clone(),
        }
    }

    /// Populate certificate bindings only from the active DTLS transport.
    pub async fn bind_live_transport(mut self, peer: &crate::webrtc::NativePeer) -> Result<Self> {
        let (local, remote) = peer.certificate_fingerprints().await?;
        self.signer_fingerprint = local;
        self.peer_fingerprint = remote;
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceProof {
    pub signer_device_id: String,
    pub signer_key_version: u32,
    pub signature: [u8; 64],
}

pub fn sign_device_proof(
    context: &DeviceProofContext,
    signer: &dyn DeviceSigner,
) -> Result<DeviceProof> {
    context.validate()?;
    let proof = DeviceProof {
        signer_device_id: context.signer_device_id.clone(),
        signer_key_version: context.signer_key_version,
        signature: signer.sign_proof(context)?,
    };
    verify_device_proof(context, &signer.verifying_key(), &proof)?;
    Ok(proof)
}

/// Evidence returned only after a registered, pinned peer's typed proof has
/// verified. It is intentionally required by peer metadata validation.
#[derive(Clone, Debug)]
pub struct VerifiedDeviceAuthority {
    peer_device_id: String,
    peer_key_version: u32,
    tree_capability: bool,
}

/// A current registry or locally persisted trust pin for the exact peer that
/// this connection expects. It is input to admission, not proof evidence.
#[derive(Clone)]
pub(crate) struct PinnedDevice {
    device_id: String,
    key_version: u32,
    public_key: VerifyingKey,
}

/// Immutable local registration identity used to reject offers addressed to a
/// different device before any manifest, staging, or grant work occurs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDevice {
    device_id: String,
    key_version: u32,
}

/// Locally verified account and device scope used to build proof transcripts.
/// It intentionally has no constructor from peer-provided hello fields.
#[derive(Clone)]
pub struct DeviceAuthenticationScope {
    instance_origin: String,
    deployment_id: String,
    account_id: String,
    device_generation: u64,
    local_device: LocalDevice,
    metadata_capabilities: Vec<String>,
}

impl DeviceAuthenticationScope {
    pub fn new(
        instance_origin: String,
        deployment_id: String,
        account_id: String,
        device_generation: u64,
        local_device: LocalDevice,
        metadata_capabilities: Vec<String>,
    ) -> Result<Self> {
        if !valid_origin(&instance_origin)
            || !valid_identifier(&deployment_id)
            || !valid_identifier(&account_id)
            || !valid_capabilities(&metadata_capabilities)
            || !metadata_capabilities
                .iter()
                .any(|value| value == "metadata-v1")
        {
            bail!("invalid device authentication scope")
        }
        Ok(Self {
            instance_origin,
            deployment_id,
            account_id,
            device_generation,
            local_device,
            metadata_capabilities,
        })
    }
}

impl LocalDevice {
    pub fn new(device_id: String, key_version: u32) -> Result<Self> {
        if !valid_identifier(&device_id) || key_version == 0 {
            bail!("invalid local device identity")
        }
        Ok(Self {
            device_id,
            key_version,
        })
    }

    pub fn id(&self) -> &str {
        &self.device_id
    }

    pub const fn key_version(&self) -> u32 {
        self.key_version
    }
}

impl PinnedDevice {
    pub(crate) fn new(
        device_id: String,
        key_version: u32,
        public_key: VerifyingKey,
    ) -> Result<Self> {
        if !valid_identifier(&device_id) || key_version == 0 {
            bail!("invalid pinned device")
        }
        Ok(Self {
            device_id,
            key_version,
            public_key,
        })
    }

    fn matches(&self, context: &DeviceProofContext) -> bool {
        self.device_id == context.signer_device_id && self.key_version == context.signer_key_version
    }

    pub fn id(&self) -> &str {
        &self.device_id
    }

    pub const fn key_version(&self) -> u32 {
        self.key_version
    }
}

impl VerifiedDeviceAuthority {
    pub fn peer_device_id(&self) -> &str {
        &self.peer_device_id
    }

    pub const fn peer_key_version(&self) -> u32 {
        self.peer_key_version
    }

    pub const fn tree_capability(&self) -> bool {
        self.tree_capability
    }
}

/// Raw cryptographic verification intentionally does not create a device
/// admission authority. Only the authenticated channel path below combines it
/// with a current/pinned peer record and live transport binding.
pub fn verify_device_proof(
    context: &DeviceProofContext,
    expected_public_key: &VerifyingKey,
    proof: &DeviceProof,
) -> Result<()> {
    context.validate()?;
    if proof.signer_device_id != context.signer_device_id
        || proof.signer_key_version != context.signer_key_version
    {
        bail!("device proof identity does not match the authenticated context")
    }
    expected_public_key
        .verify(
            &context.canonical_bytes()?,
            &Signature::from_bytes(&proof.signature),
        )
        .map_err(|_| anyhow::anyhow!("device proof signature is invalid"))?;
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceProofWire {
    signer_device_id: String,
    signer_key_version: u32,
    signature: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceHelloWire {
    device_id: String,
    key_version: u32,
    attempt_id: String,
    challenge: String,
    metadata_capabilities: Vec<String>,
}

/// Establish fresh, mutually contributed proof inputs before exchanging the
/// signed proof. The peer hello is only a bounded consistency check: registry
/// authority remains the caller-supplied pin.
pub(crate) async fn authenticate_device_connection(
    peer: &crate::webrtc::NativePeer,
    channels: &crate::webrtc::DeviceChannels,
    scope: &DeviceAuthenticationScope,
    signer: &dyn DeviceSigner,
    expected_peer: &PinnedDevice,
    attempt_id: &str,
) -> Result<VerifiedDeviceAuthority> {
    if !valid_identifier(attempt_id) {
        bail!("invalid device connection attempt")
    }
    let mut challenge = [0u8; 32];
    getrandom::getrandom(&mut challenge).context("generate device proof challenge")?;
    let hello = DeviceHelloWire {
        device_id: scope.local_device.id().to_owned(),
        key_version: scope.local_device.key_version(),
        attempt_id: attempt_id.to_owned(),
        challenge: URL_SAFE_NO_PAD.encode(challenge),
        metadata_capabilities: scope.metadata_capabilities.clone(),
    };
    let encoded = serde_json::to_vec(&hello).context("serialize device hello")?;
    let send =
        crate::webrtc::send_metadata_blob(channels.metadata.clone(), HELLO_STREAM_ID, &encoded);
    let receive = crate::webrtc::receive_metadata_blob(channels.metadata.clone(), HELLO_STREAM_ID);
    let (_, encoded) = tokio::try_join!(send, receive)?;
    let peer_hello: DeviceHelloWire =
        serde_json::from_slice(&encoded).context("parse device hello")?;
    if peer_hello.device_id != expected_peer.id()
        || peer_hello.key_version != expected_peer.key_version()
        || peer_hello.attempt_id != attempt_id
        || !valid_capabilities(&peer_hello.metadata_capabilities)
    {
        bail!("device hello does not match the expected peer")
    }
    let peer_challenge = URL_SAFE_NO_PAD
        .decode(peer_hello.challenge)
        .context("device hello challenge is not base64url")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("device hello challenge has an invalid length"))?;
    let metadata_capabilities = scope
        .metadata_capabilities
        .iter()
        .filter(|capability| {
            peer_hello
                .metadata_capabilities
                .binary_search(*capability)
                .is_ok()
        })
        .cloned()
        .collect::<Vec<_>>();
    if !metadata_capabilities
        .iter()
        .any(|value| value == "metadata-v1")
    {
        bail!("device peer does not support required metadata capability")
    }
    let context = DeviceProofContext {
        instance_origin: scope.instance_origin.clone(),
        deployment_id: scope.deployment_id.clone(),
        account_id: scope.account_id.clone(),
        device_generation: scope.device_generation,
        signer_device_id: scope.local_device.id().to_owned(),
        signer_key_version: scope.local_device.key_version(),
        peer_device_id: expected_peer.id().to_owned(),
        peer_key_version: expected_peer.key_version(),
        attempt_id: attempt_id.to_owned(),
        signer_challenge: challenge,
        peer_challenge,
        signer_fingerprint: [0; 32],
        peer_fingerprint: [0; 32],
        metadata_capabilities,
    }
    .bind_live_transport(peer)
    .await?;
    authenticate_metadata_channel(channels.metadata.clone(), &context, signer, expected_peer).await
}

/// Exchange the typed device proof over the dedicated metadata channel. Both
/// sides must call this before sending an offer, manifest, key, or chunk.
pub(crate) async fn authenticate_metadata_channel(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    context: &DeviceProofContext,
    signer: &dyn DeviceSigner,
    expected_peer: &PinnedDevice,
) -> Result<VerifiedDeviceAuthority> {
    if !expected_peer.matches(&context.peer_view()) {
        bail!("pinned device does not match the authenticated context")
    }
    let proof = sign_device_proof(context, signer)?;
    let wire = DeviceProofWire {
        signer_device_id: proof.signer_device_id,
        signer_key_version: proof.signer_key_version,
        signature: URL_SAFE_NO_PAD.encode(proof.signature),
    };
    let encoded = serde_json::to_vec(&wire).context("serialize device proof")?;
    let send = crate::webrtc::send_metadata_blob(channel.clone(), AUTH_STREAM_ID, &encoded);
    let receive = crate::webrtc::receive_metadata_blob(channel, AUTH_STREAM_ID);
    let (_, encoded) = tokio::try_join!(send, receive)?;
    let wire: DeviceProofWire = serde_json::from_slice(&encoded).context("parse device proof")?;
    let signature = URL_SAFE_NO_PAD
        .decode(wire.signature)
        .context("device proof signature is not base64url")?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| anyhow::anyhow!("device proof signature has an invalid length"))?;
    verify_device_proof(
        &context.peer_view(),
        &expected_peer.public_key,
        &DeviceProof {
            signer_device_id: wire.signer_device_id,
            signer_key_version: wire.signer_key_version,
            signature,
        },
    )?;
    Ok(VerifiedDeviceAuthority {
        peer_device_id: context.peer_device_id.clone(),
        peer_key_version: context.peer_key_version,
        tree_capability: context
            .metadata_capabilities
            .iter()
            .any(|capability| capability == "tree-v1"),
    })
}

pub(crate) async fn send_device_offer(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    authority: &VerifiedDeviceAuthority,
    offer: &DeviceOffer,
) -> Result<()> {
    validate_offer(
        offer,
        Some((&authority.peer_device_id, authority.peer_key_version)),
        None,
    )?;
    crate::webrtc::send_metadata_blob(channel, OFFER_STREAM_ID, &serde_json::to_vec(offer)?).await
}

pub(crate) async fn receive_device_offer(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    authority: &VerifiedDeviceAuthority,
    local_device: &LocalDevice,
    now_unix_seconds: u64,
) -> Result<DeviceOffer> {
    let offer: DeviceOffer = serde_json::from_slice(
        &crate::webrtc::receive_metadata_blob(channel, OFFER_STREAM_ID).await?,
    )
    .context("parse device offer")?;
    validate_offer(
        &offer,
        Some((local_device.id(), local_device.key_version())),
        Some((&authority.peer_device_id, authority.peer_key_version)),
    )?;
    if offer.expires_at_unix_seconds <= now_unix_seconds {
        bail!("device offer has expired")
    }
    Ok(offer)
}

pub(crate) async fn send_offer_decision(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    decision: &DeviceOfferDecision,
) -> Result<()> {
    if decision.grant_id.is_empty()
        || decision
            .reason
            .as_ref()
            .is_some_and(|reason| reason.len() > 256)
    {
        bail!("invalid device offer decision")
    }
    crate::webrtc::send_metadata_blob(channel, ACCEPT_STREAM_ID, &serde_json::to_vec(decision)?)
        .await
}

pub(crate) async fn receive_offer_decision(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
) -> Result<DeviceOfferDecision> {
    let decision: DeviceOfferDecision = serde_json::from_slice(
        &crate::webrtc::receive_metadata_blob(channel, ACCEPT_STREAM_ID).await?,
    )
    .context("parse device offer decision")?;
    if decision.grant_id.is_empty()
        || decision
            .reason
            .as_ref()
            .is_some_and(|reason| reason.len() > 256)
    {
        bail!("invalid device offer decision")
    }
    Ok(decision)
}

pub fn persist_device_receipt(
    store: &crate::checkpoint::Store,
    receipt: &DeviceReceipt,
) -> Result<()> {
    receipt.validate()?;
    store.save_named("device-receipt-v1", &DeviceReceiptWire::from(receipt))
}

pub fn load_device_receipt(store: &crate::checkpoint::Store) -> Result<Option<DeviceReceipt>> {
    store
        .load_named::<DeviceReceiptWire>("device-receipt-v1")?
        .map(DeviceReceipt::try_from)
        .transpose()
}

pub(crate) async fn send_device_receipt(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    receipt: &DeviceReceipt,
) -> Result<()> {
    receipt.validate()?;
    crate::webrtc::send_metadata_blob(
        channel,
        RECEIPT_STREAM_ID,
        &serde_json::to_vec(&DeviceReceiptWire::from(receipt))?,
    )
    .await
}

pub(crate) async fn receive_device_receipt(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    expected_sender: &VerifiedDeviceAuthority,
) -> Result<DeviceReceipt> {
    let wire: DeviceReceiptWire = serde_json::from_slice(
        &crate::webrtc::receive_metadata_blob(channel, RECEIPT_STREAM_ID).await?,
    )
    .context("parse device receipt")?;
    let receipt = DeviceReceipt::try_from(wire)?;
    receipt.validate()?;
    if receipt.receiver_device_id != expected_sender.peer_device_id
        || receipt.receiver_key_version != expected_sender.peer_key_version
    {
        bail!("device receipt identity does not match the authenticated peer")
    }
    Ok(receipt)
}

/// Confirm that the sender has durably accepted this exact signed receipt.
/// The authenticated metadata channel supplies peer authentication; repeating
/// the receipt binding prevents an acknowledgement from another job or replay.
pub(crate) async fn send_device_receipt_ack(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    receipt: &DeviceReceipt,
) -> Result<()> {
    receipt.validate()?;
    crate::webrtc::send_metadata_blob(
        channel,
        RECEIPT_ACK_STREAM_ID,
        &serde_json::to_vec(&DeviceReceiptAckWire::from(receipt))?,
    )
    .await
}

/// Wait for an acknowledgement of this exact signed receipt before releasing
/// the receiver's transport ownership.
pub(crate) async fn receive_device_receipt_ack(
    channel: std::sync::Arc<dyn webrtc::data_channel::DataChannel>,
    receipt: &DeviceReceipt,
) -> Result<()> {
    receipt.validate()?;
    let acknowledgement: DeviceReceiptAckWire = serde_json::from_slice(
        // The offer was authenticated and committed before this point. Some
        // native stacks may redeliver it before the acknowledgement arrives.
        &crate::webrtc::receive_metadata_blob_ignoring(
            channel,
            RECEIPT_ACK_STREAM_ID,
            Some(OFFER_STREAM_ID),
        )
        .await
        .context("receive device receipt acknowledgement frame")?,
    )
    .context("parse device receipt acknowledgement")?;
    if acknowledgement != DeviceReceiptAckWire::from(receipt) {
        bail!("device receipt acknowledgement does not match the persisted receipt")
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceReceiptWire {
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
    items: Vec<DeviceReceiptItem>,
    #[serde(default)]
    root_outcomes: Vec<DeviceReceiptRootOutcome>,
    signature: String,
}

#[derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceReceiptAckWire {
    job_id: String,
    transfer_id: String,
    commit_id: String,
    signature: String,
}

impl From<&DeviceReceipt> for DeviceReceiptAckWire {
    fn from(value: &DeviceReceipt) -> Self {
        Self {
            job_id: value.job_id.clone(),
            transfer_id: value.transfer_id.clone(),
            commit_id: value.commit_id.clone(),
            signature: URL_SAFE_NO_PAD.encode(value.signature),
        }
    }
}

impl From<&DeviceReceipt> for DeviceReceiptWire {
    fn from(value: &DeviceReceipt) -> Self {
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
            signature: URL_SAFE_NO_PAD.encode(value.signature),
        }
    }
}

impl TryFrom<DeviceReceiptWire> for DeviceReceipt {
    type Error = anyhow::Error;
    fn try_from(value: DeviceReceiptWire) -> Result<Self> {
        let signature: [u8; 64] = URL_SAFE_NO_PAD
            .decode(value.signature)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid device receipt signature"))?;
        Ok(Self {
            job_id: value.job_id,
            transfer_id: value.transfer_id,
            sender_device_id: value.sender_device_id,
            sender_key_version: value.sender_key_version,
            receiver_device_id: value.receiver_device_id,
            receiver_key_version: value.receiver_key_version,
            manifest_digest: value.manifest_digest,
            grant_id: value.grant_id,
            grant_revision: value.grant_revision,
            commit_id: value.commit_id,
            committed_at_unix_seconds: value.committed_at_unix_seconds,
            items: value.items,
            root_outcomes: value.root_outcomes,
            signature,
        })
    }
}

fn validate_offer(
    offer: &DeviceOffer,
    expected_receiver: Option<(&str, u32)>,
    expected_sender: Option<(&str, u32)>,
) -> Result<()> {
    let metadata = &offer.metadata;
    if !valid_identifier(&offer.job_id)
        || offer.expires_at_unix_seconds == 0
        || !valid_identifier(&metadata.receiver_device_id)
        || metadata.receiver_key_version == 0
        || !valid_digest(&metadata.manifest_digest)
        || !valid_identifier(&offer.requested_grant_id)
        || offer.requested_subdirectory.as_ref().is_some_and(|path| {
            path.len() > 4096
                || path.starts_with('/')
                || path
                    .split('/')
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))
        })
    {
        bail!("invalid authenticated device offer")
    }
    if expected_receiver.is_some_and(|(id, version)| {
        metadata.receiver_device_id != id || metadata.receiver_key_version != version
    }) || expected_sender.is_some_and(|(id, version)| {
        metadata.sender_device_id != id || metadata.sender_key_version != version
    }) {
        bail!("device offer identity does not match the authenticated session")
    }
    let key = URL_SAFE_NO_PAD
        .decode(&offer.transfer_key)
        .context("device offer transfer key is not base64url")?;
    if key.len() != 32 {
        bail!("device offer transfer key has an invalid length")
    }
    PeerResolvedMetadata {
        sender_device_id: metadata.sender_device_id.clone(),
        sender_key_version: metadata.sender_key_version,
        receiver_device_id: metadata.receiver_device_id.clone(),
        receiver_key_version: metadata.receiver_key_version,
        transfer_id: metadata.transfer_id.clone(),
        protocol_version: metadata.protocol_version,
        chunk_bytes: metadata.chunk_bytes,
        encrypted_manifest: metadata.encrypted_manifest.clone(),
        items: metadata.items.clone(),
    }
    .validate_descriptor_fields()
}

/// The peer-side descriptor supplied only after `VerifiedDeviceAuthority` is
/// established. The encrypted manifest stays opaque until the existing
/// manifest crypto path opens it.
#[derive(Clone, Debug)]
pub struct PeerResolvedMetadata {
    pub sender_device_id: String,
    pub sender_key_version: u32,
    pub receiver_device_id: String,
    pub receiver_key_version: u32,
    pub transfer_id: String,
    pub protocol_version: u8,
    pub chunk_bytes: u64,
    pub encrypted_manifest: String,
    pub items: Vec<ManifestMetadataItem>,
}

impl PeerResolvedMetadata {
    fn validate_descriptor_fields(&self) -> Result<()> {
        if !valid_identifier(&self.sender_device_id)
            || self.sender_key_version == 0
            || !valid_identifier(&self.receiver_device_id)
            || self.receiver_key_version == 0
            || !canonical_ulid(&self.transfer_id)
            || self.protocol_version != 1
            || self.chunk_bytes == 0
            || self.encrypted_manifest.is_empty()
            || self.encrypted_manifest.len() > MAX_ENCRYPTED_MANIFEST_BYTES
        {
            bail!("invalid authenticated peer metadata")
        }
        Ok(())
    }

    fn validate_descriptor(&self, authority: &VerifiedDeviceAuthority) -> Result<()> {
        self.validate_descriptor_fields()?;
        if self.sender_device_id != authority.peer_device_id
            || self.sender_key_version != authority.peer_key_version
        {
            bail!("peer metadata identity does not match the authenticated session")
        }
        Ok(())
    }

    pub fn validate_manifest(
        &self,
        manifest: &Manifest,
        authority: &VerifiedDeviceAuthority,
    ) -> Result<()> {
        self.validate_descriptor(authority)?;
        filebeam_transfer::manifest::validate_verified_device_manifest_with_tree_capability(
            manifest,
            self.chunk_bytes,
            &self.items,
            authority.tree_capability,
        )
        .map_err(anyhow::Error::msg)
    }
}

fn append_string(output: &mut Vec<u8>, value: &str) -> Result<()> {
    let length = u16::try_from(value.len()).context("device proof field is too large")?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn valid_origin(value: &str) -> bool {
    let Some((scheme, authority)) = value.split_once("://") else {
        return false;
    };
    matches!(scheme, "https" | "http")
        && !authority.is_empty()
        && !authority.contains(['/', '?', '#', '@'])
        && authority.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_capabilities(values: &[String]) -> bool {
    values.len() <= MAX_CAPABILITIES
        && !values.windows(2).any(|pair| pair[0] >= pair[1])
        && values.iter().all(|value| {
            !value.is_empty()
                && value.len() <= MAX_CAPABILITY_BYTES
                && value.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'-')
                })
        })
}

fn canonical_ulid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 26
        && matches!(bytes[0], b'0'..=b'7')
        && bytes.iter().skip(1).all(|byte| {
            matches!(byte, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z')
        })
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_binds_verified_plaintext_and_identity() -> Result<()> {
        let key = SigningKey::from_bytes(&[9; 32]);
        let receipt = DeviceReceipt::sign(
            DeviceReceipt {
                job_id: "job-1".into(),
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                sender_device_id: "desktop-1".into(),
                sender_key_version: 1,
                receiver_device_id: "laptop-1".into(),
                receiver_key_version: 2,
                manifest_digest: "1".repeat(64),
                grant_id: "dropzone-1".into(),
                grant_revision: 1,
                commit_id: "commit-1".into(),
                committed_at_unix_seconds: 1,
                items: vec![DeviceReceiptItem {
                    item_id: "01ARZ3NDEKTSV4RRFFQ69G5FAW".into(),
                    digest: hex::encode(Sha256::digest(b"verified bytes")),
                    plaintext_bytes: 14,
                }],
                root_outcomes: vec![DeviceReceiptRootOutcome {
                    source_root: "report.txt".into(),
                    destination_components: vec!["reports".into(), "report.txt (1)".into()],
                }],
                signature: [0; 64],
            },
            &key,
        )?;
        receipt.verify(&key.verifying_key())?;
        let mut altered = receipt.clone();
        altered.items[0].digest = "00".repeat(32);
        assert!(altered.verify(&key.verifying_key()).is_err());
        let mut altered = receipt.clone();
        altered.root_outcomes[0].destination_components[1] = "report.txt (2)".into();
        assert!(altered.verify(&key.verifying_key()).is_err());
        let wire = DeviceReceiptWire::from(&receipt);
        let replay = DeviceReceipt::try_from(wire)?;
        assert_eq!(replay, receipt);
        Ok(())
    }
    use filebeam_transfer::manifest::{DigestValue, ManifestItem};

    fn context() -> DeviceProofContext {
        DeviceProofContext {
            instance_origin: "https://filebeam.test".into(),
            deployment_id: "test-1".into(),
            account_id: "account-1".into(),
            device_generation: 3,
            signer_device_id: "desktop-1".into(),
            signer_key_version: 1,
            peer_device_id: "laptop-1".into(),
            peer_key_version: 2,
            attempt_id: "attempt-1".into(),
            signer_challenge: [1; 32],
            peer_challenge: [2; 32],
            signer_fingerprint: [3; 32],
            peer_fingerprint: [4; 32],
            metadata_capabilities: vec!["metadata-v1".into(), "tree-v1".into()],
        }
    }

    #[test]
    fn proof_rejects_replay_and_channel_binding_substitution() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let context = context();
        let proof = sign_device_proof(&context, &key).unwrap();
        assert!(verify_device_proof(&context, &key.verifying_key(), &proof).is_ok());

        let mut replay = context.clone();
        replay.attempt_id = "attempt-2".into();
        assert!(verify_device_proof(&replay, &key.verifying_key(), &proof).is_err());
        let mut substituted = context;
        substituted.peer_fingerprint[0] ^= 1;
        assert!(verify_device_proof(&substituted, &key.verifying_key(), &proof).is_err());
    }

    #[test]
    fn peer_metadata_requires_verified_authority_and_shared_manifest_validation() {
        let context = context();
        let authority = VerifiedDeviceAuthority {
            peer_device_id: context.signer_device_id.clone(),
            peer_key_version: context.signer_key_version,
            tree_capability: false,
        };
        let metadata = PeerResolvedMetadata {
            sender_device_id: "desktop-1".into(),
            sender_key_version: 1,
            receiver_device_id: "laptop-1".into(),
            receiver_key_version: 2,
            transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            protocol_version: 1,
            chunk_bytes: 16,
            encrypted_manifest: "opaque-existing-envelope".into(),
            items: vec![ManifestMetadataItem {
                id: "item".into(),
                position: 0,
                chunk_count: 1,
            }],
        };
        let manifest = Manifest {
            attached_note: None,
            join_token: None,
            tree: None,
            version: 1,
            items: vec![ManifestItem {
                id: "item".into(),
                name: "file.txt".into(),
                mime: "text/plain".into(),
                size: 1,
                nonce_prefix: "AAAAAAAAAAAAAAAAAAAAAA".into(),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: "0".repeat(64),
                },
            }],
        };
        assert!(metadata.validate_manifest(&manifest, &authority).is_ok());
    }
}
