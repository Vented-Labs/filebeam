//! Typed custody adapters for registered-device jobs.
//!
//! The host never receives a private key and cannot ask Rust to sign arbitrary
//! bytes. Proof and receipt requests are built from live, validated protocol
//! objects and Rust verifies every returned signature before it is transmitted.

use crate::{ClientError, NativeServices, Result, invalid, operation};
use ed25519_dalek::VerifyingKey;
use filebeam_client_core::device_registry::{
    AccountId, DeploymentId, DeviceGeneration, DeviceId, DeviceRegistry, DeviceStatus, Origin,
    RegistryDevice, RegistryRoute, RegistryScope,
};
use filebeam_transfer_native::device::{
    DeviceProofContext, DeviceReceipt, DeviceReceiptItem, DeviceReceiptRootOutcome, DeviceSigner,
};
use filebeam_transfer_native::{
    protocol::peer::{PeerParties, PeerReceiveGrant, PeerReceiver, PeerSource, prepare_peer_job},
    source::SourceSpec,
    webrtc::{self, DeviceSignaling},
};
mod dropzone;

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_DROPZONE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Clone, uniffi::Record)]
pub struct DeviceJobSnapshot {
    pub id: u64,
    pub state: String,
    pub checkpoint_id: Option<String>,
    pub phase: String,
    pub error: Option<String>,
}

/// Registry-derived trust only. This deliberately contains no transport,
/// credential, manifest, or peer-session data.
#[derive(Clone, uniffi::Record)]
pub struct RegisteredTrustSnapshot {
    pub device_id: String,
    pub pinned: bool,
}

/// The selected content remains outside the binding: a path is app-selected and
/// a provider identity is reopened only through the runtime's source callback.
#[derive(Clone, uniffi::Record)]
pub struct DeviceSendRequest {
    pub source_name: String,
    pub source_path: Option<String>,
    pub provider_identity: Option<String>,
    pub provider_length: Option<u64>,
    pub provider_mutation_token: Option<String>,
    pub target_device_id: String,
    pub destination_alias: String,
    pub expires_at_unix_seconds: u64,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceCoordinatorConfiguration {
    pub deployment_id: String,
    pub local_device_id: String,
    pub local_public_key: Vec<u8>,
    /// App-controlled alias, not a filesystem path supplied by a peer.
    pub dropzone_alias: String,
}

struct Configuration {
    services: Arc<NativeServices>,
    scope: RegistryScope,
    signer: Arc<dyn DeviceSigner>,
    dropzone: PathBuf,
}

#[derive(uniffi::Object)]
pub struct NativeDeviceCoordinator {
    inner: filebeam_client_core::DeviceCoordinator,
    runtime: Arc<crate::NativeRuntime>,
    configuration: Mutex<Option<Configuration>>,
}

#[uniffi::export]
impl NativeDeviceCoordinator {
    #[uniffi::constructor]
    pub fn new(runtime: Arc<crate::NativeRuntime>) -> Self {
        Self {
            inner: filebeam_client_core::DeviceCoordinator::new(runtime.device_runtime.clone()),
            runtime,
            configuration: Mutex::new(None),
        }
    }
    /// Configure one coordinator for one signed-in account/device scope. The
    /// registry cache is always opened from the runtime's encrypted store.
    pub fn configure(
        &self,
        services: Arc<NativeServices>,
        configuration: DeviceCoordinatorConfiguration,
        signer: Arc<dyn NativeDeviceSigner>,
    ) -> Result<()> {
        let account = services.account_session()?;
        let local = DeviceId::parse(configuration.local_device_id).map_err(operation)?;
        let scope = RegistryScope {
            origin: Origin::parse(services.inner.instance().to_owned()).map_err(operation)?,
            deployment: DeploymentId::parse(configuration.deployment_id).map_err(operation)?,
            account: AccountId(account.id),
            generation: DeviceGeneration(0),
            local_device: local,
        };
        if configuration.dropzone_alias.is_empty()
            || configuration.dropzone_alias.contains('/')
            || configuration.dropzone_alias.contains('\\')
            || configuration.dropzone_alias == "."
            || configuration.dropzone_alias == ".."
        {
            return Err(invalid("Dropzone alias must be a single app-managed name"));
        }
        let signer = Arc::new(HostDeviceSigner::new(
            &configuration.local_public_key,
            signer,
        )?);
        let dropzone = self
            .runtime
            .settings
            .state_home
            .join("device-dropzones")
            .join(&configuration.dropzone_alias);
        *self.configuration.lock().unwrap_or_else(|e| e.into_inner()) = Some(Configuration {
            services,
            scope,
            signer,
            dropzone,
        });
        Ok(())
    }

