//! Per-user supervisor registration for the shared daemon component: a systemd
//! user unit on Linux and a LaunchAgent on macOS. The service never needs
//! administrator rights.
use std::{
    cell::Cell,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::{DaemonService, daemon_service_name};

const READY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceState {
    /// No unit or LaunchAgent is registered for this profile.
    NotInstalled,
    /// Registered but not running.
    Stopped,
    /// The daemon exited with an error; the supervisor may be retrying it.
    Failing { detail: String },
    /// The supervisor reports the daemon as started or starting.
    Running,
}

/// Installation progress reported by [`UserDaemonService`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Starting,
    WaitingForReady,
}

type Callback<'a> = Box<dyn Fn() -> Result<()> + 'a>;

/// Supervises the shared daemon component as a per-user service. `probe` must
/// complete an authenticated IPC round trip; it is polled after each start
/// until the daemon answers or the readiness timeout elapses.
pub struct UserDaemonService<'a> {
    home: PathBuf,
    probe: Callback<'a>,
    drain: Callback<'a>,
    progress: Box<dyn Fn(Step) + 'a>,
    ready_timeout: Duration,
    started: Cell<bool>,
}

impl<'a> UserDaemonService<'a> {
    pub fn new(
        home: PathBuf,
        probe: impl Fn() -> Result<()> + 'a,
        drain: impl Fn() -> Result<()> + 'a,
    ) -> Self {
        Self {
            home,
            probe: Box::new(probe),
            drain: Box::new(drain),
            progress: Box::new(|_| {}),
            ready_timeout: READY_TIMEOUT,
            started: Cell::new(false),
        }
    }

    pub fn on_progress(mut self, progress: impl Fn(Step) + 'a) -> Self {
        self.progress = Box::new(progress);
        self
    }
}

impl DaemonService for UserDaemonService<'_> {
    fn drain(&self, _: &Path) -> Result<()> {
        (self.drain)().context("request bounded daemon checkpoint before upgrade")
    }

    fn start(&self, launcher: &Path) -> Result<()> {
        (self.progress)(Step::Starting);
        install(launcher, &self.home)?;
        self.started.set(true);
        Ok(())
    }

    fn health(&self, candidate: &Path) -> Result<()> {
        let metadata = fs::metadata(candidate).context("inspect installed daemon component")?;
        if !metadata.is_file() || metadata.len() == 0 {
            bail!("installed daemon component failed health validation");
        }
        if self.started.get() {
            (self.progress)(Step::WaitingForReady);
            wait_ready(&self.probe, || failure(&self.home), self.ready_timeout)?;
        }
        Ok(())
    }

    fn rollback(&self, launcher: Option<&Path>) -> Result<()> {
        match launcher {
            Some(launcher) => install(launcher, &self.home),
            // A first install has no launcher to restore. Unregister the
            // service so the supervisor does not restart a missing launcher.
            None => uninstall(&self.home),
        }
    }
}

/// Polls `probe` with backoff. `failing` lets the supervisor end the wait early
/// once it reports that the daemon exited.
fn wait_ready(
    probe: impl Fn() -> Result<()>,
    failing: impl Fn() -> Option<String>,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut delay = Duration::from_millis(50);
    loop {
        let error = match probe() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if let Some(detail) = failing() {
            return Err(error.context(format!("the daemon exited during startup ({detail})")));
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(error.context(format!(
                "the daemon did not respond within {} seconds",
                timeout.as_secs()
            )));
        }
        thread::sleep(delay.min(deadline - now));
        delay = (delay * 2).min(Duration::from_millis(500));
    }
}

fn failure(home: &Path) -> Option<String> {
    match state(home) {
        Ok(ServiceState::Failing { detail }) => Some(detail),
        _ => None,
    }
}

/// The invoking account when this process was elevated with sudo or doas.
/// Installing from such a shell would register the service for root instead.
pub fn elevated_from() -> Option<String> {
    if unsafe { libc::geteuid() } != 0 {
        return None;
    }
    ["SUDO_USER", "DOAS_USER"]
        .into_iter()
        .find_map(|name| env::var(name).ok().filter(|user| !user.is_empty()))
}

