use anyhow::{Context, Result, bail};

use crate::config::Config;

pub struct Status {
    receive_enabled: bool,
    runtime_unwired: bool,
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
}
#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub display_name: String,
    pub key_version: u32,
    pub status: String,
    pub fingerprint: String,
}
#[cfg(any(unix, windows))]
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
    }
}

#[cfg(any(unix, windows))]
fn destination(grant: filebeam_daemon::DestinationGrant) -> Destination {
    Destination {
        id: grant.id.to_string(),
        alias: grant.alias,
        revision: grant.revision,
        enabled: grant.enabled,
        ask_first: matches!(grant.approval, filebeam_daemon::ApprovalMode::Ask),
    }
}

#[cfg(any(unix, windows))]
fn device(value: filebeam_daemon::DaemonDevice) -> Device {
    Device {
        id: value.id,
        display_name: value.display_name,
        key_version: value.key_version,
        status: value.status,
        fingerprint: value.fingerprint,
    }
}

#[cfg(any(unix, windows))]
fn daemon_state(state: filebeam_daemon::DaemonState) -> Status {
    Status {
        receive_enabled: state.receive_enabled,
        runtime_unwired: matches!(
            state.runtime,
            filebeam_daemon::RuntimeAvailability::AwaitingPeerConfiguration
        ),
    }
}

#[cfg(any(unix, windows))]
pub fn status(config: &Config) -> Result<Status> {
    use filebeam_daemon::Request;

    let reply = filebeam_daemon::request(&config.home, Request::Status { version: filebeam_daemon::IPC_VERSION })
        .context("Filebeam daemon is not running; install and start filebeamd before enabling device receiving")?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(daemon_state(reply.state))
}

#[cfg(any(unix, windows))]
pub fn set_receive_enabled(config: &Config, enabled: bool) -> Result<Status> {
    use filebeam_daemon::Request;

    let reply = filebeam_daemon::request(
        &config.home,
        Request::SetReceiveEnabled {
            version: filebeam_daemon::IPC_VERSION,
            enabled,
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(daemon_state(reply.state))
}

#[cfg(any(unix, windows))]
pub fn shutdown(config: &Config) -> Result<()> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::Shutdown {
            version: filebeam_daemon::IPC_VERSION,
        },
    )?;
    reply.error.map_or(Ok(()), |error| bail!(error))
}

#[cfg(any(unix, windows))]
pub fn destinations(config: &Config) -> Result<Vec<Destination>> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::ListDestinations {
            version: filebeam_daemon::IPC_VERSION,
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(reply.destinations.into_iter().map(destination).collect())
}

#[cfg(any(unix, windows))]
pub fn add_destination(
    config: &Config,
    alias: &str,
    path: &std::path::Path,
    auto_approve: bool,
) -> Result<Vec<Destination>> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::AddDestination {
            version: filebeam_daemon::IPC_VERSION,
            alias: alias.into(),
            path: path.into(),
            approval: if auto_approve {
                filebeam_daemon::ApprovalMode::Auto
            } else {
                filebeam_daemon::ApprovalMode::Ask
            },
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(reply.destinations.into_iter().map(destination).collect())
}

#[cfg(any(unix, windows))]
pub fn revoke_destination(config: &Config, alias_or_id: &str) -> Result<Vec<Destination>> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::RevokeDestination {
            version: filebeam_daemon::IPC_VERSION,
            alias_or_id: alias_or_id.into(),
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(reply.destinations.into_iter().map(destination).collect())
}

