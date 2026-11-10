use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{DaemonDevice, DaemonState, DeviceApproval, DeviceProvision, RuntimeAvailability};

/// Platform-neutral account registry control. IPC transports and filesystem
/// receivers deliberately call this one owner instead of reimplementing trust.
pub(crate) struct ControlPlane {
    home: PathBuf,
    certificate: Option<reqwest::Certificate>,
}

impl ControlPlane {
    pub(crate) fn new(home: &Path, certificate: Option<reqwest::Certificate>) -> Self {
        Self {
            home: home.into(),
            certificate,
        }
    }

    pub(crate) fn provision(
        &self,
        state: &mut DaemonState,
        provision: DeviceProvision,
    ) -> Result<()> {
        validate_provision(&provision)?;
        let registry = service_client(&provision.instance, self.certificate.clone())?
            .devices()
            .device_sync(&provision.credential)?;
        if registry.generation != provision.generation {
            bail!("device provisioning generation is no longer active");
        }
        let device = registry
            .devices
            .iter()
            .find(|device| device.id == provision.device_id)
            .context("provisioned device is not in the backend registry")?;
        if device.status != "active"
            || device.key_version != provision.key_version
            || device.generation != provision.generation
            || !same_public_key(&device.public_key, &provision.public_key)?
        {
            bail!("provisioned device is not active with its expected key");
        }
        refresh_registry(&self.home, &provision, registry)?;
        daemon_store(&self.home)?.save_named("device-provision-v1.json", &provision)?;
        state.runtime = RuntimeAvailability::Provisioned;
        write_state(&self.home.join("runtime/daemon/state.json"), state)
    }

    pub(crate) fn list_devices(&self) -> Result<Vec<DaemonDevice>> {
        let provision = daemon_store(&self.home)?
            .load_named::<DeviceProvision>("device-provision-v1.json")?
            .context(crate::PROVISIONING_REQUIRED)?;
        service_client(&provision.instance, self.certificate.clone())?
            .devices()
            .device_sync(&provision.credential)?
            .devices
            .into_iter()
            .map(|device| {
                Ok(DaemonDevice {
                    id: device.id,
                    display_name: device.display_name,
                    key_version: device.key_version,
                    status: device.status,
                    fingerprint: device_fingerprint(&device.public_key)?,
                })
            })
            .collect()
    }

