use crate::{ClientConfig, Result, SecretStoreCallback, SourceCallback, invalid, operation};
use filebeam_client_core::{self as core, control::TransferSettings};
use std::sync::Arc;

/// Process-owned scheduler shared by transfer and service FFI objects.
#[derive(uniffi::Object)]
pub struct NativeRuntime {
    /// Core owns the reusable process runtime; these retained fields preserve
    /// existing FFI constructors and internal call sites.
    pub(crate) _application: core::ClientRuntime,
    pub(crate) scheduler: core::Scheduler,
    pub(crate) settings: TransferSettings,
    pub(crate) device_runtime: core::DeviceRuntime,
}

#[uniffi::export]
impl NativeRuntime {
    #[uniffi::constructor]
    pub fn new(config: ClientConfig) -> Result<Self> {
        Self::build(config, None)
    }
    #[uniffi::constructor]
    pub fn new_with_secret_store(
        config: ClientConfig,
        secret_store: Arc<dyn SecretStoreCallback>,
    ) -> Result<Self> {
        Self::build(config, Some(secret_store))
    }
    #[uniffi::constructor]
    pub fn new_with_secret_store_and_source(
        config: ClientConfig,
        secret_store: Arc<dyn SecretStoreCallback>,
        source: Arc<dyn SourceCallback>,
    ) -> Result<Self> {
        Self::build_with_source(config, Some(secret_store), Some(source))
    }
}

impl NativeRuntime {
    fn build(
        config: ClientConfig,
        secret_store: Option<Arc<dyn SecretStoreCallback>>,
    ) -> Result<Self> {
        Self::build_with_source(config, secret_store, None)
    }
    fn build_with_source(
        config: ClientConfig,
        secret_store: Option<Arc<dyn SecretStoreCallback>>,
        source: Option<Arc<dyn SourceCallback>>,
    ) -> Result<Self> {
        if !(64..=512).contains(&config.memory_budget_mib)
            || !(1..=4).contains(&config.max_concurrency)
        {
            return Err(invalid(
                "Use a 64-512 MiB buffer budget and 1-4 concurrent requests",
            ));
        }
        let memory_budget = u64::from(config.memory_budget_mib) * 1024 * 1024;
        // Admissions reserve a buffer budget and worker overhead per active job;
        // KDF/manifest work needs one additional process-wide transient reserve.
        let memory_bytes = memory_budget
            .checked_add(core::RUNTIME_ALLOWANCE_BYTES)
            .and_then(|value| value.checked_mul(u64::from(config.max_concurrency)))
            .and_then(|value| value.checked_add(core::TRANSIENT_MEMORY_ALLOWANCE_BYTES))
            .ok_or_else(|| invalid("The scheduler memory budget is too large"))?;
        let mut settings = TransferSettings {
            state_home: config.state_directory.into(),
            max_concurrency: Some(config.max_concurrency),
            memory_budget,
            client_user_agent: Some(concat!("filebeam-native/", env!("CARGO_PKG_VERSION")).into()),
            webrtc_relay_only: config.relay_only,
            checkpoint_secret_store: None,
            source_resolver: None,
        };
        if let Some(secret_store) = secret_store {
            crate::secret_store::install(&mut settings, secret_store);
        }
        if let Some(source) = source {
            crate::source::install(&mut settings, source);
        }
        let application = core::ClientRuntime::process_global(core::ApplicationSettings::new(
            settings.clone(),
            core::SchedulerLimits {
                workers: config.max_concurrency as usize,
                memory_bytes,
            },
        ))
        .map_err(operation)?;
        let device_runtime = core::DeviceRuntime::new(core::ApplicationSettings::new(
            settings.clone(),
            core::SchedulerLimits {
                workers: config.max_concurrency as usize,
                memory_bytes,
            },
        ))
        .map_err(operation)?;
        Ok(Self {
            scheduler: application.scheduler(),
            settings,
            _application: application,
            device_runtime,
        })
    }
    pub(crate) fn reserve_service_memory(
        &self,
        bytes: u64,
    ) -> Result<filebeam_client_core::control::MemoryPermit> {
        self.scheduler
            .try_reserve_service_memory(bytes)
            .map_err(operation)
    }

    pub(crate) fn registry_store(
        &self,
        id: &str,
    ) -> anyhow::Result<filebeam_transfer_native::checkpoint::Store> {
        let root = &self.settings.state_home;
        let secrets = self.settings.checkpoint_secret_store.clone();
        if root.join(id).exists() {
            match secrets {
                Some(secrets) => {
                    filebeam_transfer_native::checkpoint::Store::open_with_secret_store(
                        root, id, secrets,
                    )
                }
                None => filebeam_transfer_native::checkpoint::Store::open(root, id),
            }
        } else {
            match secrets {
                Some(secrets) => {
                    filebeam_transfer_native::checkpoint::Store::create_with_secret_store(
                        root, id, secrets,
                    )
                }
                None => filebeam_transfer_native::checkpoint::Store::create(root, id),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config(state_directory: &str) -> ClientConfig {
        ClientConfig {
            state_directory: state_directory.into(),
            memory_budget_mib: 64,
            max_concurrency: 1,
            relay_only: false,
            allow_http: false,
        }
    }

    #[test]
    fn runtimes_keep_their_own_profile_settings_while_sharing_the_scheduler() {
        let first = NativeRuntime::new(config("first-profile")).unwrap();
        let second = NativeRuntime::new(config("second-profile")).unwrap();
        assert_eq!(first.settings.state_home, PathBuf::from("first-profile"));
        assert_eq!(second.settings.state_home, PathBuf::from("second-profile"));
        assert_eq!(
            second._application.transfer_settings().state_home,
            PathBuf::from("second-profile")
        );
    }
}
