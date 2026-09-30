use std::path::PathBuf;

use anyhow::Result;
pub use filebeam_client_config::{
    DEFAULT_MEMORY_LIMIT_MIB, MAX_CONCURRENCY, MAX_MEMORY_LIMIT_MIB, MIN_MEMORY_LIMIT_MIB,
};

/// CLI compatibility view over the shared desktop/client configuration.
#[derive(Clone, Debug)]
pub struct Config {
    pub no_color: bool,
    pub reduced_motion: bool,
    pub check_updates: bool,
    pub max_concurrency: Option<u32>,
    pub memory_limit_mib: u64,
    pub webrtc_relay_only: bool,
    pub home: PathBuf,
    pub server_url: String,
    shared: filebeam_client_config::Config,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shared: filebeam_client_config::Config::default(),
            no_color: false,
            reduced_motion: false,
            check_updates: true,
            max_concurrency: None,
            memory_limit_mib: DEFAULT_MEMORY_LIMIT_MIB,
            webrtc_relay_only: false,
            home: PathBuf::new(),
            server_url: "https://filebeam.io".into(),
        }
    }
}

impl Config {
    pub fn load(home: Option<PathBuf>) -> Result<Self> {
        let shared = filebeam_client_config::Config::load(home)?;
        Ok(Self::from_shared(shared))
    }
    pub fn validate_transfer_limits(&self) -> Result<()> {
        let mut shared = self.shared.clone();
        shared.transfers.max_concurrency = self.max_concurrency;
        shared.transfers.memory_limit_mib = self.memory_limit_mib;
        shared.validate()
    }
    pub fn native_ice_override(&self) -> filebeam_transfer_native::ice::IceOverride {
        self.shared.native_ice_override()
    }

    fn from_shared(shared: filebeam_client_config::Config) -> Self {
        Self {
            no_color: shared.cli.no_color,
            reduced_motion: shared.appearance.reduced_motion,
            check_updates: shared.updates.auto_update,
            max_concurrency: shared.transfers.max_concurrency,
            memory_limit_mib: shared.transfers.memory_limit_mib,
            webrtc_relay_only: shared.webrtc.relay_only,
            home: shared.home.clone(),
            server_url: shared.server.url.clone(),
            shared,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_home_is_retained() {
        let path = PathBuf::from("/tmp/beam-home");
        assert_eq!(Config::load(Some(path.clone())).unwrap().home, path);
    }
    #[test]
    fn updates_default_to_enabled() {
        assert!(Config::default().check_updates);
    }
    #[test]
    fn transfer_limits_have_a_safe_default_and_reject_invalid_values() {
        assert_eq!(Config::default().memory_limit_mib, 512);
        let mut config = Config {
            max_concurrency: Some(0),
            ..Config::default()
        };
        assert!(config.validate_transfer_limits().is_err());
        config.max_concurrency = Some(2);
        config.memory_limit_mib = 32;
        assert!(config.validate_transfer_limits().is_err());
    }
}
