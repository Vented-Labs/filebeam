use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail};
use filebeam_client_updater::{Activation, Options, Product, Updater};

use crate::config::Config;

fn updater(home: PathBuf) -> Result<Updater> {
    let key = option_env!("BEAM_RELEASE_PUBLIC_KEY")
        .filter(|value| !value.is_empty())
        .context(
            "updates are unavailable: this build has no embedded trusted release signing key",
        )?;
    Ok(Updater::new(Options::for_product(
        home,
        Product::Cli,
        env!("BEAM_VERSION"),
        key,
    )?))
}

/// Explicit `beam update`: bypasses scheduling but retains the shared lease and all checks.
pub fn check(config: &Config) -> Result<String> {
    updater(config.home.clone())?.run_manual()
}

/// Fast launch path: this only schedules a copied, detached worker and performs no I/O.
pub fn notify_if_available(config: &Config) {
    if !config.check_updates {
        return;
    }
    let _ = updater(config.home.clone()).and_then(|updater| updater.launch(true));
}

/// Handles only hidden worker modes before clap. Normal CLI invocations must never require a key.
pub fn apply_staged_update() -> Result<bool> {
    let mut arguments = env::args_os();
    let _ = arguments.next();
    let command = arguments.next();
    if command.as_deref() == Some(std::ffi::OsStr::new("--filebeam-apply-staged")) {
        let product = arguments
            .next()
            .context("Windows update helper is missing product")?;
        let home = arguments
            .next()
            .map(PathBuf::from)
            .context("Windows update helper is missing home")?;
        if arguments.next().is_some() || product != "cli" {
            bail!("Windows update helper received invalid arguments");
        }
        #[cfg(windows)]
        return updater(home)?.apply_windows_helper();
        #[cfg(not(windows))]
        {
            let _ = home;
            bail!("Windows update helper is unsupported on this platform");
        }
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("--filebeam-update-worker")) {
        let product = arguments
            .next()
            .context("update worker is missing product")?;
        let home = arguments
            .next()
            .map(PathBuf::from)
            .context("update worker is missing home")?;
        if arguments.next().is_some() || product != "cli" {
            bail!("update worker received invalid arguments");
        }
        // Re-read after every network boundary; a settings change wins over a running worker.
        updater(home.clone())?.bootstrap(|| {
            Config::load(Some(home.clone()))
                .map(|config| config.check_updates)
                .unwrap_or(false)
        })?;
        return Ok(true);
    }
    Ok(false)
}

/// Called after the caller has parsed `--home`; activation does not consume the user command.
pub fn activate_staged(config: &Config) -> Result<Activation> {
    let Ok(updater) = updater(config.home.clone()) else {
        return Ok(Activation::None);
    };
    updater.activate_staged(|| {
        Config::load(Some(config.home.clone()))
            .map(|fresh| fresh.check_updates)
            .unwrap_or(false)
    })
}

pub fn report_staged_update_error() {}