/// Registers `launcher` with the user's supervisor and (re)starts it.
#[cfg(target_os = "linux")]
pub fn install(launcher: &Path, home: &Path) -> Result<()> {
    let unit = linux_service_unit(launcher, home)?;
    let path = unit_path(home)?;
    fs::create_dir_all(path.parent().context("systemd unit path has no parent")?)?;
    fs::write(&path, unit).with_context(|| format!("write {}", path.display()))?;
    let name = unit_name(home)?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", &name])?;
    // Clears a start-rate limit left by an earlier failed install.
    let _ = systemctl(&["reset-failed", &name]);
    systemctl(&["restart", &name])?;
    Ok(())
}

/// Stops and unregisters this profile's service. A missing unit is not an error.
#[cfg(target_os = "linux")]
pub fn uninstall(home: &Path) -> Result<()> {
    let path = unit_path(home)?;
    if !path.exists() {
        return Ok(());
    }
    let name = unit_name(home)?;
    let stopped = systemctl(&["disable", "--now", &name]);
    fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    let _ = systemctl(&["reset-failed", &name]);
    systemctl(&["daemon-reload"])?;
    stopped.map(|_| ())
}

#[cfg(target_os = "linux")]
pub fn state(home: &Path) -> Result<ServiceState> {
    if !unit_path(home)?.exists() {
        return Ok(ServiceState::NotInstalled);
    }
    let properties = systemctl(&[
        "show",
        &unit_name(home)?,
        "--property=ActiveState,SubState,Result,ExecMainStatus",
    ])?;
    Ok(parse_systemd_state(&properties))
}

/// A command that shows the service's recent output.
#[cfg(target_os = "linux")]
pub fn log_command(home: &Path) -> Result<String> {
    Ok(format!("journalctl --user -u {} -n 50", unit_name(home)?))
}

#[cfg(target_os = "linux")]
fn unit_name(home: &Path) -> Result<String> {
    Ok(format!("{}.service", daemon_service_name(home)?))
}

#[cfg(target_os = "linux")]
fn unit_path(home: &Path) -> Result<PathBuf> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("HOME or XDG_CONFIG_HOME is required for the user service")?;
    Ok(config.join("systemd/user").join(unit_name(home)?))
}

#[cfg(target_os = "linux")]
fn systemctl(arguments: &[&str]) -> Result<String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(arguments)
        .output()
        .context("run systemctl; the Filebeam service requires systemd on Linux")?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if message.contains("connect to bus") || message.contains("user scope bus") {
        bail!(
            "systemd user services are unavailable in this session ({message}). Sign in with a user session, or run `loginctl enable-linger` once so your user manager keeps running"
        );
    }
    bail!(
        "`systemctl --user {}` failed: {}",
        arguments.join(" "),
        if message.is_empty() {
            output.status.to_string()
        } else {
            message
        }
    )
}