    /// Approves a pending device on this account with this daemon's device key.
    pub(crate) fn approve_device(&self, approval: DeviceApproval, fingerprint: &str) -> Result<()> {
        let provision = daemon_store(&self.home)?
            .load_named::<DeviceProvision>("device-provision-v1.json")?
            .context("add this device with `beam devices add` before approving another")?;
        if approval.instance_origin != provision.instance {
            bail!("the approval payload belongs to another Filebeam instance");
        }
        if device_fingerprint(&approval.candidate_public_key)? != fingerprint {
            bail!("the candidate key does not match the fingerprint you verified");
        }
        let service = service_client(&provision.instance, self.certificate.clone())?;
        let devices = service.devices();
        let candidate = devices
            .device_sync(&provision.credential)?
            .devices
            .into_iter()
            .find(|device| device.id == approval.candidate_device_id)
            .context("the candidate device is not registered on this account")?;
        if candidate.status != "pending" || candidate.public_key != approval.candidate_public_key {
            bail!("the candidate device is no longer pending with the verified key");
        }
        let context = filebeam_client_core::services::DeviceApprovalContext {
            instance_origin: approval.instance_origin,
            deployment_id: approval.deployment_id,
            account_id: approval.account_id,
            generation: approval.generation,
            candidate_device_id: approval.candidate_device_id,
            candidate_key_version: approval.candidate_key_version,
            candidate_public_key: approval.candidate_public_key,
            challenge_sha256: approval.challenge_sha256,
            expires_at_unix: approval.expires_at_unix,
            approver_device_id: provision.device_id.clone(),
            approver_key_version: provision.key_version,
        };
        let key = SigningKey::from_bytes(
            &STANDARD
                .decode(&provision.signing_key)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("device signing key has an invalid length"))?,
        );
        let signature = filebeam_client_core::services::approval_signature(&key, &context)?;
        devices.approve_device_enrollment(&candidate, &provision.credential, &signature)?;
        // The user verified this key to approve it, which is the same check pairing makes.
        self.pin_peer(
            &provision,
            &candidate.id,
            candidate.key_version,
            fingerprint,
        )
    }

    pub(crate) fn confirm_peer(
        &self,
        device_id: &str,
        key_version: u32,
        fingerprint: &str,
        nonce: &str,
    ) -> Result<()> {
        if nonce.len() < 16 || !nonce.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            bail!("pairing confirmation was not locally confirmed");
        }
        let provision = daemon_store(&self.home)?
            .load_named::<DeviceProvision>("device-provision-v1.json")?
            .context(crate::PROVISIONING_REQUIRED)?;
        self.pin_peer(&provision, device_id, key_version, fingerprint)
    }

    /// Pins an active device whose fingerprint the user verified out of band.
    fn pin_peer(
        &self,
        provision: &DeviceProvision,
        device_id: &str,
        key_version: u32,
        fingerprint: &str,
    ) -> Result<()> {
        let snapshot = service_client(&provision.instance, self.certificate.clone())?
            .devices()
            .device_sync(&provision.credential)?;
        let candidate = snapshot
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .context("device is not in the authenticated account registry")?;
        if candidate.status != "active"
            || candidate.key_version != key_version
            || device_fingerprint(&candidate.public_key)? != fingerprint
        {
            bail!("pairing verification does not match the authenticated device registry");
        }
        refresh_registry(&self.home, provision, snapshot)?;
        let store = registry_store(&self.home)?;
        let scope = registry_scope(provision)?;
        let mut registry = filebeam_client_core::device_registry::DeviceRegistry::open(
            &store,
            scope,
            Duration::from_secs(7 * 24 * 60 * 60),
        )?;
        registry.approve_pairing(
            &filebeam_client_core::device_registry::DeviceId::parse(device_id.to_owned())?,
            now(),
        )
    }

    pub(crate) fn refresh_registry(&self, provision: &DeviceProvision) -> Result<()> {
        match service_client(&provision.instance, self.certificate.clone())?
            .devices()
            .device_sync(&provision.credential)
        {
            Ok(snapshot) => refresh_registry(&self.home, provision, snapshot),
            Err(error) => {
                if let Some(failure) = registry_sync_failure(&error.to_string()) {
                    let store = registry_store(&self.home)?;
                    let mut registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                        &store,
                        registry_scope(provision)?,
                        Duration::from_secs(7 * 24 * 60 * 60),
                    )?;
                    registry.fail(failure)?;
                }
                Ok(())
            }
        }
    }
}
fn registry_sync_failure(
    message: &str,
) -> Option<filebeam_client_core::device_registry::SyncFailure> {
    use filebeam_client_core::device_registry::SyncFailure;
    if message.contains("returned 401") {
        Some(SyncFailure::Unauthorized)
    } else if message.contains("returned 403") {
        Some(SyncFailure::Forbidden)
    } else if message.contains("native device request returned 4") {
        Some(SyncFailure::Disabled)
    } else if message.contains("decode native device response")
        || message.contains("response exceeds the control-plane limit")
    {
        Some(SyncFailure::Malformed)
    } else {
        None
    }
}