#[cfg(any(unix, windows))]
pub fn jobs(config: &Config) -> Result<Vec<Job>> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::ListJobs {
            version: filebeam_daemon::IPC_VERSION,
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(reply.jobs.into_iter().map(job).collect())
}
#[cfg(any(unix, windows))]
pub fn devices(config: &Config) -> Result<Vec<Device>> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::ListDevices {
            version: filebeam_daemon::IPC_VERSION,
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(reply.devices.into_iter().map(device).collect())
}
#[cfg(any(unix, windows))]
pub fn confirm_peer(
    config: &Config,
    device_id: &str,
    key_version: u32,
    fingerprint: &str,
    confirmation_nonce: &str,
) -> Result<()> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::ConfirmPeerVerification {
            version: filebeam_daemon::IPC_VERSION,
            device_id: device_id.into(),
            key_version,
            fingerprint: fingerprint.into(),
            confirmation_nonce: confirmation_nonce.into(),
        },
    )?;
    reply.error.map_or(Ok(()), |error| bail!(error))
}
#[cfg(any(unix, windows))]
pub fn provision_device(
    config: &Config,
    provision: filebeam_daemon::DeviceProvision,
) -> Result<()> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::ProvisionDevice {
            version: filebeam_daemon::IPC_VERSION,
            provision,
        },
    )?;
    reply.error.map_or(Ok(()), |error| bail!(error))
}
#[cfg(any(unix, windows))]
pub fn enqueue_job(
    config: &Config,
    target: &str,
    grant_id: &str,
    sources: Vec<std::path::PathBuf>,
) -> Result<Job> {
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::EnqueueJob {
            version: filebeam_daemon::IPC_VERSION,
            target: target.into(),
            grant_id: grant_id.into(),
            sources,
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    reply
        .jobs
        .into_iter()
        .next()
        .map(job)
        .context("daemon did not create a device job")
}
#[cfg(any(unix, windows))]
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
    let reply = filebeam_daemon::request(
        &config.home,
        filebeam_daemon::Request::ControlJob {
            version: filebeam_daemon::IPC_VERSION,
            id,
            action,
        },
    )?;
    if let Some(error) = reply.error {
        bail!(error);
    }
    Ok(reply.jobs.into_iter().map(job).collect())
}

#[cfg(not(any(unix, windows)))]
pub fn status(_: &Config) -> Result<Status> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}

#[cfg(not(any(unix, windows)))]
pub fn set_receive_enabled(_: &Config, _: bool) -> Result<Status> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}

#[cfg(not(any(unix, windows)))]
pub fn shutdown(_: &Config) -> Result<()> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}

#[cfg(not(any(unix, windows)))]
pub fn destinations(_: &Config) -> Result<Vec<Destination>> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn add_destination(
    _: &Config,
    _: &str,
    _: &std::path::Path,
    _: bool,
) -> Result<Vec<Destination>> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn revoke_destination(_: &Config, _: &str) -> Result<Vec<Destination>> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn jobs(_: &Config) -> Result<Vec<Job>> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn devices(_: &Config) -> Result<Vec<Device>> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn confirm_peer(_: &Config, _: &str, _: u32, _: &str, _: &str) -> Result<()> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn provision_device(_: &Config, _: filebeam_daemon::DeviceProvision) -> Result<()> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn enqueue_job(_: &Config, _: &str, _: &str, _: Vec<std::path::PathBuf>) -> Result<Job> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}
#[cfg(not(any(unix, windows)))]
pub fn control_job(_: &Config, _: &str, _: &str) -> Result<Vec<Job>> {
    bail!("the Filebeam daemon IPC transport is not implemented on this platform")
}

pub fn status_line(state: &Status) -> &'static str {
    if !state.receive_enabled {
        "Daemon running; receiving paused."
    } else if state.runtime_unwired {
        "Daemon running; receiving is enabled but registered-device configuration is unavailable."
    } else {
        "Daemon running."
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
    format!(
        "{}\t{}\t{}\t{}\tprepared={}",
        job.id,
        crate::presentation::clean(&job.target),
        crate::presentation::clean(&job.grant_id),
        job.state,
        job.prepared
    )
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
        };
        assert!(!device_line(&device).contains('\x1b'));
        assert!(!job_line(&job).contains('\x1b'));
    }
}
