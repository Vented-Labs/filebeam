use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use filebeam_daemon::{IPC_VERSION, Reply, Request};

use crate::{config::Config, guidance::Guidance};

pub struct Status {
    receive_enabled: bool,
    registered: bool,
}

pub struct Destination {
    id: String,
    alias: String,
    revision: u64,
    enabled: bool,
    ask_first: bool,
}
#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub target: String,
    pub grant_id: String,
    pub state: String,
    pub prepared: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub display_name: String,
    pub key_version: u32,
    pub status: String,
    pub fingerprint: String,
}
fn job(value: filebeam_daemon::DeviceJob) -> Job {
    let state = match value.state {
        filebeam_daemon::DeviceJobState::AwaitingPeerConfiguration => "awaiting_peer_configuration",
        filebeam_daemon::DeviceJobState::Preparing => "preparing",
        filebeam_daemon::DeviceJobState::Connecting => "connecting",
        filebeam_daemon::DeviceJobState::Transferring => "transferring",
        filebeam_daemon::DeviceJobState::Delivered => "delivered",
        filebeam_daemon::DeviceJobState::Failed => "failed",
        filebeam_daemon::DeviceJobState::Paused => "paused",
        filebeam_daemon::DeviceJobState::Cancelled => "cancelled",
    };
    Job {
        id: value.id.to_string(),
        target: value.target,
        grant_id: value.grant_id,
        state: state.into(),
        prepared: value.prepared,
        error: value.error,
    }
}

fn destination(grant: filebeam_daemon::DestinationGrant) -> Destination {
    Destination {
        id: grant.id.to_string(),
        alias: grant.alias,
        revision: grant.revision,
        enabled: grant.enabled,
        ask_first: matches!(grant.approval, filebeam_daemon::ApprovalMode::Ask),
    }
}

fn device(value: filebeam_daemon::DaemonDevice) -> Device {
    Device {
        id: value.id,
        display_name: value.display_name,
        key_version: value.key_version,
        status: value.status,
        fingerprint: value.fingerprint,
    }
}

fn daemon_state(state: filebeam_daemon::DaemonState) -> Status {
    Status {
        receive_enabled: state.receive_enabled,
        registered: matches!(
            state.runtime,
            filebeam_daemon::RuntimeAvailability::Provisioned
        ),
    }
}

/// Sends one request and turns a stopped daemon or an unregistered profile
/// into guidance that names the fixing command.
fn call(config: &Config, request: Request) -> Result<Reply> {
    let reply = filebeam_daemon::request(&config.home, request).map_err(|error| {
        if filebeam_daemon::is_not_running(&error) {
            not_running(config).into()
        } else {
            error
        }
    })?;
    match reply.error {
        Some(error) if error == filebeam_daemon::PROVISIONING_REQUIRED => {
            Err(unregistered(config).into())
        }
        Some(error) => bail!(error),
        None => Ok(reply),
    }
}

/// A `beam` command line for this profile, suitable for copying.
pub fn command(config: &Config, arguments: &str) -> String {
    if config.home == filebeam_client_config::default_home() {
        return format!("beam {arguments}");
    }
    let home = config.home.to_string_lossy();
    let home = if home
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"/._-+:@".contains(&byte))
    {
        home.into_owned()
    } else {
        format!("'{}'", home.replace('\'', r"'\''"))
    };
    format!("beam --home {home} {arguments}")
}