#[cfg(target_os = "linux")]
fn parse_systemd_state(properties: &str) -> ServiceState {
    let value = |key: &str| {
        properties
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .unwrap_or("")
    };
    let (active, sub, result, status) = (
        value("ActiveState"),
        value("SubState"),
        value("Result"),
        value("ExecMainStatus"),
    );
    if active == "failed" || sub == "auto-restart" {
        let detail = match (result, status) {
            ("exit-code", "203") => "its launcher could not be executed".to_owned(),
            ("exit-code", status) if !status.is_empty() => format!("exit status {status}"),
            ("", _) => "failed".to_owned(),
            (result, _) => result.replace('-', " "),
        };
        return ServiceState::Failing { detail };
    }
    match active {
        "active" | "activating" | "reloading" => ServiceState::Running,
        _ => ServiceState::Stopped,
    }
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

#[cfg(target_os = "macos")]
pub fn install(launcher: &Path, home: &Path) -> Result<()> {
    let launcher = launcher
        .to_str()
        .context("daemon launcher path is not valid UTF-8")?;
    let home_text = home.to_str().context("profile home is not valid UTF-8")?;
    let label = launch_agent_label(home)?;
    let plist = plist_path(home)?;
    fs::create_dir_all(plist.parent().context("LaunchAgent path has no parent")?)?;
    fs::write(
        &plist,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string><string>--home</string><string>{}</string></array><key>RunAtLoad</key><true/></dict></plist>\n",
            xml_escape(&label),
            xml_escape(launcher),
            xml_escape(home_text)
        ),
    )?;
    let domain = gui_domain()?;
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{label}")])
        .output();
    let output = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&plist)
        .output()
        .context("start Filebeam LaunchAgent")?;
    if !output.status.success() {
        bail!(
            "launchctl could not start the Filebeam service: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall(home: &Path) -> Result<()> {
    let plist = plist_path(home)?;
    if !plist.exists() {
        return Ok(());
    }
    let _ = Command::new("launchctl")
        .args([
            "bootout",
            &format!("{}/{}", gui_domain()?, launch_agent_label(home)?),
        ])
        .output();
    fs::remove_file(&plist).with_context(|| format!("remove {}", plist.display()))
}

#[cfg(target_os = "macos")]
pub fn state(home: &Path) -> Result<ServiceState> {
    if !plist_path(home)?.exists() {
        return Ok(ServiceState::NotInstalled);
    }
    let output = Command::new("launchctl")
        .args([
            "print",
            &format!("{}/{}", gui_domain()?, launch_agent_label(home)?),
        ])
        .output()
        .context("inspect Filebeam LaunchAgent")?;
    if !output.status.success() {
        return Ok(ServiceState::Stopped);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if text.lines().any(|line| line.trim() == "state = running") {
        return Ok(ServiceState::Running);
    }
    let exit = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("last exit code = "))
        .unwrap_or("0");
    Ok(if exit == "0" || exit.starts_with('(') {
        ServiceState::Stopped
    } else {
        ServiceState::Failing {
            detail: format!("exit code {exit}"),
        }
    })
}

#[cfg(target_os = "macos")]
pub fn log_command(home: &Path) -> Result<String> {
    Ok(format!(
        "launchctl print {}/{}",
        gui_domain()?,
        launch_agent_label(home)?
    ))
}

#[cfg(target_os = "macos")]
fn plist_path(home: &Path) -> Result<PathBuf> {
    let agents = env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/LaunchAgents"))
        .context("HOME is required for the LaunchAgent")?;
    Ok(agents.join(format!("{}.plist", launch_agent_label(home)?)))
}

#[cfg(target_os = "macos")]
fn gui_domain() -> Result<String> {
    Ok(format!("gui/{}", unsafe { libc::getuid() }))
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn readiness_waits_for_a_slow_daemon() {
        let attempts = Cell::new(0);
        wait_ready(
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() < 4 {
                    bail!("not running")
                }
                Ok(())
            },
            || None,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(attempts.get(), 4);
    }

    #[test]
    fn readiness_stops_when_the_supervisor_reports_a_crash_or_time_runs_out() {
        let error = wait_ready(
            || bail!("not running"),
            || Some("exit status 1".into()),
            Duration::from_secs(30),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("exited during startup (exit status 1)"));

        let started = Instant::now();
        let error =
            wait_ready(|| bail!("not running"), || None, Duration::from_millis(200)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(format!("{error:#}").contains("did not respond"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_properties_map_to_service_states() {
        let state = |text: &str| parse_systemd_state(text);
        assert_eq!(
            state("ActiveState=active\nSubState=running\nResult=success\nExecMainStatus=0\n"),
            ServiceState::Running
        );
        assert_eq!(
            state("ActiveState=inactive\nSubState=dead\nResult=success\nExecMainStatus=0\n"),
            ServiceState::Stopped
        );
        assert_eq!(
            state(
                "ActiveState=activating\nSubState=auto-restart\nResult=exit-code\nExecMainStatus=203\n"
            ),
            ServiceState::Failing {
                detail: "its launcher could not be executed".into()
            }
        );
        assert_eq!(
            state("ActiveState=failed\nSubState=failed\nResult=exit-code\nExecMainStatus=1\n"),
            ServiceState::Failing {
                detail: "exit status 1".into()
            }
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn daemon_unit_uses_the_stable_launcher_and_rejects_unsafe_paths() {
        let unit = linux_service_unit(
            Path::new("/home/alice/.filebeam/runtime/daemon/filebeamd"),
            Path::new("/home/alice/.filebeam"),
        )
        .unwrap();
        assert!(unit.contains("ExecStart=\"/home/alice/.filebeam/runtime/daemon/filebeamd\""));
        assert!(linux_service_unit(Path::new("/bad\"path"), Path::new("/home")).is_err());
    }

    #[test]
    fn launchagent_values_are_xml_escaped_without_shell_interpolation() {
        assert_eq!(xml_escape("a&<>'\"$()"), "a&amp;&lt;&gt;&apos;&quot;$()");
    }
}