    /// Prepare immutable ciphertext artifacts, rendezvous through DeviceSignaling,
    /// then hand the authenticated session to the core-owned coordinator.
    pub fn start_sender(&self, request: DeviceSendRequest) -> Result<u64> {
        let configured = self.configuration()?;
        let (store, scope) = sync_registry(&configured, &self.runtime)?;
        let registry = DeviceRegistry::open(
            &store,
            scope.clone(),
            std::time::Duration::from_secs(7 * 24 * 60 * 60),
        )
        .map_err(operation)?;
        let target = DeviceId::parse(request.target_device_id.clone()).map_err(operation)?;
        let target_info = registry
            .pinned_target(&scope, &target, RegistryRoute::Instance, now())
            .map_err(operation)?;
        let source = peer_source(&request)?;
        let checkpoint = format!("device-send-{}", uuid::Uuid::new_v4());
        let store = self
            .runtime
            .registry_store(&checkpoint)
            .map_err(operation)?;
        let (tx, _) = std::sync::mpsc::channel();
        let control =
            filebeam_client_core::control::Control::new(self.runtime.settings.clone(), tx);
        let local = registry.local_device().map_err(operation)?;
        let prepared = prepare_peer_job(
            &store,
            &[source],
            PeerParties {
                sender_device_id: local.id().to_owned(),
                sender_key_version: local.key_version(),
                receiver_device_id: target_info.id.as_str().to_owned(),
                receiver_key_version: target_info.key_version,
            },
            webrtc::FRAME_PAYLOAD_BYTES as u64,
            control,
        )
        .map_err(operation)?;
        let credential = configured
            .services
            .device_credentials
            .as_ref()
            .ok_or_else(|| invalid("Device jobs require credential custody"))?
            .load(configured.services.inner.instance(), local.id().to_owned())
            .map_err(operation)?;
        let instance = configured.services.inner.instance();
        let signaling = DeviceSignaling::new(reqwest::Client::new(), &instance, &credential)
            .map_err(operation)?;
        let session = registry
            .peer_session(&scope, &target, RegistryRoute::Instance, now())
            .map_err(operation)?;
        let (peer, connection, channels) =
            filebeam_transfer_native::runtime::shared_tokio_runtime()
                .block_on(webrtc::connect_device_initiator(
                    &signaling,
                    target.as_str(),
                    &[],
                    self.runtime.settings.webrtc_relay_only,
                ))
                .map_err(operation)?;
        let authenticated = filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(session.authenticate(
                peer,
                channels,
                configured.signer.clone(),
                &connection.id,
            ))
            .map_err(operation)?;
        Ok(self
            .inner
            .start_sender(
                authenticated,
                prepared,
                request.destination_alias,
                request.expires_at_unix_seconds,
            )
            .0)
    }

