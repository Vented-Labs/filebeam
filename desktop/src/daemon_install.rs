//! Explicit desktop entry point for the shared signed daemon component.
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::{
    cell::Cell,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use anyhow::{Context, Result, bail};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use filebeam_client_config::Config;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use filebeam_client_updater::{
    DaemonService, HttpFetcher, Options, Product, Updater, daemon_service_name,
};

/// Installs the signed shared component and enables the existing per-user service.
/// Desktop callers must invoke this only from an explicit background-receiving action.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn enable(config: &Config, version: &str) -> Result<String> {
    let key = option_env!("FILEBEAM_RELEASE_PUBLIC_KEY")
        .filter(|key| !key.is_empty())
        .context("daemon service setup is blocked: this build has no embedded release key")?;
    let updater = Updater::new(Options::for_product(
        config.home.clone(),
        Product::Desktop,
        version,
        key,
    )?);
    let service = DesktopDaemonService {
        home: config.home.clone(),
        started: Cell::new(false),
    };
    updater
        .install_daemon_component_with(
            &HttpFetcher::new(Duration::from_secs(30)),
            "desktop",
            filebeam_daemon::IPC_VERSION,
            1,
            &service,
        )
        .context("daemon service setup is blocked: this signed release catalog has no compatible daemon component")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct DesktopDaemonService {
    home: PathBuf,
    started: Cell<bool>,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl DaemonService for DesktopDaemonService {
    fn drain(&self, _: &Path) -> Result<()> {
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

#[cfg(target_os = "linux")]
fn install_linux_user_service(launcher: &Path, home: &Path) -> Result<()> {
    let launcher = launcher
        .to_str()
        .context("daemon launcher path is not valid UTF-8")?;
    let home = home.to_str().context("profile home is not valid UTF-8")?;
    let service = daemon_service_name(Path::new(home))?;
    if launcher
        .chars()
        .chain(home.chars())
        .any(|character| matches!(character, '"' | '\\' | '\n' | '\r'))
    {
        bail!("daemon launcher and profile paths contain unsupported systemd characters");
    }
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("HOME or XDG_CONFIG_HOME is required for the user service")?;
    let directory = config.join("systemd/user");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join(format!("{service}.service")),
        format!(
            "[Unit]\nDescription=Filebeam local device daemon\n\n[Service]\nType=simple\nExecStart=\"{launcher}\" --home \"{home}\"\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n"
        ),
    )?;
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
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("read user id")?;
    let uid = String::from_utf8(output.stdout).context("read user id")?;
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

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
