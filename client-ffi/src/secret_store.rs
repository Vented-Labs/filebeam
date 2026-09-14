//! Host-backed custody for native checkpoint catalog keys.

use crate::{ClientError, operation};
use filebeam_client_core::control::TransferSettings;
use filebeam_transfer_native::checkpoint::SecretStore;
use std::sync::Arc;

#[uniffi::export(foreign)]
pub trait SecretStoreCallback: Send + Sync {
    fn load_or_create(&self, scope: String) -> std::result::Result<Vec<u8>, ClientError>;
    fn remove(&self, scope: String) -> std::result::Result<(), ClientError>;
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
