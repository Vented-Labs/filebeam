use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_MEMORY_LIMIT_MIB: u64 = 512;
pub const MIN_MEMORY_LIMIT_MIB: u64 = 64;
pub const MAX_MEMORY_LIMIT_MIB: u64 = 4096;
pub const MAX_CONCURRENCY: u32 = 64;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub no_color: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(default)]
    pub max_concurrency: Option<u32>,
    #[serde(default = "default_memory_limit_mib")]
    pub memory_limit_mib: u64,
    #[serde(skip)]
    pub home: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            no_color: false,
            reduced_motion: false,
            check_updates: true,
            max_concurrency: None,
            memory_limit_mib: DEFAULT_MEMORY_LIMIT_MIB,
            home: PathBuf::new(),
        }
    }
}

impl Config {
    pub fn load(home: Option<PathBuf>) -> Result<Self> {
        let home = home.unwrap_or_else(default_home);
        let path = home.join("config.toml");
        let mut config = if path.exists() {
            toml::from_str(
                &fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
            )
            .context("parse config.toml")?
        } else {
            Self::default()
        };
        config.validate_transfer_limits()?;
        config.home = home;
        Ok(config)
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.home.join("cache")
    }
    pub fn ensure_cache_dir(&self) -> Result<PathBuf> {
        let path = self.cache_dir();
        fs::create_dir_all(&path).with_context(|| format!("create {}", path.display()))?;
        Ok(path)
    }

    pub fn validate_transfer_limits(&self) -> Result<()> {
        if self
            .max_concurrency
            .is_some_and(|value| value == 0 || value > MAX_CONCURRENCY)
        {
            anyhow::bail!("max_concurrency must be between 1 and {MAX_CONCURRENCY}");
        }
        if !(MIN_MEMORY_LIMIT_MIB..=MAX_MEMORY_LIMIT_MIB).contains(&self.memory_limit_mib) {
            anyhow::bail!(
                "memory_limit_mib must be between {MIN_MEMORY_LIMIT_MIB} and {MAX_MEMORY_LIMIT_MIB}"
            );
        }
        Ok(())
    }
}

fn default_check_updates() -> bool {
    true
}
fn default_memory_limit_mib() -> u64 {
    DEFAULT_MEMORY_LIMIT_MIB
}
fn default_home() -> PathBuf {
    std::env::var_os("FILEBEAM_HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .map(|path| path.join(".filebeam"))
        .unwrap_or_else(|| Path::new(".").join(".filebeam"))
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