pub(crate) fn daemon_store(home: &Path) -> Result<filebeam_transfer_native::checkpoint::Store> {
    open_store(&home.join("transfers"), "daemon-private-v1")
}
pub(crate) fn registry_store(home: &Path) -> Result<filebeam_transfer_native::checkpoint::Store> {
    open_store(&home.join("registry"), "device-registry-v1")
}
/// Opens a private store, creating it on first use. Daemon threads share these
/// stores through an exclusive lock, so a short wait covers another thread's
/// read or write instead of failing that thread's whole pass.
fn open_store(root: &Path, id: &str) -> Result<filebeam_transfer_native::checkpoint::Store> {
    use filebeam_transfer_native::checkpoint::Store;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        match Store::open(root, id) {
            Ok(store) => return Ok(store),
            Err(error) if error.to_string().contains("does not exist") => {
                return Store::create(root, id);
            }
            Err(error) if lock_contended(&error) && std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
}
fn lock_contended(error: &anyhow::Error) -> bool {
    let contended = fs2::lock_contended_error().raw_os_error();
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|cause| cause.raw_os_error() == contended)
}
pub(crate) fn write_state(path: &Path, state: &DaemonState) -> Result<()> {
    write_json(path, state)
}
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("state path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, &serde_json::to_vec(value)?)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| anyhow::Error::from(error.error))?;
    sync_parent(parent)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<()> {
    std::fs::File::open(parent)?.sync_all().map_err(Into::into)
}
#[cfg(windows)]
fn sync_parent(parent: &Path) -> Result<()> {
    std::fs::File::open(parent)?.sync_all().map_err(Into::into)
}
#[cfg(not(any(unix, windows)))]
fn sync_parent(_: &Path) -> Result<()> {
    Ok(())
}
#[cfg(windows)]
pub(crate) fn read_state(path: &Path) -> Result<DaemonState> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("read daemon state"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DaemonState::default()),
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn registry_scope(
    provision: &DeviceProvision,
) -> Result<filebeam_client_core::device_registry::RegistryScope> {
    use filebeam_client_core::device_registry::{
        AccountId, DeploymentId, DeviceGeneration, DeviceId, Origin, RegistryScope,
    };
    Ok(RegistryScope {
        origin: Origin::parse(provision.instance.clone())?,
        deployment: DeploymentId::parse(provision.deployment.clone())?,
        account: AccountId(provision.account_id),
        generation: DeviceGeneration(provision.generation),
        local_device: DeviceId::parse(provision.device_id.clone())?,
    })
}
fn validate_provision(provision: &DeviceProvision) -> Result<()> {
    filebeam_client_core::device_registry::Origin::parse(provision.instance.clone())?;
    filebeam_client_core::device_registry::DeploymentId::parse(provision.deployment.clone())?;
    filebeam_client_core::device_registry::DeviceId::parse(provision.device_id.clone())?;
    if provision.generation == 0
        || provision.key_version == 0
        || provision.credential.len() != 64
        || !provision
            .credential
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        bail!("device provisioning identity is invalid");
    }
    let public: [u8; 32] = STANDARD
        .decode(&provision.public_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("device public key has an invalid length"))?;
    let signing: [u8; 32] = STANDARD
        .decode(&provision.signing_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("device signing key has an invalid length"))?;
    if SigningKey::from_bytes(&signing).verifying_key() != VerifyingKey::from_bytes(&public)? {
        bail!("device signing key does not match its public key");
    }
    Ok(())
}
fn service_client(
    instance: &str,
    certificate: Option<reqwest::Certificate>,
) -> Result<filebeam_client_core::services::ServiceClient> {
    match certificate {
        Some(certificate) => {
            filebeam_client_core::services::ServiceClient::new_with_trusted_certificate(
                instance,
                certificate,
            )
        }
        None => filebeam_client_core::services::ServiceClient::new(instance),
    }
}
fn same_public_key(left: &str, right: &str) -> Result<bool> {
    Ok(STANDARD.decode(left)? == STANDARD.decode(right)?)
}
fn device_fingerprint(public_key: &str) -> Result<String> {
    Ok(hex::encode(Sha256::digest(STANDARD.decode(public_key)?)))
}
fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .unwrap_or(i64::MAX)
}
fn refresh_registry(
    home: &Path,
    provision: &DeviceProvision,
    snapshot: filebeam_client_core::services::DeviceRegistrySnapshot,
) -> Result<()> {
    use filebeam_client_core::device_registry::{
        DeviceGeneration, DeviceRegistry, DeviceStatus, RegistryDevice, RegistryResponse,
    };
    let filebeam_client_core::services::DeviceRegistrySnapshot {
        revision,
        generation,
        offline_freshness_seconds,
        devices,
    } = snapshot;
    let devices = devices
        .into_iter()
        .map(|device| {
            Ok(RegistryDevice {
                id: filebeam_client_core::device_registry::DeviceId::parse(device.id)?,
                key_version: device.key_version,
                public_key: device.public_key.trim_end_matches('=').into(),
                generation: DeviceGeneration(device.generation),
                status: match device.status.as_str() {
                    "pending" => DeviceStatus::Pending,
                    "active" => DeviceStatus::Active,
                    "revoked" => DeviceStatus::Revoked,
                    _ => bail!("backend returned an invalid device status"),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let store = registry_store(home)?;
    let mut registry = DeviceRegistry::open(
        &store,
        registry_scope(provision)?,
        Duration::from_secs(7 * 24 * 60 * 60),
    )?;
    registry.ingest(
        RegistryResponse {
            revision,
            generation: DeviceGeneration(generation),
            offline_freshness_seconds: offline_freshness_seconds
                .context("device registry response omitted offline freshness")?,
            devices,
        },
        now(),
    )
}
