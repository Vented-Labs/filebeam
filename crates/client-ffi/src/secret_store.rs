//! Host-backed custody for native checkpoint catalog keys.

use crate::{ClientError, Result, invalid, operation};
use filebeam_client_core::control::TransferSettings;
use filebeam_transfer_native::checkpoint::{SecretStore, Store};
use std::{path::PathBuf, sync::Arc};

#[uniffi::export(foreign)]
pub trait SecretStoreCallback: Send + Sync {
    fn load_or_create(&self, scope: String) -> std::result::Result<Vec<u8>, ClientError>;
    fn remove(&self, scope: String) -> std::result::Result<(), ClientError>;
}

/// Typed device-key operations. The host key remains in platform custody and
/// this interface cannot sign caller-selected bytes.
#[uniffi::export(foreign)]
pub trait DeviceKeySignerCallback: Send + Sync {
    fn sign_enrollment(
        &self,
        device_id: String,
        generation: u64,
        challenge: String,
    ) -> std::result::Result<Vec<u8>, ClientError>;
    fn sign_approval(
        &self,
        approval: crate::services::DeviceApprovalPayload,
    ) -> std::result::Result<Vec<u8>, ClientError>;
}

/// Platform-owned storage for revocable device credentials. Credential values
/// are never returned by registry records or kept in Rust persistence.
#[uniffi::export(foreign)]
pub trait DeviceCredentialStoreCallback: Send + Sync {
    fn store(
        &self,
        instance: String,
        device_id: String,
        credential: String,
    ) -> std::result::Result<(), ClientError>;
    fn load(&self, instance: String, device_id: String)
    -> std::result::Result<String, ClientError>;
    fn remove(&self, instance: String, device_id: String) -> std::result::Result<(), ClientError>;
}

struct HostSecretStore(Arc<dyn SecretStoreCallback>);

impl SecretStore for HostSecretStore {
    fn load_or_create(&self, scope: &str) -> anyhow::Result<[u8; 32]> {
        let key = self.0.load_or_create(scope.into()).map_err(operation)?;
        key.try_into()
            .map_err(|_| anyhow::anyhow!("host checkpoint key has invalid length"))
    }

    fn remove(&self, scope: &str) -> anyhow::Result<()> {
        self.0.remove(scope.into()).map_err(operation)?;
        Ok(())
    }
}

pub fn install(settings: &mut TransferSettings, callback: Arc<dyn SecretStoreCallback>) {
    settings.checkpoint_secret_store = Some(Arc::new(HostSecretStore(callback)));
}

/// Exercises host-backed catalog and immutable publication without network I/O.
#[uniffi::export]
pub fn checkpoint_self_test(
    state_directory: String,
    secret_store: Arc<dyn SecretStoreCallback>,
) -> Result<bool> {
    let root = PathBuf::from(state_directory);
    if !root.is_absolute() {
        return Err(invalid("The transfer state directory must be absolute"));
    }
    let id = "checkpoint-self-test";
    let store = Store::create_with_secret_store(&root, id, Arc::new(HostSecretStore(secret_store)))
        .map_err(|error| ClientError::Operation {
            detail: format!("{error:#}"),
        })?;
    let result = (|| {
        store.save_named("probe.json", &"catalog")?;
        let artifact = store.persist_immutable("chunk-0-0", b"immutable")?;
        Ok::<_, anyhow::Error>(artifact.is_file())
    })();
    drop(store);
    let _ = Store::discard(&root, id);
    result.map_err(|error| ClientError::Operation {
        detail: format!("{error:#}"),
    })
}
