use crate::{ClientConfig, Result, invalid, operation};
use filebeam_client_core::{self as core, control::TransferSettings};

/// Process-owned scheduler shared by transfer and service FFI objects.
#[derive(uniffi::Object)]
pub struct NativeRuntime {
    pub(crate) scheduler: core::Scheduler,
    pub(crate) settings: TransferSettings,
}

#[uniffi::export]
impl NativeRuntime {
    #[uniffi::constructor]
    pub fn new(config: ClientConfig) -> Result<Self> {
        if !(64..=512).contains(&config.memory_budget_mib)
            || !(1..=4).contains(&config.max_concurrency)
        {
            return Err(invalid(
                "Use a 64-512 MiB buffer budget and 1-4 concurrent requests",
            ));
        }
        let memory_budget = u64::from(config.memory_budget_mib) * 1024 * 1024;
        let memory_bytes = memory_budget
            .checked_add(core::RUNTIME_ALLOWANCE_BYTES)
            .and_then(|value| value.checked_add(core::TRANSIENT_MEMORY_ALLOWANCE_BYTES))
            .and_then(|value| value.checked_mul(u64::from(config.max_concurrency)))
            .ok_or_else(|| invalid("The scheduler memory budget is too large"))?;
        Ok(Self {
            scheduler: core::Scheduler::new(core::SchedulerLimits {
                workers: config.max_concurrency as usize,
                memory_bytes,
            })
            .map_err(operation)?,
            settings: TransferSettings {
                state_home: config.state_directory.into(),
                max_concurrency: Some(config.max_concurrency),
                memory_budget,
                client_user_agent: Some(
                    concat!("filebeam-native/", env!("CARGO_PKG_VERSION")).into(),
                ),
                webrtc_relay_only: config.relay_only,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
        })
    }
}
