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
    if command.as_deref() == Some(std::ffi::OsStr::new("--beam-apply-update")) {
        let destination = arguments
            .next()
            .map(PathBuf::from)
            .context("legacy update helper is missing destination")?;
        if arguments.next().is_some() {
            bail!("legacy update helper received unexpected arguments");
        }
        #[cfg(windows)]
        return apply_legacy_windows_update(destination);
        #[cfg(not(windows))]
        {
            let _ = destination;
            bail!("Windows update helper is unsupported on this platform");
        }
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("--filebeam-apply-staged")) {
        let product = arguments
            .next()
            .context("Windows update helper is missing product")?;
        let home = arguments
            .next()
            .map(PathBuf::from)
            .context("Windows update helper is missing home")?;
        if product != "cli" {
            bail!("Windows update helper received invalid arguments");
        }
        #[cfg(windows)]
        return updater(home)?.apply_windows_helper(&arguments.collect::<Vec<_>>());
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
        let executable = arguments
            .next()
            .map(PathBuf::from)
            .context("update worker is missing executable")?;
        if arguments.next().is_some() || product != "cli" {
            bail!("update worker received invalid arguments");
        }
        // Re-read after every network boundary; a settings change wins over a running worker.
        let key = option_env!("BEAM_RELEASE_PUBLIC_KEY").context("build has no release key")?;
        let mut options =
            Options::for_product(home.clone(), Product::Cli, env!("BEAM_VERSION"), key)?;
        options.executable = executable;
        Updater::new(options).bootstrap(|| {
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

#[cfg(any(windows, test))]
fn validate_legacy_update_paths(
    candidate: &std::path::Path,
    destination: &std::path::Path,
) -> Result<()> {
    let name = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid legacy candidate")?;
    let pid = name
        .strip_prefix(".beam-update-")
        .and_then(|name| name.strip_suffix(".exe"))
        .context("invalid legacy candidate name")?;
    if pid.is_empty()
        || !pid.bytes().all(|byte| byte.is_ascii_digit())
        || destination.file_name() != Some(std::ffi::OsStr::new("beam.exe"))
        || candidate
            .parent()
            .context("candidate has no directory")?
            .canonicalize()?
            != destination
                .parent()
                .context("destination has no directory")?
                .canonicalize()?
    {
        bail!("legacy update destination is invalid");
    }
    Ok(())
}

#[cfg(windows)]
fn apply_legacy_windows_update(destination: PathBuf) -> Result<bool> {
    use std::{fs, thread, time::Duration};
    let candidate = env::current_exe()?;
    validate_legacy_update_paths(&candidate, &destination)?;
    let backup = destination.with_file_name("beam.previous.exe");
    let error_path = destination.with_file_name("beam.update-error");
    for _ in 0..300 {
        let _ = fs::remove_file(&backup);
        if fs::rename(&destination, &backup).is_ok() {
            if let Err(error) = fs::copy(&candidate, &destination) {
                let _ = fs::rename(&backup, &destination);
                let _ = fs::write(&error_path, error.to_string());
                return Err(error.into());
            }
            let _ = fs::remove_file(error_path);
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(100));
    }
    let error = "timed out waiting for beam.exe to exit before applying update";
    let _ = fs::write(error_path, error);
    bail!(error)
}

pub fn report_staged_update_error() {
    #[cfg(windows)]
    if let Ok(executable) = env::current_exe() {
        let path = executable.with_file_name("beam.update-error");
        if let Ok(error) = std::fs::read_to_string(&path) {
            let _ = std::fs::remove_file(path);
            eprintln!(
                "beam update failed: {}; run `beam update` to retry",
                error.trim()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_legacy_update_paths;

    #[test]
    fn legacy_windows_update_targets_only_its_own_install_directory() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let candidate = root.path().join(".beam-update-123.exe");
        assert!(validate_legacy_update_paths(&candidate, &root.path().join("beam.exe")).is_ok());
        assert!(validate_legacy_update_paths(&candidate, &other.path().join("beam.exe")).is_err());
        assert!(
            validate_legacy_update_paths(&candidate, &root.path().join("filebeam.exe")).is_err()
        );
        assert!(
            validate_legacy_update_paths(
                &root.path().join("beam.exe"),
                &root.path().join("beam.exe")
            )
            .is_err()
        );
    }
}
