use std::{
    cell::Cell,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use filebeam_client_updater::{
    Activation, DaemonService, Options, Product, Updater, daemon_service_name,
};

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

/// Explicit service setup never downloads an unsigned daemon. A missing optional
/// component in the signed catalog is reported to the caller as unsupported.
pub fn install_daemon_component(config: &Config) -> Result<String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let service = CliDaemonService {
            home: config.home.clone(),
            started: Cell::new(false),
        };
        updater(config.home.clone())?
            .install_daemon_component_with(
                &filebeam_client_updater::HttpFetcher::new(std::time::Duration::from_secs(30)),
                "cli",
                filebeam_daemon::IPC_VERSION,
                filebeam_daemon::STORAGE_SCHEMA,
                &service,
            )
            .context("could not install the daemon from the signed release catalog")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = config;
        bail!(
            "daemon service setup is unavailable on this platform until its runtime reports supported service capabilities"
        )
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct CliDaemonService {
    home: PathBuf,
    started: Cell<bool>,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl DaemonService for CliDaemonService {
    fn drain(&self, _: &Path) -> Result<()> {
        // Shutdown checkpoints jobs before the service is restarted onto the new launcher.
        let reply = filebeam_daemon::request(
            &self.home,
            filebeam_daemon::Request::Shutdown {
                version: filebeam_daemon::IPC_VERSION,
            },
        )
        .context("request bounded daemon checkpoint before upgrade")?;
        reply.error.map_or(Ok(()), |error| bail!(error))
    }

    fn start(&self, launcher: &Path) -> Result<()> {
        install_user_service(launcher, &self.home)?;
        self.started.set(true);
        Ok(())
    }

    fn health(&self, candidate: &Path) -> Result<()> {
        let metadata = fs::metadata(candidate).context("inspect installed daemon component")?;
        if !metadata.is_file() || metadata.len() == 0 {
            bail!("installed daemon component failed health validation");
        }
        if self.started.get() {
            let reply = filebeam_daemon::request(
                &self.home,
                filebeam_daemon::Request::Status {
                    version: filebeam_daemon::IPC_VERSION,
                },
            )
            .context("verify restarted daemon IPC")?;
            reply.error.map_or(Ok(()), |error| bail!(error))?;
        }
        Ok(())
    }

    fn rollback(&self, launcher: Option<&Path>) -> Result<()> {
        if let Some(launcher) = launcher {
            install_user_service(launcher, &self.home)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn install_user_service(launcher: &Path, home: &Path) -> Result<()> {
    install_linux_user_service(launcher, home)
}

#[cfg(target_os = "macos")]
fn install_user_service(launcher: &Path, home: &Path) -> Result<()> {
    let launcher = launcher
        .to_str()
        .context("daemon launcher path is not valid UTF-8")?;
    let home = home.to_str().context("profile home is not valid UTF-8")?;
    let label = launch_agent_label(Path::new(home))?;
    let agents = env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/LaunchAgents"))
        .context("HOME is required for the LaunchAgent")?;
    fs::create_dir_all(&agents)?;
    let plist = agents.join(format!("{label}.plist"));
    fs::write(
        &plist,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>--home</string><string>{}</string></array><key>RunAtLoad</key><true/></dict></plist>\n",
            xml_escape(&label),
            xml_escape(launcher),
            xml_escape(home)
        ),
    )?;
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .context("read user id")?;
    let uid = String::from_utf8(uid.stdout).context("read user id")?;
    let domain = format!("gui/{}/{}", uid.trim(), label);
    let _ = Command::new("launchctl")
        .args(["bootout", &domain])
        .status();
    let status = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{}", uid.trim())])
        .arg(&plist)
        .status()
        .context("start Filebeam LaunchAgent")?;
    if !status.success() {
        bail!("launchctl could not start the Filebeam user daemon service");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_agent_label(home: &Path) -> Result<String> {
    let name = daemon_service_name(home)?;
    Ok(if name == "filebeam-daemon" {
        "io.filebeam.daemon".into()
    } else {
        format!("io.filebeam.daemon-{}", &name["filebeam-daemon-".len()..])
    })
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "linux")]
fn install_linux_user_service(launcher: &Path, home: &Path) -> Result<()> {
    let unit = linux_service_unit(launcher, home)?;
    let service = daemon_service_name(home)?;
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("HOME or XDG_CONFIG_HOME is required for the user service")?;
    let directory = config.join("systemd/user");
    fs::create_dir_all(&directory)?;
    fs::write(directory.join(format!("{service}.service")), unit)?;
    let unit_name = format!("{service}.service");
    for arguments in [
        vec!["--user".into(), "daemon-reload".into()],
        vec![
            "--user".into(),
            "enable".into(),
            "--now".into(),
            unit_name.clone(),
        ],
        vec!["--user".into(), "restart".into(), unit_name.clone()],
    ] {
        let status = Command::new("systemctl")
            .args(arguments)
            .status()
            .context("run systemctl for Filebeam daemon service")?;
        if !status.success() {
            bail!("systemctl could not start the Filebeam user daemon service");
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_service_unit(launcher: &Path, home: &Path) -> Result<String> {
    let launcher = launcher
        .to_str()
        .context("daemon launcher path is not valid UTF-8")?;
    let home = home.to_str().context("profile home is not valid UTF-8")?;
    if launcher
        .chars()
        .chain(home.chars())
        .any(|character| matches!(character, '"' | '\\' | '\n' | '\r'))
    {
        bail!("daemon launcher and profile paths contain unsupported systemd characters");
    }
    Ok(format!(
        "[Unit]\nDescription=Filebeam local device daemon\n\n[Service]\nType=simple\nExecStart=\"{launcher}\" --home \"{home}\"\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n"
    ))
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
    #[cfg(target_os = "linux")]
    use super::linux_service_unit;
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

    #[cfg(target_os = "linux")]
    #[test]
    fn daemon_unit_uses_the_stable_launcher_and_rejects_unsafe_paths() {
        let unit = linux_service_unit(
            std::path::Path::new("/home/alice/.filebeam/runtime/daemon/filebeamd"),
            std::path::Path::new("/home/alice/.filebeam"),
        )
        .unwrap();
        assert!(unit.contains("ExecStart=\"/home/alice/.filebeam/runtime/daemon/filebeamd\""));
        assert!(
            linux_service_unit(
                std::path::Path::new("/bad\"path"),
                std::path::Path::new("/home")
            )
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn launchagent_values_are_xml_escaped_without_shell_interpolation() {
        assert_eq!(
            super::xml_escape("a&<>'\"$()"),
            "a&amp;&lt;&gt;&apos;&quot;$()"
        );
    }
}
