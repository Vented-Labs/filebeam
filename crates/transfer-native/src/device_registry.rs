//! Durable authorization and pin state for registered-device peers.
//!
//! This module deliberately consumes typed, authenticated sync results.  It
//! does not fetch them: callers must classify HTTP and decode failures before
//! updating this cache.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    checkpoint::Store,
    control::Control,
    device::{
        DeviceAuthenticationScope, DeviceOffer, DeviceReceipt, DeviceSigner, LocalDevice,
        PinnedDevice, VerifiedDeviceAuthority, authenticate_device_connection,
    },
    protocol::peer::{
        PeerPreparedJob, PeerReceiveGrant, PeerReceiver, receive_admitted_offer,
        receive_authenticated_peer_offer, send_prepared_peer_job,
    },
    webrtc::{DeviceChannels, NativePeer},
};
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const RECORD: &str = "device-registry-v1.json";
const REFRESH: i64 = 30;
const FAILURE: i64 = 60;
const MAX_OFFLINE: i64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Origin(String);

impl Origin {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let host = value.strip_prefix("https://").unwrap_or_default();
        if !value.starts_with("https://")
            || value.ends_with('/')
            || value.contains(['?', '#', '@'])
            || value.len() > 255
            || host.is_empty()
            || !host.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b':')
            })
        {
            bail!("origin must be a normalized HTTPS origin");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeploymentId(String);

impl DeploymentId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 255 || value.contains('\0') {
            bail!("invalid deployment ID");
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceGeneration(pub u64);

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.len() != 26
            || !value
                .bytes()
                .all(|c| matches!(c, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
        {
            bail!("device ID must be a ULID");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegistryScope {
    pub origin: Origin,
    pub deployment: DeploymentId,
    pub account: AccountId,
    pub generation: DeviceGeneration,
    pub local_device: DeviceId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    Pending,
    Active,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDevice {
    pub id: DeviceId,
    pub key_version: u32,
    pub public_key: String,
    pub status: DeviceStatus,
    pub generation: DeviceGeneration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryResponse {
    pub revision: u64,
    pub generation: DeviceGeneration,
    pub offline_freshness_seconds: u64,
    pub devices: Vec<RegistryDevice>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncFailure {
    Network,
    Unauthorized,
    Forbidden,
    Malformed,
    Disabled,
    GenerationMismatch,
    PinMismatch,
}

impl SyncFailure {
    fn terminal(self) -> bool {
        !matches!(self, Self::Network)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryRoute {
    Instance,
    OfflineLan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryGate {
    Allowed,
    RefreshRequired,
    Denied(SyncFailure),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Pin {
    device: DeviceId,
    key_version: u32,
    public_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Cache {
    scope: RegistryScope,
    revision: u64,
    last_success: i64,
    offline_freshness: i64,
    devices: Vec<RegistryDevice>,
    pins: Vec<Pin>,
    denied: Option<SyncFailureWire>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum SyncFailureWire {
    Unauthorized,
    Forbidden,
    Malformed,
    Disabled,
    GenerationMismatch,
    PinMismatch,
}

impl From<SyncFailure> for Option<SyncFailureWire> {
    fn from(value: SyncFailure) -> Self {
        match value {
            SyncFailure::Network => None,
            SyncFailure::Unauthorized => Some(SyncFailureWire::Unauthorized),
            SyncFailure::Forbidden => Some(SyncFailureWire::Forbidden),
            SyncFailure::Malformed => Some(SyncFailureWire::Malformed),
            SyncFailure::Disabled => Some(SyncFailureWire::Disabled),
            SyncFailure::GenerationMismatch => Some(SyncFailureWire::GenerationMismatch),
            SyncFailure::PinMismatch => Some(SyncFailureWire::PinMismatch),
        }
    }
}

impl From<SyncFailureWire> for SyncFailure {
    fn from(value: SyncFailureWire) -> Self {
        match value {
            SyncFailureWire::Unauthorized => Self::Unauthorized,
            SyncFailureWire::Forbidden => Self::Forbidden,
            SyncFailureWire::Malformed => Self::Malformed,
            SyncFailureWire::Disabled => Self::Disabled,
            SyncFailureWire::GenerationMismatch => Self::GenerationMismatch,
            SyncFailureWire::PinMismatch => Self::PinMismatch,
        }
    }
}

/// Per-profile durable registry state. `Store` encrypts and authenticates this
/// record with the host's `SecretStore`; this module never writes a side cache.
///
/// ```compile_fail
/// use filebeam_transfer_native::device::{PinnedDevice, authenticate_metadata_channel};
///
/// // External callers cannot self-pin a peer or turn a proof into transfer authority.
/// let _ = (PinnedDevice, authenticate_metadata_channel);
/// ```
pub struct DeviceRegistry<'a> {
    store: &'a Store,
    cache: Arc<Mutex<Cache>>,
    offline_limit: Arc<Mutex<i64>>,
}

struct SharedRegistry {
    scope: RegistryScope,
    cache: Weak<Mutex<Cache>>,
    offline_limit: Weak<Mutex<i64>>,
}

fn shared_registries() -> &'static Mutex<HashMap<PathBuf, SharedRegistry>> {
    static REGISTRIES: OnceLock<Mutex<HashMap<PathBuf, SharedRegistry>>> = OnceLock::new();
    REGISTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// An unauthenticated connection attempt whose identity was selected from a
/// current registry pin. It cannot be constructed by daemon callers.
pub struct RegistryPeerSession {
    scope: RegistryScope,
    peer: DeviceId,
    route: RegistryRoute,
    pin: PinnedDevice,
    receiver_key: VerifyingKey,
    cache: Arc<Mutex<Cache>>,
    _offline_limit: Arc<Mutex<i64>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceTarget {
    pub id: DeviceId,
    pub key_version: u32,
}

/// A live proof bound to a concrete native transport. Runtime peer transfer
/// entry points accept this instead of caller-supplied proof authority.
pub struct AuthenticatedPeerSession {
    peer: Arc<NativePeer>,
    channels: DeviceChannels,
    authority: VerifiedDeviceAuthority,
    local_device: LocalDevice,
    receiver_key: VerifyingKey,
    session: Arc<RegistryPeerSession>,
}

impl<'a> DeviceRegistry<'a> {
    pub fn open(store: &'a Store, scope: RegistryScope, offline_limit: Duration) -> Result<Self> {
        let requested = i64::try_from(offline_limit.as_secs()).unwrap_or(i64::MAX);
        let offline_limit = requested.min(MAX_OFFLINE);
        let path = store.path().canonicalize()?;
        let mut registries = shared_registries()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(shared) = registries.get(&path) {
            if shared.scope != scope {
                bail!("registry cache belongs to another account, origin, or generation");
            }
            if let (Some(cache), Some(limit)) =
                (shared.cache.upgrade(), shared.offline_limit.upgrade())
            {
                let mut limit_guard = limit.lock().unwrap_or_else(|error| error.into_inner());
                *limit_guard = (*limit_guard).min(offline_limit);
                let mut cache_guard = cache.lock().unwrap_or_else(|error| error.into_inner());
                cache_guard.offline_freshness = cache_guard.offline_freshness.min(*limit_guard);
                drop(cache_guard);
                drop(limit_guard);
                return Ok(Self {
                    store,
                    cache,
                    offline_limit: limit,
                });
            }
        }
        let cache = match store.load_named(RECORD)? {
            Some(cache) => {
                let cache: Cache = cache;
                if cache.scope != scope {
                    bail!("registry cache belongs to another account, origin, or generation");
                }
                cache
            }
            None => Cache {
                scope: scope.clone(),
                revision: 0,
                last_success: 0,
                offline_freshness: offline_limit,
                devices: vec![],
                pins: vec![],
                denied: None,
            },
        };
        let cache = Arc::new(Mutex::new(cache));
        let limit = Arc::new(Mutex::new(offline_limit));
        registries.insert(
            path,
            SharedRegistry {
                scope,
                cache: Arc::downgrade(&cache),
                offline_limit: Arc::downgrade(&limit),
            },
        );
        Ok(Self {
            store,
            cache,
            offline_limit: limit,
        })
    }

    pub fn revision(&self) -> u64 {
        self.cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .revision
    }

    /// A presentation-safe view of pins retained by this shared registry.
    /// Account-visible activity is intentionally not treated as trust.
    pub fn trust_snapshot(&self) -> Result<Vec<(String, bool)>> {
        let cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        Ok(cache
            .devices
            .iter()
            .map(|device| {
                let pinned = device.status == DeviceStatus::Active
                    && cache.pins.iter().any(|pin| {
                        pin.device == device.id
                            && pin.key_version == device.key_version
                            && pin.public_key == device.public_key
                    });
                (device.id.as_str().to_owned(), pinned)
            })
            .collect())
    }

    pub fn ingest(&mut self, response: RegistryResponse, now: i64) -> Result<()> {
        let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if now < cache.last_success {
            drop(cache);
            return self.deny(SyncFailure::Malformed);
        }
        if response.generation != cache.scope.generation {
            drop(cache);
            return self.deny(SyncFailure::GenerationMismatch);
        }
        if response.revision < cache.revision || response.offline_freshness_seconds == 0 {
            drop(cache);
            return self.deny(SyncFailure::Malformed);
        }
        if response.devices.iter().any(|device| {
            device.generation != response.generation
                || device.key_version == 0
                || decode_key(&device.public_key).is_err()
        }) || has_duplicate_ids(&response.devices)
        {
            drop(cache);
            return self.deny(SyncFailure::Malformed);
        }
        if response
            .devices
            .iter()
            .find(|device| device.id == cache.scope.local_device)
            .is_none_or(|device| device.status != DeviceStatus::Active)
        {
            drop(cache);
            return self.deny(SyncFailure::Forbidden);
        }
        for pin in &cache.pins {
            let Some(device) = response
                .devices
                .iter()
                .find(|device| device.id == pin.device)
            else {
                drop(cache);
                return self.deny(SyncFailure::Malformed);
            };
            if device.status == DeviceStatus::Revoked {
                drop(cache);
                return self.deny(SyncFailure::Forbidden);
            }
            if device.status != DeviceStatus::Active
                || device.key_version != pin.key_version
                || device.public_key != pin.public_key
            {
                drop(cache);
                return self.deny(SyncFailure::PinMismatch);
            }
        }
        cache.revision = response.revision;
        cache.last_success = now;
        cache.offline_freshness = i64::try_from(response.offline_freshness_seconds)
            .unwrap_or(i64::MAX)
            .min(
                *self
                    .offline_limit
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()),
            )
            .min(MAX_OFFLINE);
        cache.devices = response.devices;
        cache.denied = None;
        self.persist_cache(&cache)
    }

    /// Record an HTTP/decode outcome. Anything other than a connectivity error
    /// is durable terminal evidence and can never be retried as offline trust.
    pub fn fail(&mut self, failure: SyncFailure) -> Result<()> {
        if failure.terminal() {
            let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
            cache.denied = failure.into();
            self.persist_cache(&cache)?;
        }
        Ok(())
    }

    /// Query at admission, at least every refresh interval, and before commit.
    pub fn gate(&self, route: RegistryRoute, now: i64) -> RegistryGate {
        gate_cache(
            &self.cache.lock().unwrap_or_else(|error| error.into_inner()),
            route,
            now,
        )
    }

    /// Pins only an active key from a current authenticated registry after the
    /// caller's out-of-band ceremony has verified that exact device/key.
    pub fn approve_pairing(&mut self, peer: &DeviceId, now: i64) -> Result<()> {
        if self.gate(RegistryRoute::Instance, now) != RegistryGate::Allowed {
            bail!("registry authorization is not current");
        }
        let device = active_device(
            &self.cache.lock().unwrap_or_else(|error| error.into_inner()),
            peer,
        )?
        .clone();
        self.insert_pin(Pin {
            device: device.id,
            key_version: device.key_version,
            public_key: device.public_key,
        })
    }

    /// Import a peer introduced by an already pinned, active introducer. The
    /// introduction is a detached Ed25519 statement over its exact scope/key;
    /// candidates cannot self-introduce.
    pub fn import_introduction(&mut self, introduction: PeerIntroduction, now: i64) -> Result<()> {
        if self.gate(RegistryRoute::Instance, now) != RegistryGate::Allowed {
            bail!("registry authorization is not current");
        }
        if introduction.scope
            != self
                .cache
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .scope
        {
            bail!("introduction scope does not match registry");
        }
        let cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        let introducer = active_pin(&cache, &introduction.introducer)?.clone();
        let active = active_device(&cache, &introduction.introducer)?;
        if active.key_version != introducer.key_version
            || active.public_key != introducer.public_key
        {
            drop(cache);
            return self.deny(SyncFailure::PinMismatch);
        }
        let key = decode_key(&introducer.public_key)?;
        key.verify(
            &introduction.bytes()?,
            &Signature::from_bytes(&introduction.signature),
        )
        .map_err(|_| anyhow::anyhow!("invalid peer introduction signature"))?;
        let candidate = active_device(&cache, &introduction.candidate)?.clone();
        drop(cache);
        if candidate.key_version != introduction.key_version
            || candidate.public_key != introduction.public_key
        {
            bail!("introduction does not bind the current candidate key");
        }
        self.insert_pin(Pin {
            device: candidate.id,
            key_version: candidate.key_version,
            public_key: candidate.public_key,
        })
    }

    /// Creates the native proof input only after every registry and local pin
    /// predicate has been rechecked for this exact scope and peer identity.
    pub(crate) fn pinned_peer(
        &self,
        scope: &RegistryScope,
        peer: &DeviceId,
        route: RegistryRoute,
        now: i64,
    ) -> Result<PinnedDevice> {
        let cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if scope != &cache.scope {
            bail!("peer scope does not match registry");
        }
        if gate_cache(&cache, route, now) != RegistryGate::Allowed {
            bail!("registry authorization is not current");
        }
        let pin = active_pin(&cache, peer)?;
        let active = active_device(&cache, peer)?;
        if active.key_version != pin.key_version || active.public_key != pin.public_key {
            bail!("pinned peer no longer matches registry");
        }
        PinnedDevice::new(
            peer.as_str().to_owned(),
            pin.key_version,
            decode_key(&pin.public_key)?,
        )
    }

    /// Starts the only production path to a device-transfer authority. The
    /// returned object keeps observing this registry's shared, serialized state.
    pub fn peer_session(
        &self,
        scope: &RegistryScope,
        peer: &DeviceId,
        route: RegistryRoute,
        now: i64,
    ) -> Result<RegistryPeerSession> {
        let pin = self.pinned_peer(scope, peer, route, now)?;
        let receiver_key = {
            let cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
            decode_key(&active_pin(&cache, peer)?.public_key)?
        };
        Ok(RegistryPeerSession {
            scope: scope.clone(),
            peer: peer.clone(),
            route,
            pin,
            receiver_key,
            cache: self.cache.clone(),
            _offline_limit: self.offline_limit.clone(),
        })
    }

    /// Returns the active local identity without exposing mutable cache state.
    pub fn local_device(&self) -> Result<LocalDevice> {
        let cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        let local = active_device(&cache, &cache.scope.local_device)?;
        LocalDevice::new(local.id.as_str().to_owned(), local.key_version)
    }

    /// Resolves a currently pinned target for rendezvous metadata.
    pub fn pinned_target(
        &self,
        scope: &RegistryScope,
        peer: &DeviceId,
        route: RegistryRoute,
        now: i64,
    ) -> Result<DeviceTarget> {
        let pin = self.pinned_peer(scope, peer, route, now)?;
        Ok(DeviceTarget {
            id: peer.clone(),
            key_version: pin.key_version(),
        })
    }

    fn insert_pin(&mut self, pin: Pin) -> Result<()> {
        let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(old) = cache.pins.iter().find(|old| old.device == pin.device) {
            if old.key_version == pin.key_version && old.public_key == pin.public_key {
                return Ok(());
            }
            drop(cache);
            return self.deny(SyncFailure::PinMismatch);
        }
        decode_key(&pin.public_key)?;
        cache.pins.push(pin);
        self.persist_cache(&cache)
    }
    fn persist_cache(&self, cache: &Cache) -> Result<()> {
        self.store.save_named(RECORD, cache)
    }
    fn deny<T>(&mut self, failure: SyncFailure) -> Result<T> {
        self.fail(failure)?;
        bail!("registry authorization denied: {failure:?}")
    }
}

impl RegistryPeerSession {
    /// Authenticate a fresh hello/challenge exchange against the actual DTLS
    /// transport after a fresh registry gate.
    pub async fn authenticate(
        self,
        peer: NativePeer,
        channels: DeviceChannels,
        signer: std::sync::Arc<dyn DeviceSigner>,
        attempt_id: &str,
    ) -> Result<AuthenticatedPeerSession> {
        self.check(now())?;
        let local = active_device(
            &self.cache.lock().unwrap_or_else(|error| error.into_inner()),
            &self.scope.local_device,
        )?
        .clone();
        let local_device = LocalDevice::new(local.id.as_str().to_owned(), local.key_version)?;
        if decode_key(&local.public_key)? != signer.verifying_key() {
            bail!("device signing key does not match local registry key");
        }
        let scope = DeviceAuthenticationScope::new(
            self.scope.origin.as_str().to_owned(),
            self.scope.deployment.0.clone(),
            self.scope.account.0.to_string(),
            self.scope.generation.0,
            local_device.clone(),
            vec!["metadata-v1".into(), "tree-v1".into()],
        )?;
        let peer = Arc::new(peer);
        let authority = authenticate_device_connection(
            peer.as_ref(),
            &channels,
            &scope,
            signer.as_ref(),
            &self.pin,
            attempt_id,
        )
        .await?;
        self.check(now())?;
        Ok(AuthenticatedPeerSession {
            peer,
            channels,
            authority,
            local_device,
            receiver_key: self.receiver_key,
            session: Arc::new(self),
        })
    }

    pub(crate) fn check(&self, now: i64) -> Result<()> {
        let cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if cache.scope != self.scope || gate_cache(&cache, self.route, now) != RegistryGate::Allowed
        {
            bail!("registry authorization is not current");
        }
        let pin = active_pin(&cache, &self.peer)?;
        let active = active_device(&cache, &self.peer)?;
        if pin.key_version != active.key_version || pin.public_key != active.public_key {
            bail!("pinned peer no longer matches registry");
        }
        Ok(())
    }
}

impl AuthenticatedPeerSession {
    fn recheck(&self) -> Result<()> {
        self.session.check(now())
    }
    fn receiver_store_id(&self, receiver_job_id: &str) -> String {
        let scope = &self.session.scope;
        let mut hash = Sha256::new();
        for value in [
            scope.origin.as_str().as_bytes(),
            scope.deployment.0.as_bytes(),
            &scope.account.0.to_be_bytes(),
            &scope.generation.0.to_be_bytes(),
            scope.local_device.as_str().as_bytes(),
            receiver_job_id.as_bytes(),
        ] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value);
        }
        let id = hash
            .finalize()
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("peer-recv-{id}")
    }

    /// Send through this registry-issued session. The raw proof authority never
    /// leaves this module, and the registry remains live through publication.
    pub async fn send_prepared_peer_job(
        &self,
        store: &Store,
        job: &PeerPreparedJob,
        requested_grant_id: String,
        requested_subdirectory: Option<String>,
        expires_at_unix_seconds: u64,
        control: &Control,
    ) -> Result<DeviceReceipt> {
        self.recheck()?;
        let monitor = SessionMonitor::start(self, control);
        let result = send_prepared_peer_job(
            store,
            job,
            &self.channels,
            &self.authority,
            requested_grant_id,
            requested_subdirectory,
            expires_at_unix_seconds,
            &self.receiver_key,
        )
        .await;
        self.peer.close().await;
        monitor.stop();
        self.recheck()?;
        result
    }

    /// Receive the offer before choosing its private checkpoint store.
    pub async fn receive_peer_offer(&self, now_unix_seconds: u64) -> Result<DeviceOffer> {
        self.recheck()?;
        receive_authenticated_peer_offer(
            &self.channels,
            &self.authority,
            &self.local_device,
            now_unix_seconds,
        )
        .await
    }

    /// Continue a received offer using a registry-gated receiver. Admission,
    /// commit, receipt persistence, and periodic monitoring all recheck the
    /// durable registry state.
    pub async fn receive_admitted_peer_offer(
        &self,
        store: &Store,
        offer: &DeviceOffer,
        receiver: &dyn PeerReceiver,
        receiver_signer: &dyn DeviceSigner,
        control: &Control,
    ) -> Result<DeviceReceipt> {
        self.recheck()?;
        let monitor = SessionMonitor::start(self, control);
        let receiver = GatedReceiver {
            gate: self.session.clone(),
            receiver,
        };
        let result = receive_admitted_offer(
            store,
            &self.channels,
            &self.authority,
            offer,
            &receiver,
            receiver_signer,
        )
        .await;
        self.peer.close().await;
        monitor.stop();
        result
    }

    pub fn receiver_store_id_for_offer(&self, offer: &DeviceOffer) -> String {
        self.receiver_store_id(&crate::protocol::peer::receiver_job_id(offer))
    }
}

struct GatedReceiver<'a> {
    gate: Arc<RegistryPeerSession>,
    receiver: &'a dyn PeerReceiver,
}

impl PeerReceiver for GatedReceiver<'_> {
    fn admit(
        &self,
        offer: &DeviceOffer,
        manifest: &filebeam_transfer::manifest::Manifest,
    ) -> Result<PeerReceiveGrant> {
        self.gate.check(now())?;
        let grant = self.receiver.admit(offer, manifest)?;
        self.gate.check(now())?;
        Ok(grant)
    }

    fn recover_receipt(&self, offer: &DeviceOffer) -> Result<Option<DeviceReceipt>> {
        self.gate.check(now())?;
        self.receiver.recover_receipt(offer)
    }

    fn commit(
        &self,
        grant: &PeerReceiveGrant,
        job_id: &str,
    ) -> Result<Vec<crate::device::DeviceReceiptRootOutcome>> {
        self.gate.check(now())?;
        self.receiver.commit(grant, job_id)
    }

    fn persist_receipt(&self, grant: &PeerReceiveGrant, receipt: &DeviceReceipt) -> Result<()> {
        self.gate.check(now())?;
        self.receiver.persist_receipt(grant, receipt)
    }
}

struct SessionMonitor(Arc<AtomicBool>);

impl SessionMonitor {
    fn start(session: &AuthenticatedPeerSession, control: &Control) -> Self {
        let stopped = Arc::new(AtomicBool::new(false));
        let done = stopped.clone();
        let peer = session.peer.clone();
        let gate = session.session.clone();
        let control = control.clone();
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(30));
                if done.load(Ordering::Relaxed) {
                    break;
                }
                if gate.check(now()).is_err() {
                    control.cancel();
                    crate::runtime::shared_tokio_runtime().block_on(peer.close());
                    break;
                }
            }
        });
        Self(stopped)
    }

    fn stop(self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl Drop for SessionMonitor {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .ok()
        .and_then(|time| i64::try_from(time).ok())
        .unwrap_or(i64::MAX)
}

fn gate_cache(cache: &Cache, route: RegistryRoute, now: i64) -> RegistryGate {
    if now < cache.last_success {
        return RegistryGate::Denied(SyncFailure::Malformed);
    }
    if let Some(denied) = cache.denied {
        return RegistryGate::Denied(denied.into());
    }
    let age = now.saturating_sub(cache.last_success);
    if cache.last_success == 0 {
        return RegistryGate::RefreshRequired;
    }
    match route {
        RegistryRoute::OfflineLan if age <= cache.offline_freshness => RegistryGate::Allowed,
        RegistryRoute::OfflineLan => RegistryGate::Denied(SyncFailure::Network),
        RegistryRoute::Instance if age <= REFRESH => RegistryGate::Allowed,
        RegistryRoute::Instance if age <= FAILURE => RegistryGate::RefreshRequired,
        RegistryRoute::Instance => RegistryGate::Denied(SyncFailure::Network),
    }
}

fn active_device<'a>(cache: &'a Cache, id: &DeviceId) -> Result<&'a RegistryDevice> {
    cache
        .devices
        .iter()
        .find(|device| &device.id == id && device.status == DeviceStatus::Active)
        .ok_or_else(|| anyhow::anyhow!("device is not active in the current registry"))
}

fn active_pin<'a>(cache: &'a Cache, id: &DeviceId) -> Result<&'a Pin> {
    cache
        .pins
        .iter()
        .find(|pin| &pin.device == id)
        .ok_or_else(|| anyhow::anyhow!("device has not been locally verified"))
}

#[derive(Clone, Debug)]
pub struct PeerIntroduction {
    pub scope: RegistryScope,
    pub introducer: DeviceId,
    pub candidate: DeviceId,
    pub key_version: u32,
    pub public_key: String,
    pub signature: [u8; 64],
}

impl PeerIntroduction {
    pub fn bytes(&self) -> Result<Vec<u8>> {
        if self.key_version == 0 {
            bail!("invalid introduced key version");
        }
        let key = STANDARD_NO_PAD
            .decode(&self.public_key)
            .map_err(|_| anyhow::anyhow!("introduced key is not base64"))?;
        if key.len() != 32 {
            bail!("introduced key has invalid length");
        }
        Ok([
            b"filebeam-peer-introduction-v1\0".as_slice(),
            self.scope.origin.as_str().as_bytes(),
            b"\0",
            self.scope.deployment.0.as_bytes(),
            b"\0",
            self.scope.account.0.to_string().as_bytes(),
            b"\0",
            self.scope.generation.0.to_string().as_bytes(),
            b"\0",
            self.introducer.as_str().as_bytes(),
            b"\0",
            self.candidate.as_str().as_bytes(),
            b"\0",
            self.key_version.to_string().as_bytes(),
            b"\0",
            key.as_slice(),
        ]
        .concat())
    }
}

fn decode_key(encoded: &str) -> Result<VerifyingKey> {
    let bytes = STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|_| anyhow::anyhow!("device key is not base64"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("device key has invalid length"))?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| anyhow::anyhow!("device key is invalid"))
}

fn has_duplicate_ids(devices: &[RegistryDevice]) -> bool {
    devices
        .iter()
        .enumerate()
        .any(|(index, device)| devices[..index].iter().any(|prior| prior.id == device.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    const LOCAL: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const PEER: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";

    fn scope() -> RegistryScope {
        RegistryScope {
            origin: Origin::parse("https://example.test").unwrap(),
            deployment: DeploymentId::parse("example-test").unwrap(),
            account: AccountId(7),
            generation: DeviceGeneration(3),
            local_device: DeviceId::parse(LOCAL).unwrap(),
        }
    }
    fn key(seed: u8) -> String {
        STANDARD_NO_PAD.encode(
            SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes(),
        )
    }
    fn response(revision: u64, peer_key: String, status: DeviceStatus) -> RegistryResponse {
        RegistryResponse {
            revision,
            generation: DeviceGeneration(3),
            offline_freshness_seconds: MAX_OFFLINE as u64,
            devices: vec![
                RegistryDevice {
                    id: DeviceId::parse(LOCAL).unwrap(),
                    key_version: 1,
                    public_key: key(1),
                    status: DeviceStatus::Active,
                    generation: DeviceGeneration(3),
                },
                RegistryDevice {
                    id: DeviceId::parse(PEER).unwrap(),
                    key_version: 1,
                    public_key: peer_key,
                    status,
                    generation: DeviceGeneration(3),
                },
            ],
        }
    }
    fn store() -> (std::path::PathBuf, Store) {
        let root = std::env::temp_dir().join(format!(
            "filebeam-registry-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::create(&root, "registry").unwrap();
        (root, store)
    }

    #[test]
    fn rollback_stale_cache_is_denied() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        registry
            .approve_pairing(&DeviceId::parse(PEER).unwrap(), 100)
            .unwrap();
        assert_eq!(
            registry.gate(RegistryRoute::OfflineLan, 99),
            RegistryGate::Denied(SyncFailure::Malformed)
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reordered_registry_revision_is_a_terminal_denial() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        assert!(
            registry
                .ingest(response(3, key(2), DeviceStatus::Active), 101)
                .is_err()
        );
        assert_eq!(
            registry.gate(RegistryRoute::OfflineLan, 102),
            RegistryGate::Denied(SyncFailure::Malformed)
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn connected_sessions_refresh_at_thirty_and_stop_at_sixty_seconds() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        assert_eq!(
            registry.gate(RegistryRoute::Instance, 131),
            RegistryGate::RefreshRequired
        );
        assert_eq!(
            registry.gate(RegistryRoute::Instance, 161),
            RegistryGate::Denied(SyncFailure::Network)
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_cache_and_revoke_deny_offline() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        registry
            .approve_pairing(&DeviceId::parse(PEER).unwrap(), 100)
            .unwrap();
        assert_eq!(
            registry.gate(RegistryRoute::OfflineLan, 100 + MAX_OFFLINE + 1),
            RegistryGate::Denied(SyncFailure::Network)
        );
        registry
            .ingest(response(5, key(2), DeviceStatus::Revoked), 101)
            .unwrap_err();
        assert_eq!(
            registry.gate(RegistryRoute::OfflineLan, 102),
            RegistryGate::Denied(SyncFailure::Forbidden)
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registered_session_rechecks_a_revocation_before_commit() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        registry
            .approve_pairing(&DeviceId::parse(PEER).unwrap(), 100)
            .unwrap();
        let session = registry
            .peer_session(
                &scope(),
                &DeviceId::parse(PEER).unwrap(),
                RegistryRoute::OfflineLan,
                100,
            )
            .unwrap();
        registry
            .ingest(response(5, key(2), DeviceStatus::Revoked), 101)
            .unwrap_err();
        assert!(session.check(102).is_err());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reopened_registry_updates_a_live_session_and_ignores_disk_rollback() {
        let (root, store) = store();
        let mut first =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        first
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        first
            .approve_pairing(&DeviceId::parse(PEER).unwrap(), 100)
            .unwrap();
        let session = first
            .peer_session(
                &scope(),
                &DeviceId::parse(PEER).unwrap(),
                RegistryRoute::OfflineLan,
                100,
            )
            .unwrap();
        drop(first);

        let mut refreshed =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        refreshed
            .ingest(response(5, key(2), DeviceStatus::Revoked), 101)
            .unwrap_err();
        assert!(session.check(102).is_err());

        // A stale on-disk record must not replace the terminal shared state
        // while the live session still owns it.
        store
            .save_named(
                RECORD,
                &Cache {
                    scope: scope(),
                    revision: 4,
                    last_success: 100,
                    offline_freshness: MAX_OFFLINE,
                    devices: response(4, key(2), DeviceStatus::Active).devices,
                    pins: vec![Pin {
                        device: DeviceId::parse(PEER).unwrap(),
                        key_version: 1,
                        public_key: key(2),
                    }],
                    denied: None,
                },
            )
            .unwrap();
        drop(refreshed);
        let reopened =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        assert_eq!(
            reopened.gate(RegistryRoute::OfflineLan, 102),
            RegistryGate::Denied(SyncFailure::Forbidden)
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn server_key_substitution_never_replaces_a_pin() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        registry
            .approve_pairing(&DeviceId::parse(PEER).unwrap(), 100)
            .unwrap();
        assert!(
            registry
                .ingest(response(5, key(9), DeviceStatus::Active), 101)
                .is_err()
        );
        assert_eq!(
            registry.gate(RegistryRoute::OfflineLan, 102),
            RegistryGate::Denied(SyncFailure::PinMismatch)
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signed_introduction_requires_a_current_verified_introducer() {
        let (root, store) = store();
        let mut registry =
            DeviceRegistry::open(&store, scope(), Duration::from_secs(MAX_OFFLINE as u64)).unwrap();
        registry
            .ingest(response(4, key(2), DeviceStatus::Active), 100)
            .unwrap();
        registry
            .approve_pairing(&DeviceId::parse(PEER).unwrap(), 100)
            .unwrap();
        let signer = SigningKey::from_bytes(&[2; 32]);
        let mut introduction = PeerIntroduction {
            scope: scope(),
            introducer: DeviceId::parse(PEER).unwrap(),
            candidate: DeviceId::parse(LOCAL).unwrap(),
            key_version: 1,
            public_key: key(1),
            signature: [0; 64],
        };
        introduction.signature = signer.sign(&introduction.bytes().unwrap()).to_bytes();
        registry.import_introduction(introduction, 100).unwrap();
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