    /// Poll authorized rendezvous records and start one receiver for each. Data
    /// is staged under the configured private dropzone, never a remote path.
    pub fn poll_receivers(&self) -> Result<Vec<u64>> {
        let configured = self.configuration()?;
        let (store, scope) = sync_registry(&configured, &self.runtime)?;
        let registry = DeviceRegistry::open(
            &store,
            scope.clone(),
            std::time::Duration::from_secs(7 * 24 * 60 * 60),
        )
        .map_err(operation)?;
        let local = registry.local_device().map_err(operation)?;
        let credential = configured
            .services
            .device_credentials
            .as_ref()
            .ok_or_else(|| invalid("Device jobs require credential custody"))?
            .load(configured.services.inner.instance(), local.id().to_owned())
            .map_err(operation)?;
        let instance = configured.services.inner.instance();
        let signaling = DeviceSignaling::new(reqwest::Client::new(), &instance, &credential)
            .map_err(operation)?;
        let incoming = filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(signaling.incoming())
            .map_err(operation)?;
        let mut ids = Vec::new();
        for connection in incoming.connections {
            let peer_id = DeviceId::parse(connection.source_device_id).map_err(operation)?;
            let session = registry
                .peer_session(&scope, &peer_id, RegistryRoute::Instance, now())
                .map_err(operation)?;
            let servers = filebeam_transfer_native::runtime::shared_tokio_runtime()
                .block_on(signaling.ice_servers(&connection.id))
                .map_err(operation)?;
            let (peer, _, channels) = filebeam_transfer_native::runtime::shared_tokio_runtime()
                .block_on(webrtc::connect_device_responder(
                    &signaling,
                    &connection.id,
                    &servers,
                    self.runtime.settings.webrtc_relay_only,
                ))
                .map_err(operation)?;
            let authenticated = filebeam_transfer_native::runtime::shared_tokio_runtime()
                .block_on(session.authenticate(
                    peer,
                    channels,
                    configured.signer.clone(),
                    &connection.id,
                ))
                .map_err(operation)?;
            ids.push(
                self.inner
                    .start_receiver(
                        authenticated,
                        Arc::new(DropzoneReceiver::new(
                            configured.dropzone.clone(),
                            self.runtime.settings.state_home.clone(),
                            self.runtime.settings.checkpoint_secret_store.clone(),
                        )),
                        configured.signer.clone(),
                    )
                    .0,
            );
        }
        Ok(ids)
    }
    /// Refreshes the shared registry and reports which active identities are
    /// pinned by the authenticated local registry. UI must not infer this from
    /// an account-visible `active` status.
    pub fn registered_trust_snapshot(&self) -> Result<Vec<RegisteredTrustSnapshot>> {
        let configured = self.configuration()?;
        let (store, scope) = sync_registry(&configured, &self.runtime)?;
        let registry = DeviceRegistry::open(
            &store,
            scope.clone(),
            std::time::Duration::from_secs(7 * 24 * 60 * 60),
        )
        .map_err(operation)?;
        Ok(registry
            .trust_snapshot()
            .map_err(operation)?
            .into_iter()
            .map(|(device_id, pinned)| RegisteredTrustSnapshot { device_id, pinned })
            .collect())
    }
    /// Advertise only from a live receiver loop after it has proven it can poll
    /// the private dropzone with unlocked credential custody.
    pub fn advertise_presence(&self, ready: bool) -> Result<()> {
        let configured = self.configuration()?;
        let local = configured.scope.local_device.as_str().to_owned();
        configured
            .services
            .device_advertise_presence(local, ready, vec!["peer-receive".into()])?;
        Ok(())
    }
    pub fn snapshots(&self) -> Vec<DeviceJobSnapshot> {
        self.inner
            .snapshots()
            .into_iter()
            .map(|snapshot| DeviceJobSnapshot {
                id: snapshot.id.0,
                state: format!("{:?}", snapshot.job.state).to_lowercase(),
                checkpoint_id: snapshot.job.checkpoint_id,
                phase: filebeam_client_core::phase_name(snapshot.job.progress.phase).into(),
                error: snapshot.job.error,
            })
            .collect()
    }
    pub fn pause(&self, id: u64) -> Result<()> {
        self.inner
            .pause(filebeam_client_core::JobId(id))
            .map_err(operation)
    }
    pub fn cancel(&self, id: u64) -> Result<()> {
        self.inner
            .cancel(filebeam_client_core::JobId(id))
            .map_err(operation)
    }
    pub fn is_idle(&self) -> bool {
        self.inner.is_idle()
    }
    pub fn expire(&self, timeout_millis: u64) -> Vec<u64> {
        self.inner
            .expire(std::time::Duration::from_millis(timeout_millis))
            .checkpointed
            .into_iter()
            .map(|id| id.0)
            .collect()
    }
}

impl NativeDeviceCoordinator {
    fn configuration(&self) -> Result<Configuration> {
        let guard = self.configuration.lock().unwrap_or_else(|e| e.into_inner());
        let value = guard
            .as_ref()
            .ok_or_else(|| invalid("Configure the device coordinator before starting jobs"))?;
        Ok(Configuration {
            services: value.services.clone(),
            scope: value.scope.clone(),
            signer: value.signer.clone(),
            dropzone: value.dropzone.clone(),
        })
    }
}

fn peer_source(request: &DeviceSendRequest) -> Result<PeerSource> {
    if request.source_name.is_empty()
        || request.target_device_id.is_empty()
        || request.destination_alias.is_empty()
        || request.expires_at_unix_seconds <= now() as u64
    {
        return Err(invalid("Device send request is incomplete or expired"));
    }
    let spec = match (&request.source_path, &request.provider_identity) {
        (Some(path), None) => {
            let length = fs::metadata(path).map_err(operation)?.len();
            SourceSpec::Path {
                path: PathBuf::from(path),
                offset: 0,
                length,
            }
        }
        (None, Some(identity)) => SourceSpec::Provider {
            identity: identity.clone(),
            offset: 0,
            length: request
                .provider_length
                .ok_or_else(|| invalid("Provider source length is required"))?,
            mutation_token: request
                .provider_mutation_token
                .clone()
                .ok_or_else(|| invalid("Provider mutation token is required"))?,
        },
        _ => {
            return Err(invalid(
                "Specify exactly one source path or provider identity",
            ));
        }
    };
    Ok(PeerSource {
        name: request.source_name.clone(),
        spec,
    })
}

fn sync_registry(
    configuration: &Configuration,
    runtime: &crate::NativeRuntime,
) -> Result<(filebeam_transfer_native::checkpoint::Store, RegistryScope)> {
    let local = configuration.scope.local_device.clone();
    let credential = configuration
        .services
        .device_credentials
        .as_ref()
        .ok_or_else(|| invalid("Device jobs require credential custody"))?
        .load(
            configuration.services.inner.instance(),
            local.as_str().to_owned(),
        )
        .map_err(operation)?;
    let snapshot = configuration
        .services
        .inner
        .devices()
        .device_sync(&credential)
        .map_err(operation)?;
    let scope = RegistryScope {
        generation: DeviceGeneration(snapshot.generation),
        ..configuration.scope.clone()
    };
    let mut digest = sha2::Sha256::new();
    use sha2::Digest as _;
    digest.update(scope.origin.as_str().as_bytes());
    digest.update([0]);
    digest.update(scope.deployment.as_str().as_bytes());
    digest.update(scope.account.0.to_be_bytes());
    digest.update(scope.generation.0.to_be_bytes());
    let id = format!("device-registry-{}", hex::encode(digest.finalize()));
    let store = runtime.registry_store(&id).map_err(operation)?;
    let mut registry = DeviceRegistry::open(
        &store,
        scope.clone(),
        std::time::Duration::from_secs(7 * 24 * 60 * 60),
    )
    .map_err(operation)?;
    let freshness = snapshot.offline_freshness_seconds.ok_or_else(|| {
        invalid("Authenticated device registry response is missing freshness policy")
    })?;
    registry
        .ingest(
            filebeam_client_core::device_registry::RegistryResponse {
                revision: snapshot.revision,
                generation: DeviceGeneration(snapshot.generation),
                offline_freshness_seconds: freshness,
                devices: snapshot
                    .devices
                    .into_iter()
                    .map(|device| {
                        Ok(RegistryDevice {
                            id: DeviceId::parse(device.id)?,
                            key_version: device.key_version,
                            public_key: device.public_key,
                            generation: DeviceGeneration(device.generation),
                            status: match device.status.as_str() {
                                "active" => DeviceStatus::Active,
                                "pending" => DeviceStatus::Pending,
                                "revoked" => DeviceStatus::Revoked,
                                _ => anyhow::bail!("invalid device registry status"),
                            },
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()
                    .map_err(operation)?,
            },
            now(),
        )
        .map_err(operation)?;
    Ok((store, scope))
}

#[derive(Clone)]
struct DropzoneReceiver {
    root: PathBuf,
    state_root: PathBuf,
    secrets: Option<Arc<dyn filebeam_transfer_native::checkpoint::SecretStore>>,
}

impl DropzoneReceiver {
    fn new(
        root: PathBuf,
        state_root: PathBuf,
        secrets: Option<Arc<dyn filebeam_transfer_native::checkpoint::SecretStore>>,
    ) -> Self {
        Self {
            root,
            state_root,
            secrets,
        }
    }
}

impl PeerReceiver for DropzoneReceiver {
    fn admit(
        &self,
        offer: &filebeam_transfer_native::device::DeviceOffer,
        manifest: &filebeam_transfer::manifest::Manifest,
    ) -> anyhow::Result<PeerReceiveGrant> {
        ensure_private_directory(&self.root)?;
        let expected = manifest.items.iter().try_fold(0u64, |total, item| {
            total
                .checked_add(item.size)
                .ok_or_else(|| anyhow::anyhow!("dropzone size overflows"))
        })?;
        if expected > MAX_DROPZONE_BYTES {
            anyhow::bail!("peer receive exceeds the app dropzone limit")
        }
        let journal = dropzone::Journal::open(
            &self.state_root,
            self.secrets.clone(),
            &self.root,
            offer,
            manifest,
        )?;
        let stage = journal.stage_path()?;
        // The encrypted tree controls only names inside this newly-created,
        // app-owned stage. It never selects a destination outside the dropzone.
        let tree_paths = manifest.tree.as_ref().map(|tree| {
            tree.entries
                .iter()
                .filter_map(|entry| {
                    entry
                        .item_id
                        .as_ref()
                        .map(|id| (id.as_str(), entry.path.as_slice()))
                })
                .collect::<HashMap<_, _>>()
        });
        if let Some(tree) = &manifest.tree {
            for entry in &tree.entries {
                let path = safe_tree_path(&stage, &entry.path)?;
                if entry.item_id.is_none() {
                    fs::create_dir_all(path)?;
                }
            }
        }
        let targets = manifest
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let path = tree_paths
                    .as_ref()
                    .and_then(|paths| paths.get(item.id.as_str()).copied())
                    .map(|components| safe_tree_path(&stage, components))
                    .transpose()?
                    .unwrap_or_else(|| stage.join(format!("item-{index}")));
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                Ok(path)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(PeerReceiveGrant {
            id: journal.grant_id().to_owned(),
            revision: journal.grant_revision(),
            staging_base: stage,
            targets,
            staging: None,
        })
    }
    fn recover_receipt(
        &self,
        offer: &filebeam_transfer_native::device::DeviceOffer,
    ) -> anyhow::Result<Option<DeviceReceipt>> {
        let journal = dropzone::Journal::reopen(
            &self.state_root,
            self.secrets.clone(),
            &self.root,
            &offer.job_id,
        );
        match journal {
            Ok(journal) => journal.recover_receipt(),
            Err(error) if error.to_string().contains("journal is unavailable") => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn commit(
        &self,
        grant: &PeerReceiveGrant,
        job_id: &str,
    ) -> anyhow::Result<Vec<DeviceReceiptRootOutcome>> {
        let journal =
            dropzone::Journal::reopen(&self.state_root, self.secrets.clone(), &self.root, job_id)?;
        journal.commit(grant)
    }
    fn persist_receipt(
        &self,
        grant: &PeerReceiveGrant,
        receipt: &DeviceReceipt,
    ) -> anyhow::Result<()> {
        let journal = dropzone::Journal::reopen(
            &self.state_root,
            self.secrets.clone(),
            &self.root,
            &receipt.job_id,
        )?;
        journal.persist_receipt(grant, receipt)
    }
}

fn ensure_private_directory(path: &std::path::Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => anyhow::bail!("dropzone path is not a private directory"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            ensure_private_directory(path)
        }
        Err(error) => Err(error.into()),
    }
}

fn safe_tree_path(root: &std::path::Path, components: &[String]) -> anyhow::Result<PathBuf> {
    if components.is_empty()
        || components.iter().any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component.contains('/')
                || component.contains('\\')
        })
    {
        anyhow::bail!("peer tree path is invalid")
    }
    Ok(components
        .iter()
        .fold(root.to_path_buf(), |path, component| path.join(component)))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(i64::MAX)
}

#[derive(Clone, uniffi::Record)]
pub struct DevicePeerProofRequest {
    pub instance_origin: String,
    pub deployment_id: String,
    pub account_id: String,
    pub device_generation: u64,
    pub signer_device_id: String,
    pub signer_key_version: u32,
    pub peer_device_id: String,
    pub peer_key_version: u32,
    pub attempt_id: String,
    pub signer_challenge: Vec<u8>,
    pub peer_challenge: Vec<u8>,
    pub signer_fingerprint: Vec<u8>,
    pub peer_fingerprint: Vec<u8>,
    pub metadata_capabilities: Vec<String>,
    /// Rust-generated canonical transcript for this already validated request.
    pub signing_payload: Vec<u8>,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceReceiptItemRequest {
    pub item_id: String,
    pub digest: String,
    pub plaintext_bytes: u64,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceReceiptRootOutcomeRequest {
    pub source_root: String,
    pub destination_components: Vec<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceReceiptRequest {
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
    pub items: Vec<DeviceReceiptItemRequest>,
    pub root_outcomes: Vec<DeviceReceiptRootOutcomeRequest>,
    /// Rust-generated canonical transcript for this already validated receipt.
    pub signing_payload: Vec<u8>,
}

/// Device-key custody for peer proof and receipt statements. Implementations
/// must sign the documented canonical form of these typed requests with the
/// registered Ed25519 key; there is deliberately no raw-message method.
#[uniffi::export(foreign)]
pub trait NativeDeviceSigner: Send + Sync {
    fn sign_peer_proof(
        &self,
        request: DevicePeerProofRequest,
    ) -> std::result::Result<Vec<u8>, ClientError>;
    fn sign_peer_receipt(
        &self,
        request: DeviceReceiptRequest,
    ) -> std::result::Result<Vec<u8>, ClientError>;
}

pub(crate) struct HostDeviceSigner {
    key: VerifyingKey,
    host: Arc<dyn NativeDeviceSigner>,
}

impl HostDeviceSigner {
    pub(crate) fn new(public_key: &[u8], host: Arc<dyn NativeDeviceSigner>) -> Result<Self> {
        let key = public_key
            .try_into()
            .map_err(|_| invalid("Device public key must be 32 bytes"))?;
        Ok(Self {
            key: VerifyingKey::from_bytes(&key)
                .map_err(|_| invalid("Device public key is invalid"))?,
            host,
        })
    }
}

impl DeviceSigner for HostDeviceSigner {
    fn verifying_key(&self) -> VerifyingKey {
        self.key
    }

    fn sign_proof(&self, context: &DeviceProofContext) -> anyhow::Result<[u8; 64]> {
        // Canonicalization also validates every identifier, capability, and bound
        // transport fingerprint before platform custody sees the request.
        context.canonical_bytes()?;
        self.host
            .sign_peer_proof(DevicePeerProofRequest {
                instance_origin: context.instance_origin.clone(),
                deployment_id: context.deployment_id.clone(),
                account_id: context.account_id.clone(),
                device_generation: context.device_generation,
                signer_device_id: context.signer_device_id.clone(),
                signer_key_version: context.signer_key_version,
                peer_device_id: context.peer_device_id.clone(),
                peer_key_version: context.peer_key_version,
                attempt_id: context.attempt_id.clone(),
                signer_challenge: context.signer_challenge.to_vec(),
                peer_challenge: context.peer_challenge.to_vec(),
                signer_fingerprint: context.signer_fingerprint.to_vec(),
                peer_fingerprint: context.peer_fingerprint.to_vec(),
                metadata_capabilities: context.metadata_capabilities.clone(),
                signing_payload: context.canonical_bytes()?,
            })
            .map_err(operation)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Device proof signature must be 64 bytes"))
    }

    fn sign_receipt(&self, receipt: &DeviceReceipt) -> anyhow::Result<[u8; 64]> {
        receipt.validate()?;
        self.host
            .sign_peer_receipt(DeviceReceiptRequest {
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
                items: receipt.items.iter().map(receipt_item).collect(),
                root_outcomes: receipt.root_outcomes.iter().map(root_outcome).collect(),
                signing_payload: DeviceReceipt::canonical_bytes(receipt)?,
            })
            .map_err(operation)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Device receipt signature must be 64 bytes"))
    }
}

fn receipt_item(item: &DeviceReceiptItem) -> DeviceReceiptItemRequest {
    DeviceReceiptItemRequest {
        item_id: item.item_id.clone(),
        digest: item.digest.clone(),
        plaintext_bytes: item.plaintext_bytes,
    }
}

fn root_outcome(outcome: &DeviceReceiptRootOutcome) -> DeviceReceiptRootOutcomeRequest {
    DeviceReceiptRootOutcomeRequest {
        source_root: outcome.source_root.clone(),
        destination_components: outcome.destination_components.clone(),
    }
}
