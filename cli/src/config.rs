use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub no_color: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(skip)]
    pub home: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            no_color: false,
            reduced_motion: false,
            check_updates: true,
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
}

fn default_check_updates() -> bool {
    true
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
}