fn unregistered(config: &Config) -> Guidance {
    Guidance::new("This computer isn't registered as one of your devices yet.")
        .action("To register it, run", command(config, "devices add"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn not_running(config: &Config) -> Guidance {
    use filebeam_client_updater::user_service::{self, ServiceState};

    let enable = command(config, "service enable");
    match user_service::state(&config.home) {
        Ok(ServiceState::NotInstalled) => {
            Guidance::new("The Filebeam background service isn't set up on this computer.")
                .note("Sending files to your devices and receiving from them runs through it.")
                .action("To set it up, run", enable)
        }
        Ok(ServiceState::Failing { detail }) => {
            let guidance = Guidance::new(format!(
                "The Filebeam background service keeps stopping ({detail})."
            ))
            .action("To repair it, run", enable);
            match user_service::log_command(&config.home) {
                Ok(log) => guidance.action("To see why it stopped, run", log),
                Err(_) => guidance,
            }
        }
        Ok(ServiceState::Running) => {
            Guidance::new("The Filebeam background service isn't responding yet.")
                .note("It may still be starting. Try again in a moment.")
                .action("To restart it, run", enable)
        }
        Ok(ServiceState::Stopped) | Err(_) => {
            Guidance::new("The Filebeam background service isn't running.")
                .action("To start it, run", enable)
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn not_running(_: &Config) -> Guidance {
    Guidance::new("The Filebeam background service isn't running.")
        .note("Setting it up from Beam isn't available on this platform yet.")
}

/// Makes sure the daemon answers before a command that needs it. At an
/// interactive terminal, offers to set the service up and then continues.
pub fn ensure_running(config: &Config, plain: bool) -> Result<()> {
    let error = match filebeam_daemon::request(
        &config.home,
        Request::Status {
            version: IPC_VERSION,
        },
    ) {
        Ok(_) => return Ok(()),
        Err(error) if filebeam_daemon::is_not_running(&error) => not_running(config),
        Err(error) => return Err(error),
    };
    let interactive = !plain && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !cfg!(any(target_os = "linux", target_os = "macos")) || !interactive {
        return Err(error.into());
    }
    eprint!("{}\nSet it up now? [Y/n] ", error.message());
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    ) {
        return Err(error.into());
    }
    install(config, true)?;
    eprintln!("The Filebeam background service is running.\n");
    Ok(())
}

/// Installs or repairs the signed daemon component and waits until it answers.
fn install(config: &Config, progress: bool) -> Result<String> {
    if let Some(user) = elevated_from() {
        return Err(
            Guidance::new("The Filebeam background service runs under your own account.")
                .note("It doesn't need administrator rights, and sudo would set it up for root.")
                .action(format!("As {user}, run"), command(config, "service enable"))
                .into(),
        );
    }
    let step = |message: &str| {
        if progress {
            eprintln!("{message}");
        }
    };
    step("Downloading and verifying the Filebeam background service…");
    crate::update::install_daemon_component(config, &step).map_err(|error| {
        if crate::guidance::find(&error).is_some() {
            return error;
        }
        let mut guidance = Guidance::new("Couldn't set up the Filebeam background service.");
        for cause in error.chain().take(4) {
            guidance = guidance.note(crate::guidance::sentence(&cause.to_string()));
        }
        if let Some(log) = log_command(config) {
            guidance = guidance.action("To see the service log, run", log);
        }
        guidance
            .action("To try again, run", command(config, "service enable"))
            .into()
    })
}

/// `beam service enable`.
pub fn enable(config: &Config, plain: bool) -> Result<()> {
    let launcher = install(config, !plain && std::io::stderr().is_terminal())?;
    if plain {
        crate::output::result(
            &format!("Verified daemon component installed and started at {launcher}."),
            plain,
        )?;
        return Ok(());
    }
    let registered = status(config).is_ok_and(|status| status.registered);
    let mut summary = String::from(
        "The Filebeam background service is running. It starts automatically when you log in.",
    );
    if !registered {
        summary.push_str(&format!(
            "\nNext, register this computer as one of your devices:\n  {}",
            command(config, "devices add")
        ));
    }
    crate::output::result(&summary, plain)?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn elevated_from() -> Option<String> {
    filebeam_client_updater::user_service::elevated_from()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn elevated_from() -> Option<String> {
    None
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn log_command(config: &Config) -> Option<String> {
    filebeam_client_updater::user_service::log_command(&config.home).ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn log_command(_: &Config) -> Option<String> {
    None
}

/// `beam service status`. Returns false when the service is not running.
pub fn print_status(config: &Config, plain: bool) -> Result<bool> {
    match status(config) {
        Ok(state) => {
            crate::output::result(status_line(&state), plain)?;
            if !state.registered && !plain {
                crate::output::result(
                    &format!(
                        "This computer isn't registered as one of your devices yet. To register it, run:\n  {}",
                        command(config, "devices add")
                    ),
                    plain,
                )?;
            }
            Ok(true)
        }
        Err(error) => {
            let Some(guidance) = crate::guidance::find(&error) else {
                return Err(error);
            };
            crate::output::result(&guidance.to_string(), plain)?;
            Ok(false)
        }
    }
}

/// `beam service stop`. Stopping a service that is not running succeeds.
pub fn stop(config: &Config, plain: bool) -> Result<()> {
    match shutdown(config) {
        Ok(()) => crate::output::result("The Filebeam background service stopped.", plain)?,
        Err(error) if crate::guidance::find(&error).is_some() => {
            crate::output::result("The Filebeam background service isn't running.", plain)?
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

pub fn status(config: &Config) -> Result<Status> {
    let reply = call(
        config,
        Request::Status {
            version: IPC_VERSION,
        },
    )?;
    Ok(daemon_state(reply.state))
}

pub fn set_receive_enabled(config: &Config, enabled: bool) -> Result<Status> {
    let reply = call(
        config,
        Request::SetReceiveEnabled {
            version: IPC_VERSION,
            enabled,
        },
    )?;
    Ok(daemon_state(reply.state))
}

pub fn shutdown(config: &Config) -> Result<()> {
    call(
        config,
        Request::Shutdown {
            version: IPC_VERSION,
        },
    )
    .map(drop)
}

pub fn destinations(config: &Config) -> Result<Vec<Destination>> {
    let reply = call(
        config,
        Request::ListDestinations {
            version: IPC_VERSION,
        },
    )?;
    Ok(reply.destinations.into_iter().map(destination).collect())
}

pub fn add_destination(
    config: &Config,
    alias: &str,
    path: &std::path::Path,
    auto_approve: bool,
) -> Result<Vec<Destination>> {
    let reply = call(
        config,
        Request::AddDestination {
            version: IPC_VERSION,
            alias: alias.into(),
            path: path.into(),
            approval: if auto_approve {
                filebeam_daemon::ApprovalMode::Auto
            } else {
                filebeam_daemon::ApprovalMode::Ask
            },
        },
    )?;
    Ok(reply.destinations.into_iter().map(destination).collect())
}

pub fn revoke_destination(config: &Config, alias_or_id: &str) -> Result<Vec<Destination>> {
    let reply = call(
        config,
        Request::RevokeDestination {
            version: IPC_VERSION,
            alias_or_id: alias_or_id.into(),
        },
    )?;
    Ok(reply.destinations.into_iter().map(destination).collect())
}

pub fn jobs(config: &Config) -> Result<Vec<Job>> {
    let reply = call(
        config,
        Request::ListJobs {
            version: IPC_VERSION,
        },
    )?;
    Ok(reply.jobs.into_iter().map(job).collect())
}

pub fn devices(config: &Config) -> Result<Vec<Device>> {
    let reply = call(
        config,
        Request::ListDevices {
            version: IPC_VERSION,
        },
    )?;
    Ok(reply.devices.into_iter().map(device).collect())
}

pub fn confirm_peer(
    config: &Config,
    device_id: &str,
    key_version: u32,
    fingerprint: &str,
    confirmation_nonce: &str,
) -> Result<()> {
    call(
        config,
        Request::ConfirmPeerVerification {
            version: IPC_VERSION,
            device_id: device_id.into(),
            key_version,
            fingerprint: fingerprint.into(),
            confirmation_nonce: confirmation_nonce.into(),
        },
    )
    .map(drop)
}

pub fn approve_device(
    config: &Config,
    approval: filebeam_daemon::DeviceApproval,
    fingerprint: &str,
) -> Result<()> {
    call(
        config,
        Request::ApproveDevice {
            version: IPC_VERSION,
            approval,
            fingerprint: fingerprint.into(),
        },
    )
    .map(drop)
}

pub fn provision_device(
    config: &Config,
    provision: filebeam_daemon::DeviceProvision,
) -> Result<()> {
    call(
        config,
        Request::ProvisionDevice {
            version: IPC_VERSION,
            provision,
        },
    )
    .map(drop)
}

pub fn enqueue_job(
    config: &Config,
    target: &str,
    grant_id: &str,
    sources: Vec<std::path::PathBuf>,
) -> Result<Job> {
    call(
        config,
        Request::EnqueueJob {
            version: IPC_VERSION,
            target: target.into(),
            grant_id: grant_id.into(),
            sources,
        },
    )?
    .jobs
    .into_iter()
    .next()
    .map(job)
    .context("daemon did not create a device job")
}

pub fn control_job(config: &Config, id: &str, action: &str) -> Result<Vec<Job>> {
    let id = uuid::Uuid::parse_str(id).context("device job ID must be a UUID")?;
    let action = match action {
        "paused" => filebeam_daemon::DeviceJobState::Paused,
        "awaiting_peer_configuration" | "awaiting_peer_orchestrator" => {
            filebeam_daemon::DeviceJobState::AwaitingPeerConfiguration
        }
        "cancelled" => filebeam_daemon::DeviceJobState::Cancelled,
        _ => bail!("invalid device job action"),
    };
    let reply = call(
        config,
        Request::ControlJob {
            version: IPC_VERSION,
            id,
            action,
        },
    )?;
    Ok(reply.jobs.into_iter().map(job).collect())
}

pub fn status_line(state: &Status) -> &'static str {
    match (state.registered, state.receive_enabled) {
        (false, _) => "The Filebeam background service is running.",
        (true, false) => "The Filebeam background service is running; receiving is paused.",
        (true, true) => "The Filebeam background service is running and receiving.",
    }
}

pub fn destination_line(destination: &Destination) -> String {
    let approval = if destination.ask_first { "ask" } else { "auto" };
    let state = if destination.enabled {
        "enabled"
    } else {
        "revoked"
    };
    format!(
        "{}\t{}\t{}\t{}\t{}",
        destination.id,
        crate::presentation::clean(&destination.alias),
        destination.revision,
        approval,
        state
    )
}
pub fn job_line(job: &Job) -> String {
    let mut line = format!(
        "{}\t{}\t{}\t{}\tprepared={}",
        job.id,
        crate::presentation::clean(&job.target),
        crate::presentation::clean(&job.grant_id),
        job.state,
        job.prepared
    );
    if let Some(error) = &job.error {
        line.push('\t');
        line.push_str(&crate::presentation::clean(error));
    }
    line
}

pub fn device_line(device: &Device) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        crate::presentation::clean(&device.display_name),
        device.id,
        device.status,
        device.key_version,
        crate::presentation::clean(&device.fingerprint)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_rows_strip_terminal_controls_on_every_platform() {
        let device = Device {
            id: "device-id".into(),
            display_name: "\x1b]52;c;bad\x07Laptop".into(),
            key_version: 1,
            status: "active".into(),
            fingerprint: "\x1b]8;;https://invalid\x1b\\abcdef".into(),
        };
        let job = Job {
            id: "job-id".into(),
            target: "\x1b]52;c;bad\x07Laptop".into(),
            grant_id: "drop\x1b]8;;https://invalid\x1b\\zone".into(),
            state: "queued".into(),
            prepared: false,
            error: Some("\x1b]52;c;bad\x07offline".into()),
        };
        assert!(!device_line(&device).contains('\x1b'));
        assert!(!job_line(&job).contains('\x1b'));
    }

    // Windows validates the profile directory before looking for the pipe.
    #[cfg(unix)]
    #[test]
    fn stopped_daemons_and_unregistered_profiles_name_the_fixing_command() {
        let home = tempfile::tempdir().unwrap();
        let config = Config::load(Some(home.path().into())).unwrap();
        let error = devices(&config).unwrap_err();
        let guidance = crate::guidance::find(&error).expect("guidance for a stopped daemon");
        assert!(guidance.message().contains("background service"));
        assert!(
            guidance.to_string().contains(&format!(
                "`beam --home {} service enable`",
                home.path().display()
            )),
            "{guidance}"
        );
        assert!(unregistered(&config).to_string().ends_with("devices add`."));
    }

    #[test]
    fn suggested_commands_quote_custom_profiles() {
        let mut config = Config::load(Some("/tmp/it's here".into())).unwrap();
        assert_eq!(
            command(&config, "service enable"),
            r"beam --home '/tmp/it'\''s here' service enable"
        );
        config.home = filebeam_client_config::default_home();
        assert_eq!(command(&config, "devices add"), "beam devices add");
    }
}
