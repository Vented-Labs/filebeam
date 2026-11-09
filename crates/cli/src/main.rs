mod app;
mod clipboard;
mod config;
mod daemon;
mod inline;
mod input;
mod output;
mod presentation;
mod protocol;
mod services;
mod terminal;
mod tui;
mod update;
mod uploads;

use std::{
    env,
    io::{IsTerminal, Write},
    path::PathBuf,
    process::Command as ProcessCommand,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{MAX_CONCURRENCY, MAX_MEMORY_LIMIT_MIB, MIN_MEMORY_LIMIT_MIB};

#[cfg(test)]
fn configured_instance(value: &str) -> String {
    filebeam_client_config::normalize_server_url(value).expect("CLI instance was validated")
}

#[derive(Parser)]
#[command(
    name = "beam",
    version = env!("BEAM_VERSION"),
    about = "Private, end-to-end encrypted file sharing"
)]
struct Cli {
    /// Use this directory instead of ~/.filebeam for configuration and cache data.
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    /// Use this server for this invocation without changing config.toml.
    #[arg(long, global = true)]
    instance: Option<String>,
    /// Use stable, machine-friendly output.
    #[arg(long, global = true)]
    plain: bool,
    /// Disable colored output.
    #[arg(long, global = true)]
    no_color: bool,
    /// Disable decorative animation and progress interpolation.
    #[arg(long, global = true)]
    reduced_motion: bool,
    /// Allow WebRTC downloads to expose your network address to the sender.
    #[arg(long, global = true)]
    accept_peer_address_exposure: bool,
    /// Require WebRTC transfers to use a TURN UDP relay and not expose peer addresses.
    #[arg(long, global = true)]
    webrtc_relay_only: bool,
    /// Limit simultaneous transfer requests (1-64).
    #[arg(long, global = true, value_parser = clap::value_parser!(u32).range(1..=MAX_CONCURRENCY as i64))]
    max_concurrency: Option<u32>,
    /// Total memory budget for transfer buffers in MiB (64-4096).
    #[arg(long, global = true, value_parser = clap::value_parser!(u64).range(MIN_MEMORY_LIMIT_MIB..=MAX_MEMORY_LIMIT_MIB))]
    memory_limit_mib: Option<u64>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List and manage outgoing transfers owned by your signed-in account.
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    /// Send files to a registered device through the local daemon.
    To {
        #[arg(value_parser = device_or_directed_username)]
        recipient: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Receiver destination grant alias or UUID. Defaults to dropzone.
        #[arg(long)]
        into: Option<String>,
        /// Wait for a verified receiver receipt instead of returning once queued.
        #[arg(long, conflicts_with = "detach")]
        wait: bool,
        /// Return after the daemon has prepared the durable local snapshot.
        #[arg(long, conflicts_with = "wait")]
        detach: bool,
        /// Stop waiting after this many seconds without cancelling the durable job.
        #[arg(long, requires = "wait", value_parser = clap::value_parser!(u64).range(1..))]
        wait_timeout: Option<u64>,
        /// Print the queued job as JSON.
        #[arg(long)]
        json: bool,
        #[arg(long, conflicts_with = "individual")]
        zip: bool,
        #[arg(long, conflicts_with = "zip")]
        individual: bool,
    },
    /// Inspect or control the local daemon-backed device receiver.
    Receive {
        #[command(subcommand)]
        command: ReceiveCommand,
    },
    /// Inspect or stop the local Filebeam daemon.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Manage local receiver-owned destination grants.
    Destinations {
        #[command(subcommand)]
        command: DestinationCommand,
    },
    /// Inspect or control daemon-owned device jobs.
    Jobs {
        #[command(subcommand)]
        command: DeviceJobsCommand,
    },
    /// List, verify, and manage registered devices.
    Devices {
        #[command(subcommand)]
        command: DevicesCommand,
    },
    /// Manage mutual friends and your own incoming permissions.
    Contacts {
        #[command(subcommand)]
        command: ContactsCommand,
    },
    /// Encrypt and upload files or directories.
    Up {
        files: Vec<PathBuf>,
        /// Transfer over the HTTP relay or directly over WebRTC.
        #[arg(long, value_enum, default_value_t = Transport::Http)]
        transport: Transport,
        /// Publish a Turbo HTTP transfer so recipients can receive verified chunks while uploading.
        #[arg(long)]
        turbo: bool,
        /// Require a transfer password in addition to the link key.
        #[arg(long)]
        password: bool,
        /// Retain the transfer for this many hours when offered by the instance.
        #[arg(long)]
        retention_hours: Option<u64>,
        /// Combine the selection into one ZIP archive before encrypting.
        #[arg(long, conflicts_with = "individual")]
        zip: bool,
        /// Recursively send regular files; each counts against the file limit.
        #[arg(long, conflicts_with = "zip")]
        individual: bool,
        /// Deliver to one validated account username. This requires HTTP without Turbo or password protection.
        #[arg(long)]
        username: Option<String>,
        /// Attach UTF-8 text from a file, or use - to read standard input (64 KiB maximum).
        #[arg(long)]
        note_file: Option<PathBuf>,
        #[arg(long, requires = "note_file")]
        note_title: Option<String>,
        #[arg(long, requires = "note_file")]
        note_language: Option<String>,
    },
    /// Download and verify a shared link.
    Down {
        link: String,
        /// Save the attached note to a new file, or use - for stdout.
        #[arg(long)]
        note_output: Option<PathBuf>,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },
    /// Check the signed release catalog for an update.
    Update,
    /// List resumable transfers stored on this device.
    Transfers,
    /// Resume a saved transfer.
    Resume {
        id: String,
        /// Export the attached note after a resumed download; - writes to stdout.
        #[arg(long)]
        note_output: Option<PathBuf>,
    },
    /// Discard a saved transfer and its local resume state.
    Cancel { id: String },
    /// Revoke the remote upload using its durable delete capability.
    Revoke { id: String },
    /// End a live WebRTC share without discarding its local recovery state.
    EndLive { id: String },
    /// Open an authenticated account inbox delivery.
    Inbox {
        #[command(subcommand)]
        command: InboxCommand,
    },
    /// Create or read encrypted hosted notes.
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// Manage the account session for this instance origin.
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
}

#[derive(Subcommand)]
enum NoteCommand {
    /// Create an encrypted hosted note from a file or standard input.
    Create {
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "plain")]
        language: String,
        #[arg(long)]
        password: bool,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
        #[arg(long)]
        burn_on_read: bool,
        #[arg(long)]
        retention_hours: Option<u64>,
        /// Print the link and its key separately.
        #[arg(long)]
        separate_key: bool,
        /// Serve this note over a live WebRTC share until Ctrl+C explicitly ends it.
        #[arg(long)]
        live: bool,
    },
    /// Decrypt and print a hosted note. Burn notes are consumed only after successful decrypt.
    Open {
        link: String,
        #[arg(long)]
        password: bool,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Subcommand)]
enum ReceiveCommand {
    Status,
    Pause,
    Enable,
}

#[derive(Subcommand)]
enum ServiceCommand {
    Status,
    /// Install a daemon only from this build's signed release catalog.
    Enable,
    /// Stop the daemon after it has persisted its local control state.
    Stop,
}

#[derive(Subcommand)]
enum DestinationCommand {
    List,
    /// Create a local destination grant. Custom aliases ask before receiving by default.
    Add {
        alias: String,
        path: PathBuf,
        /// Permit unattended receives after the shared receiver runtime authorizes a trusted sender.
        #[arg(long)]
        auto_approve: bool,
    },
    Revoke {
        alias_or_id: String,
    },
}

#[derive(Subcommand)]
enum DeviceJobsCommand {
    List,
    Show { id: String },
    Wait { id: String },
    Pause { id: String },
    Resume { id: String },
    Cancel { id: String },
}

#[derive(Subcommand)]
enum DevicesCommand {
    List,
    /// Register this CLI profile as a device and provision the local daemon after approval.
    Add {
        /// Device name. Required when standard input is not a terminal.
        #[arg(long)]
        name: Option<String>,
    },
    /// Approve a pending device from its `devices add` payload, read from standard input.
    Approve {
        /// The candidate's full fingerprint, compared out of band.
        #[arg(long)]
        fingerprint: String,
    },
    /// Confirm an active peer's full fingerprint after comparing it out of band.
    Pair {
        target: String,
        #[arg(long)]
        fingerprint: String,
    },
    Rename {
        id: String,
        name: String,
    },
    Revoke {
        id: String,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Subcommand)]
enum AccountCommand {
    /// Set account receiving defaults; automatic receiving is friends-only.
    Receiving {
        #[arg(long, value_parser = ["anyone", "authenticated", "friends", "nobody"])]
        policy: String,
        #[arg(long, action = clap::ArgAction::Set)]
        auto_download: bool,
    },
    /// Sign in and store the same-origin session in encrypted local state.
    Login {
        email: String,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Register a new account and store its same-origin session.
    Register {
        username: String,
        email: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
    /// End the server session and remove its local encrypted cookie state.
    Logout,
    /// Show the authenticated profile for this instance origin.
    Profile,
    /// Send another verification email for the authenticated account.
    ResendVerification,
    /// Verify the authenticated account with the complete signed email link.
    Verify { link: String },
    /// Request a password-recovery email.
    RecoveryRequest { email: String },
    /// Set a new password using a recovery token.
    RecoveryReset {
        email: String,
        token: String,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Generate and register a self-custody receiving key.
    KeySetup {
        /// Store the custody key locally, or encrypt it for password recovery.
        #[arg(long, value_enum, default_value_t = Custody::SelfCustody)]
        custody: Custody,
        #[arg(long)]
        acknowledge_replace: bool,
        #[arg(long)]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Import an owner-only self-custody key file after validating it against the active account key.
    KeyImport {
        file: PathBuf,
        #[arg(long)]
        acknowledge_replace: bool,
    },
    /// Reveal the stored self-custody key only after explicit acknowledgement.
    KeyExport {
        #[arg(long)]
        acknowledge_export: bool,
    },
}

#[derive(Subcommand)]
enum InboxCommand {
    /// Receive eligible friend deliveries into private ciphertext staging until Ctrl+C.
    Watch {
        #[arg(long)]
        once: bool,
    },
    /// List locally staged deliveries.
    Staged,
    /// Verify and save a staged delivery with the local receiving key.
    Save {
        id: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Remove local staging and remember that this delivery was dismissed.
    Dismiss { id: String },
    /// List private deliveries without decrypting their metadata.
    List,
    /// Decrypt metadata and download a delivery with the stored custody key.
    Download {
        id: String,
        /// Export the attached note to a new file; - writes to stdout.
        #[arg(long)]
        note_output: Option<PathBuf>,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum HistoryCommand {
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        driver: Option<String>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, default_value_t = 25, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
    },
    /// Delete the encrypted transfer for all recipients.
    Delete { id: String },
    /// Set total retention from completion/publication, not from now.
    Extend {
        id: String,
        #[arg(long)]
        retention_hours: u64,
    },
}

#[derive(Subcommand)]
enum ContactsCommand {
    List,
    Requests,
    Request {
        username: String,
    },
    Accept {
        username: String,
    },
    Decline {
        username: String,
    },
    Cancel {
        username: String,
    },
    Remove {
        username: String,
    },
    Block {
        username: String,
    },
    Unblock {
        username: String,
    },
    Set {
        username: String,
        #[arg(long, value_enum)]
        can_send: Option<ContactOverride>,
        #[arg(long, value_enum)]
        auto_download: Option<ContactOverride>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ContactOverride {
    Inherit,
    Allow,
    Deny,
}
impl ContactOverride {
    fn value(self) -> Option<bool> {
        match self {
            Self::Inherit => None,
            Self::Allow => Some(true),
            Self::Deny => Some(false),
        }
    }
}

fn directed_username(value: &str) -> std::result::Result<String, String> {
    if !value.starts_with('@') {
        return Err("Use @username for an account destination".into());
    }
    filebeam_client_core::services::contacts::contact_username(value)
        .map(str::to_owned)
        .map_err(|error| error.to_string())
}

fn device_or_directed_username(value: &str) -> std::result::Result<String, String> {
    if value.starts_with('@') {
        directed_username(value).map(|username| format!("@{username}"))
    } else {
        if value.trim().is_empty() || value.len() > 80 || value.contains(['/', '\\', '\0']) {
            Err("device destinations must be a registered device ID or display name".into())
        } else {
            Ok(value.into())
        }
    }
}

fn resolve_device(config: &config::Config, target: &str) -> Result<daemon::Device> {
    resolve_device_from(daemon::devices(config)?, target)
}

fn resolve_device_from(devices: Vec<daemon::Device>, target: &str) -> Result<daemon::Device> {
    let matches = devices
        .into_iter()
        .filter(|device| device.id == target || device.display_name == target)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [device] if device.status == "active" => Ok(daemon::Device {
            id: device.id.clone(),
            display_name: device.display_name.clone(),
            key_version: device.key_version,
            status: device.status.clone(),
            fingerprint: device.fingerprint.clone(),
        }),
        [_] => anyhow::bail!("device is not active"),
        [] => anyhow::bail!("registered device was not found"),
        _ => anyhow::bail!("device display name is ambiguous; use its immutable device ID"),
    }
}

fn normalize_fingerprint(value: &str) -> Result<String> {
    let value = value
        .bytes()
        .filter(u8::is_ascii_hexdigit)
        .map(char::from)
        .collect::<String>()
        .to_ascii_lowercase();
    if value.len() != 64 {
        anyhow::bail!("device fingerprint must contain 64 hexadecimal characters");
    }
    Ok(value)
}

/// Reads the single-line JSON payload printed by `beam devices add` or the desktop app.
fn read_device_approval() -> Result<filebeam_daemon::DeviceApproval> {
    if std::io::stdin().is_terminal() {
        eprint!("Paste the approval payload: ");
        std::io::stderr().flush()?;
    }
    let mut payload = String::new();
    std::io::stdin().read_line(&mut payload)?;
    serde_json::from_str(payload.trim())
        .context("the approval payload must be the JSON printed by `beam devices add`")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn confirm_fingerprint(fingerprint: &str) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("peer verification requires an interactive terminal");
    }
    eprint!("Type the full verified fingerprint to pair: ");
    std::io::stderr().flush()?;
    let mut confirmation = String::new();
    std::io::stdin().read_line(&mut confirmation)?;
    if normalize_fingerprint(confirmation.trim())? != fingerprint {
        anyhow::bail!("fingerprint confirmation did not match");
    }
    Ok(())
}

fn device_name(value: Option<String>) -> Result<String> {
    if let Some(value) = value {
        anyhow::ensure!(!value.trim().is_empty(), "device name must not be empty");
        return Ok(value);
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("devices add requires --name when standard input is not a terminal");
    }
    eprint!("Device name: ");
    std::io::stderr().flush()?;
    let mut value = String::new();
    std::io::stdin().read_line(&mut value)?;
    let value = value.trim();
    anyhow::ensure!(!value.is_empty(), "device name must not be empty");
    Ok(value.into())
}

fn job_by_id(config: &config::Config, id: &str) -> Result<daemon::Job> {
    daemon::jobs(config)?
        .into_iter()
        .find(|job| job.id == id)
        .context("daemon job disappeared")
}

fn wait_for_prepared(config: &config::Config, id: &str, interrupted: &AtomicBool) -> Result<()> {
    loop {
        if interrupted.load(Ordering::Relaxed) {
            anyhow::bail!("wait interrupted; device job remains queued")
        }
        let job = job_by_id(config, id)?;
        if job.prepared {
            return Ok(());
        }
        match job.state.as_str() {
            "cancelled" => anyhow::bail!("device job was cancelled"),
            "failed" => anyhow::bail!(
                "device job failed before its durable snapshot was prepared: {}",
                job.error.as_deref().unwrap_or("unknown error")
            ),
            _ => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
}

fn wait_for_delivery(
    config: &config::Config,
    id: &str,
    plain: bool,
    timeout: Option<Duration>,
    interrupted: &AtomicBool,
) -> Result<()> {
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    loop {
        if interrupted.load(Ordering::Relaxed) {
            anyhow::bail!("wait interrupted; device job remains queued")
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            anyhow::bail!("timed out waiting; device job remains queued")
        }
        let job = job_by_id(config, id)?;
        match job.state.as_str() {
            "delivered" => {
                output::result("Verified receiver receipt persisted.", plain)?;
                return Ok(());
            }
            "cancelled" => anyhow::bail!("device job was cancelled"),
            "failed" => anyhow::bail!(
                "device job failed: {}",
                job.error.as_deref().unwrap_or("unknown error")
            ),
            _ => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Transport {
    Http,
    Webrtc,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Custody {
    #[value(name = "self")]
    SelfCustody,
    Password,
}

impl Transport {
    fn upload_options(
        self,
        turbo: bool,
        password: bool,
        retention_hours: Option<u64>,
    ) -> Result<protocol::UploadOptions> {
        if turbo && self == Self::Webrtc {
            anyhow::bail!(
                "--turbo requires HTTP uploads; remove --transport webrtc or use --transport http"
            );
        }
        Ok(protocol::UploadOptions {
            transport: match self {
                Self::Http => protocol::Transport::Http,
                Self::Webrtc => protocol::Transport::WebRtc,
            },
            turbo,
            password,
            retention_hours,
            ..Default::default()
        })
    }
}

fn main() -> Result<()> {
    if update::apply_staged_update()? {
        return Ok(());
    }
    update::report_staged_update_error();
    if bootstrap_update_startup()? {
        return Ok(());
    }
    let cli = Cli::parse();
    let mut config = config::Config::load(cli.home)?;
    config.no_color |= cli.no_color;
    config.reduced_motion |= cli.reduced_motion;
    config.webrtc_relay_only |= cli.webrtc_relay_only;
    if let Some(value) = cli.max_concurrency {
        config.max_concurrency = Some(value);
    }
    if let Some(value) = cli.memory_limit_mib {
        config.memory_limit_mib = value;
    }
    config.validate_transfer_limits()?;
    let instance = match cli.instance.as_deref() {
        Some(value) => filebeam_client_config::normalize_server_url(value)?,
        None => config.server_url.clone(),
    };
    match cli.command {
        Some(Command::To {
            recipient,
            files,
            into,
            wait,
            detach: _,
            wait_timeout,
            json,
            zip,
            individual,
        }) => {
            if recipient.starts_with('@') {
                let recipient = recipient.trim_start_matches('@');
                let client = services::client(&config, &instance)?;
                let target = client.account().recipient(recipient)?;
                let options = protocol::UploadOptions {
                    authentication: protocol::UploadAuthentication::SessionCookie(
                        client.cookie_context()?,
                    ),
                    recipient: Some(protocol::UploadRecipient {
                        username: target.username.clone(),
                        user_id: target.id,
                        account_key_bundle_id: target.account_key_bundle_id,
                        public_key: target.public_key,
                    }),
                    ..Default::default()
                };
                let mode = if zip {
                    uploads::DirectoryMode::Zip
                } else if individual {
                    uploads::DirectoryMode::Individual
                } else if cli.plain || !std::io::stdin().is_terminal() {
                    uploads::DirectoryMode::RequireFlag
                } else {
                    uploads::DirectoryMode::Ask
                };
                let receipts = inline::run(
                    &config,
                    &instance,
                    app::Request::Upload(files, mode, options),
                    cli.plain,
                    cli.accept_peer_address_exposure,
                )?;
                for receipt in receipts {
                    output::result(&format!("@{}\t{receipt}", target.username), cli.plain)?;
                }
            } else {
                if zip || individual {
                    anyhow::bail!("--zip and --individual apply only to @account delivery");
                }
                let device = resolve_device(&config, &recipient)?;
                let grant_id = into.unwrap_or_else(|| "dropzone".into());
                let job = daemon::enqueue_job(&config, &device.id, &grant_id, files)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "id": job.id,
                            "target": job.target,
                            "destination": job.grant_id,
                            "state": job.state,
                            "prepared": job.prepared,
                        })
                    );
                } else {
                    output::result(&daemon::job_line(&job), cli.plain)?;
                }
                let interrupted = Arc::new(AtomicBool::new(false));
                let _interrupt = terminal::watch_interrupt(interrupted.clone())?;
                if !wait {
                    wait_for_prepared(&config, &job.id, &interrupted)?;
                    std::process::exit(10);
                }
                wait_for_delivery(
                    &config,
                    &job.id,
                    cli.plain,
                    wait_timeout.map(Duration::from_secs),
                    &interrupted,
                )?;
            }
        }
        Some(Command::Contacts { command }) => {
            services::contacts(&config, &instance, command, cli.plain)?
        }
        Some(Command::Receive { command }) => {
            let state = match command {
                ReceiveCommand::Status => daemon::status(&config)?,
                ReceiveCommand::Pause => daemon::set_receive_enabled(&config, false)?,
                ReceiveCommand::Enable => daemon::set_receive_enabled(&config, true)?,
            };
            output::result(daemon::status_line(&state), cli.plain)?;
        }
        Some(Command::Service { command }) => match command {
            ServiceCommand::Status => {
                output::result(daemon::status_line(&daemon::status(&config)?), cli.plain)?
            }
            ServiceCommand::Stop => {
                daemon::shutdown(&config)?;
                output::result("Daemon stopped.", cli.plain)?;
            }
            ServiceCommand::Enable => {
                let launcher = update::install_daemon_component(&config)?;
                output::result(
                    &format!("Verified daemon component installed and started at {launcher}."),
                    cli.plain,
                )?;
            }
        },
        Some(Command::Destinations { command }) => {
            let grants = match command {
                DestinationCommand::List => daemon::destinations(&config)?,
                DestinationCommand::Add {
                    alias,
                    path,
                    auto_approve,
                } => daemon::add_destination(&config, &alias, &path, auto_approve)?,
                DestinationCommand::Revoke { alias_or_id } => {
                    daemon::revoke_destination(&config, &alias_or_id)?
                }
            };
            for grant in grants {
                output::result(&daemon::destination_line(&grant), cli.plain)?;
            }
        }
        Some(Command::Jobs { command }) => {
            let jobs = match command {
                DeviceJobsCommand::List => daemon::jobs(&config)?,
                DeviceJobsCommand::Show { id } => daemon::jobs(&config)?
                    .into_iter()
                    .filter(|job| job.id == id)
                    .collect(),
                DeviceJobsCommand::Wait { id } => {
                    let interrupted = Arc::new(AtomicBool::new(false));
                    let _interrupt = terminal::watch_interrupt(interrupted.clone())?;
                    wait_for_delivery(&config, &id, cli.plain, None, &interrupted)?;
                    Vec::new()
                }
                DeviceJobsCommand::Pause { id } => daemon::control_job(&config, &id, "paused")?,
                DeviceJobsCommand::Resume { id } => {
                    daemon::control_job(&config, &id, "awaiting_peer_configuration")?
                }
                DeviceJobsCommand::Cancel { id } => daemon::control_job(&config, &id, "cancelled")?,
            };
            for job in jobs {
                output::result(&daemon::job_line(&job), cli.plain)?;
            }
        }
        Some(Command::Devices { command }) => match command {
            DevicesCommand::List => {
                for device in daemon::devices(&config)? {
                    output::result(&daemon::device_line(&device), cli.plain)?;
                }
            }
            DevicesCommand::Add { name } => {
                let name = device_name(name)?;
                daemon::status(&config).context(
                    "start the local Filebeam daemon before adding a device; provisioning never writes device keys to an unverified service",
                )?;
                match services::begin_or_resume_device_add(&config, &instance, &name)? {
                    services::DeviceAdd::Pending {
                        id,
                        fingerprint,
                        approval,
                    } => {
                        let mut message = format!(
                            "Enrollment pending approval. Device: {id}\nFingerprint: {fingerprint}"
                        );
                        if let Some(approval) = approval {
                            message.push_str(&format!(
                                "\nOn an active device, run `beam devices approve --fingerprint {fingerprint}` and paste this payload:\n{}\nThen rerun `beam devices add` here within {} minutes.",
                                serde_json::to_string(&approval)?,
                                approval
                                    .expires_at_unix
                                    .saturating_sub(unix_now())
                                    .div_ceil(60)
                            ));
                        } else {
                            message.push_str(
                                "\nApprove it on an active device, then rerun `beam devices add`.",
                            );
                        }
                        output::result(&message, cli.plain)?;
                    }
                    services::DeviceAdd::Provision(provision) => {
                        let id = provision.device_id.clone();
                        daemon::provision_device(&config, provision)?;
                        services::finish_device_add(&config, &instance)?;
                        output::result(
                            &format!("Device {id} registered and provisioned."),
                            cli.plain,
                        )?;
                    }
                }
            }
            DevicesCommand::Approve { fingerprint } => {
                let fingerprint = normalize_fingerprint(&fingerprint)?;
                let approval = read_device_approval()?;
                if services::device_fingerprint(&approval.candidate_public_key)? != fingerprint {
                    anyhow::bail!(
                        "the pasted candidate key does not match the supplied fingerprint"
                    );
                }
                if std::io::stdin().is_terminal() {
                    confirm_fingerprint(&fingerprint)?;
                }
                let candidate = approval.candidate_device_id.clone();
                daemon::approve_device(&config, approval, &fingerprint)?;
                output::result(
                    &format!("Device {candidate} approved and paired with this device."),
                    cli.plain,
                )?;
            }
            DevicesCommand::Pair {
                target,
                fingerprint,
            } => {
                let device = resolve_device(&config, &target)?;
                let fingerprint = normalize_fingerprint(&fingerprint)?;
                if device.fingerprint != fingerprint {
                    anyhow::bail!(
                        "the supplied fingerprint does not match the authenticated device registry"
                    );
                }
                confirm_fingerprint(&fingerprint)?;
                daemon::confirm_peer(
                    &config,
                    &device.id,
                    device.key_version,
                    &fingerprint,
                    &uuid::Uuid::new_v4().simple().to_string(),
                )?;
                output::result("Peer verification persisted.", cli.plain)?;
            }
            DevicesCommand::Rename { id, name } => {
                let device = services::client(&config, &instance)?
                    .devices()
                    .account_device_rename(&id, &name)?;
                output::result(
                    &format!("{}\t{}", device.display_name, device.id),
                    cli.plain,
                )?;
            }
            DevicesCommand::Revoke {
                id,
                password_file,
                password_stdin,
            } => {
                let password =
                    services::secret(password_file.as_deref(), password_stdin, "Account password")?;
                let (device, _) = services::client(&config, &instance)?
                    .devices()
                    .account_device_revoke(&id, &password)?;
                output::result(
                    &format!("{}\t{}", device.display_name, device.status),
                    cli.plain,
                )?;
            }
        },
        Some(Command::Up {
            files,
            transport,
            turbo,
            password,
            retention_hours,
            zip,
            individual,
            username,
            note_file,
            note_title,
            note_language,
        }) => {
            if files.is_empty() {
                anyhow::bail!("provide at least one file or directory");
            }
            let mode = if zip {
                uploads::DirectoryMode::Zip
            } else if individual {
                uploads::DirectoryMode::Individual
            } else if cli.plain || !std::io::stdin().is_terminal() {
                uploads::DirectoryMode::RequireFlag
            } else {
                uploads::DirectoryMode::Ask
            };
            let mut options = transport.upload_options(turbo, password, retention_hours)?;
            options.authentication =
                services::client(&config, &instance)?.upload_authentication()?;
            if let Some(path) = note_file {
                let text = if path == std::path::Path::new("-") {
                    services::read_note_text(std::io::stdin(), "attached note")?
                } else {
                    services::read_note_text(
                        std::fs::File::open(&path)
                            .with_context(|| format!("read note input {}", path.display()))?,
                        "attached note",
                    )?
                };
                let note = protocol::AttachedNote {
                    text,
                    title: note_title,
                    language: note_language.unwrap_or_else(|| "plain".into()),
                };
                note.validate().map_err(anyhow::Error::msg)?;
                options.attached_note = Some(note);
            }
            if let Some(username) = username {
                if turbo || password || transport == Transport::Webrtc {
                    anyhow::bail!("username delivery requires HTTP without --turbo or --password");
                }
                let client = services::client(&config, &instance)?;
                let recipient = client.account().recipient(&username)?;
                options.authentication =
                    protocol::UploadAuthentication::SessionCookie(client.cookie_context()?);
                options.recipient = Some(protocol::UploadRecipient {
                    username: recipient.username,
                    user_id: recipient.id,
                    account_key_bundle_id: recipient.account_key_bundle_id,
                    public_key: recipient.public_key,
                });
            }
            for link in inline::run(
                &config,
                &instance,
                app::Request::Upload(files, mode, options),
                cli.plain,
                cli.accept_peer_address_exposure,
            )? {
                output::result(&link, cli.plain)?;
            }
        }
        Some(Command::Down {
            link,
            output,
            note_output,
        }) => {
            let note_stdout = note_output.as_deref() == Some(std::path::Path::new("-"));
            let paths = inline::run(
                &config,
                &instance,
                app::Request::Download {
                    link,
                    output,
                    note_output,
                },
                cli.plain,
                cli.accept_peer_address_exposure,
            )?;
            for path in paths {
                if note_stdout {
                    eprintln!("{}", presentation::clean(&path));
                } else {
                    output::result(&path, cli.plain)?;
                }
            }
        }
        Some(Command::Update) => println!("{}", update::check(&config)?),
        Some(Command::History { command }) => {
            let client = services::client(&config, &instance)?;
            match command {
                HistoryCommand::List {
                    status,
                    kind,
                    driver,
                    cursor,
                    limit,
                } => {
                    let page = client.history().list(
                        &filebeam_client_core::services::HistoryFilter {
                            status,
                            kind,
                            driver,
                        },
                        cursor.as_deref(),
                        limit,
                    )?;
                    for entry in page.data {
                        output::result(&services::history_row(&entry), cli.plain)?;
                    }
                    if let Some(cursor) = page.next_cursor {
                        eprintln!("Older entries: beam history list --cursor {cursor}");
                    }
                }
                HistoryCommand::Delete { id } => {
                    client.history().delete(&id)?;
                    output::result(
                        "Deletion scheduled; the history summary remains for 90 days after cleanup.",
                        cli.plain,
                    )?;
                }
                HistoryCommand::Extend {
                    id,
                    retention_hours,
                } => {
                    let update = client.history().extend(&id, retention_hours)?;
                    output::result(
                        &format!(
                            "{}\t{}\t{}",
                            update.id, update.retention_hours, update.expires_at
                        ),
                        cli.plain,
                    )?;
                }
            }
        }
        Some(Command::Transfers) => {
            for transfer in protocol::saved_transfers(&config.home.join("transfers"))? {
                println!(
                    "{}\t{}\t{}\t{}/{}",
                    transfer.id, transfer.direction, transfer.state, transfer.done, transfer.total
                );
            }
        }
        Some(Command::Resume { id, note_output }) => {
            let note_stdout = note_output.as_deref() == Some(std::path::Path::new("-"));
            let transfer = protocol::saved_transfers(&config.home.join("transfers"))?
                .into_iter()
                .find(|transfer| transfer.id == id)
                .context("saved transfer was not found or is malformed")?;
            let direction = match transfer.direction.as_str() {
                "upload" => app::Direction::Upload,
                "download" => app::Direction::Download,
                _ => unreachable!("saved transfer direction is validated"),
            };
            if note_output.is_some() && direction != app::Direction::Download {
                anyhow::bail!("attached-note exports require a downloaded transfer");
            }
            for value in inline::run(
                &config,
                &instance,
                app::Request::Resume {
                    note_output,
                    id: transfer.id,
                    direction,
                },
                cli.plain,
                cli.accept_peer_address_exposure,
            )? {
                if note_stdout {
                    eprintln!("{}", presentation::clean(&value));
                } else {
                    output::result(&value, cli.plain)?;
                }
            }
        }
        Some(Command::Cancel { id }) => {
            protocol::discard_transfer(&config.home.join("transfers"), &id)?
        }
        Some(Command::Revoke { id }) => {
            inline::run(
                &config,
                &instance,
                app::Request::Revoke { id },
                cli.plain,
                cli.accept_peer_address_exposure,
            )?;
        }
        Some(Command::EndLive { id }) => {
            inline::run(
                &config,
                &instance,
                app::Request::EndLive { id },
                cli.plain,
                cli.accept_peer_address_exposure,
            )?;
        }
        Some(Command::Inbox { command }) => match command {
            InboxCommand::Watch { once } => services::watch_inbox(&config, &instance, once)?,
            InboxCommand::Staged => services::staged_inbox(&config, &instance, None, None)?,
            InboxCommand::Save { id, output } => {
                services::staged_inbox(&config, &instance, Some(&id), Some(&output))?
            }
            InboxCommand::Dismiss { id } => services::dismiss_staged(&config, &instance, &id)?,
            InboxCommand::List => {
                for item in services::client(&config, &instance)?.account().inbox()? {
                    output::result(
                        &format!("{}\t{}\t{}", item.id, item.item_count, item.expires_at),
                        cli.plain,
                    )?;
                }
            }
            InboxCommand::Download {
                id,
                output: destination,
                note_output,
            } => {
                let note_stdout = note_output.as_deref() == Some(std::path::Path::new("-"));
                let client = services::client(&config, &instance)?;
                let key = services::stored_private_key(&config, &instance)?;
                let metadata = client.account().inbox_metadata(&id)?;
                let key = filebeam_client_core::services::open_recipient_key(
                    &key,
                    &metadata.recipient_key,
                    &id,
                )?;
                for path in inline::run(
                    &config,
                    &instance,
                    app::Request::InboxDownload {
                        note_output,
                        id,
                        output: destination,
                        key: key.to_vec(),
                        cookie: client.cookie_context()?,
                    },
                    cli.plain,
                    cli.accept_peer_address_exposure,
                )? {
                    if note_stdout {
                        eprintln!("{}", presentation::clean(&path));
                    } else {
                        output::result(&path, cli.plain)?;
                    }
                }
            }
        },
        Some(Command::Note { command }) => {
            let values = match command {
                NoteCommand::Create {
                    input,
                    title,
                    language,
                    password,
                    password_file,
                    password_stdin,
                    burn_on_read,
                    retention_hours,
                    separate_key,
                    live,
                } => {
                    let text = if let Some(path) = input {
                        services::read_note_text(
                            std::fs::File::open(&path)
                                .with_context(|| format!("read note input {}", path.display()))?,
                            "note input",
                        )?
                    } else {
                        services::read_note_text(std::io::stdin(), "note from standard input")?
                    };
                    if live {
                        if separate_key {
                            anyhow::bail!("--separate-key is not available for a live note");
                        }
                        let password = password
                            .then(|| {
                                services::secret(
                                    password_file.as_deref(),
                                    password_stdin,
                                    "Note password",
                                )
                            })
                            .transpose()?;
                        inline::run(
                            &config,
                            &instance,
                            app::Request::NoteLive(filebeam_client_core::services::NoteCreate {
                                text,
                                title,
                                language,
                                password: password.map(|value| value.to_string()),
                                burn_on_read,
                                retention_hours,
                            }),
                            cli.plain,
                            cli.accept_peer_address_exposure,
                        )?
                    } else {
                        services::note_create(
                            &config,
                            &instance,
                            services::NoteRequest {
                                text,
                                title,
                                language,
                                password_file: password_file.as_deref(),
                                password_stdin,
                                password,
                                burn_on_read,
                                retention_hours,
                                separate_key,
                            },
                        )?
                    }
                }
                NoteCommand::Open {
                    link,
                    password,
                    password_file,
                    password_stdin,
                } => services::note_open(
                    &config,
                    &instance,
                    &link,
                    password_file.as_deref(),
                    password_stdin,
                    password,
                )?,
            };
            for value in values {
                output::result(&value, cli.plain)?;
            }
        }
        Some(Command::Account { command }) => match command {
            AccountCommand::Receiving {
                policy,
                auto_download,
            } => {
                services::client(&config, &instance)?
                    .account()
                    .set_receiving_defaults(&policy, auto_download)?;
                output::result("Receiving defaults saved.", cli.plain)?;
            }
            AccountCommand::Login {
                email,
                password_file,
                password_stdin,
            } => {
                let password =
                    services::secret(password_file.as_deref(), password_stdin, "Account password")?;
                let client = services::client(&config, &instance)?;
                let session = client.account().login(&email, &password, true)?;
                services::save_session(&config, &instance, &client)?;
                output::result(&session.email, cli.plain)?;
            }
            AccountCommand::Register {
                username,
                email,
                name,
                password_file,
                password_stdin,
            } => {
                let password =
                    services::secret(password_file.as_deref(), password_stdin, "Account password")?;
                let client = services::client(&config, &instance)?;
                let session =
                    client
                        .account()
                        .register(&username, name.as_deref(), &email, &password)?;
                services::save_session(&config, &instance, &client)?;
                output::result(&session.email, cli.plain)?;
            }
            AccountCommand::Logout => {
                let client = services::client(&config, &instance)?;
                client.account().logout()?;
                services::clear_session(&config, &instance)?;
            }
            AccountCommand::Profile => {
                let session = services::client(&config, &instance)?.account().session()?;
                output::result(
                    &format!(
                        "{}\t{}\t{}",
                        session.email,
                        session.username.unwrap_or_default(),
                        session.inbox_enabled
                    ),
                    cli.plain,
                )?;
            }
            AccountCommand::ResendVerification => {
                services::client(&config, &instance)?
                    .account()
                    .resend_verification()?;
            }
            AccountCommand::Verify { link } => {
                services::client(&config, &instance)?
                    .account()
                    .verify_email_link(&link)?;
            }
            AccountCommand::RecoveryRequest { email } => {
                services::client(&config, &instance)?
                    .account()
                    .request_password_reset(&email)?;
            }
            AccountCommand::RecoveryReset {
                email,
                token,
                password_file,
                password_stdin,
            } => {
                let password = services::secret(
                    password_file.as_deref(),
                    password_stdin,
                    "New account password",
                )?;
                services::client(&config, &instance)?
                    .account()
                    .reset_password(&email, &token, &password)?;
            }
            AccountCommand::KeySetup {
                custody,
                acknowledge_replace,
                password_file,
                password_stdin,
            } => match custody {
                Custody::SelfCustody => {
                    if password_file.is_some() || password_stdin {
                        anyhow::bail!("password input is only valid with --custody password");
                    }
                    services::setup_self_key(&config, &instance, acknowledge_replace)?;
                }
                Custody::Password => {
                    let password = services::secret(
                        password_file.as_deref(),
                        password_stdin,
                        "Custody password",
                    )?;
                    services::setup_password_key(
                        &config,
                        &instance,
                        &password,
                        acknowledge_replace,
                    )?;
                }
            },
            AccountCommand::KeyImport {
                file,
                acknowledge_replace,
            } => {
                let value = services::private_text_file(&file)?;
                services::import_self_key_for_account(
                    &config,
                    &instance,
                    value.trim(),
                    acknowledge_replace,
                )?;
            }
            AccountCommand::KeyExport { acknowledge_export } => {
                if !acknowledge_export {
                    anyhow::bail!("key export requires --acknowledge-export");
                }
                output::result(&services::export_stored_key(&config, &instance)?, cli.plain)?;
            }
        },
        None => {
            if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() || cli.plain {
                println!("beam {}\n{}", env!("BEAM_VERSION"), instance);
                return Ok(());
            }
            tui::run(&config, &instance).context("terminal UI failed")?;
        }
    }
    Ok(())
}

/// Clap exits for help and version, so schedule and activate before parsing the command.
fn bootstrap_update_startup() -> Result<bool> {
    let home = bootstrap_home();
    if let Ok(config) = config::Config::load(home) {
        if config.check_updates
            && update::activate_staged(&config).unwrap_or(filebeam_client_updater::Activation::None)
                == filebeam_client_updater::Activation::Reexec
        {
            #[cfg(unix)]
            reexec_with_original_arguments(config.home.join("bin/beam"))?;
            #[cfg(windows)]
            return Ok(true);
        }
        update::notify_if_available(&config);
    }
    Ok(false)
}

#[cfg(unix)]
fn reexec_with_original_arguments(executable: PathBuf) -> Result<()> {
    let mut command = ProcessCommand::new(executable);
    command.args(env::args_os().skip(1));
    use std::os::unix::process::CommandExt;
    Err(command.exec().into())
}

fn bootstrap_home() -> Option<PathBuf> {
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--home" {
            return arguments.next().map(PathBuf::from);
        }
        if let Some(value) = argument.to_string_lossy().strip_prefix("--home=") {
            return Some(PathBuf::from(value));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, Command, Custody, DevicesCommand, InboxCommand, NoteCommand, ReceiveCommand,
        ServiceCommand, Transport, configured_instance, resolve_device_from,
    };
    use clap::Parser;

    #[test]
    fn configured_instances_drop_a_trailing_slash() {
        assert_eq!(
            configured_instance("http://localhost:8017/"),
            "http://localhost:8017"
        );
        assert_eq!(
            configured_instance("https://files.company.test/"),
            "https://files.company.test"
        );
    }

    #[test]
    fn transfer_commands_and_bounds_are_parsed() {
        let cli = Cli::try_parse_from([
            "beam",
            "--webrtc-relay-only",
            "--max-concurrency",
            "4",
            "resume",
            "job-1",
        ])
        .unwrap();
        assert_eq!(cli.max_concurrency, Some(4));
        assert!(cli.webrtc_relay_only);
        assert!(matches!(cli.command, Some(Command::Resume { id, .. }) if id == "job-1"));
        assert!(Cli::try_parse_from(["beam", "--memory-limit-mib", "32", "transfers"]).is_err());
    }

    #[test]
    fn daemon_control_commands_are_parsed_without_changing_public_transfers() {
        let cli = Cli::try_parse_from(["beam", "receive", "enable"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Receive {
                command: ReceiveCommand::Enable
            })
        ));
        let cli = Cli::try_parse_from(["beam", "service", "stop"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Service {
                command: ServiceCommand::Stop
            })
        ));
    }

    #[test]
    fn device_add_requires_an_explicit_name_for_noninteractive_use() {
        let cli = Cli::try_parse_from(["beam", "devices", "add", "--name", "Office PC"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Devices {
                command: DevicesCommand::Add { name: Some(name) }
            }) if name == "Office PC"
        ));
    }

    #[test]
    fn account_destinations_and_contact_overrides_are_unambiguous() {
        let cli = Cli::try_parse_from(["beam", "to", "@alice", "report.pdf"]).unwrap();
        assert!(
            matches!(cli.command, Some(Command::To { recipient, files, .. }) if recipient == "@alice" && files == vec![std::path::PathBuf::from("report.pdf")])
        );
        assert!(Cli::try_parse_from(["beam", "to", "alice", "report.pdf"]).is_ok());
        assert!(Cli::try_parse_from(["beam", "to", "@alice@remote", "report.pdf"]).is_err());
        assert!(Cli::try_parse_from(["beam", "to", "@alice"]).is_err());
        assert!(
            Cli::try_parse_from([
                "beam",
                "contacts",
                "set",
                "@alice",
                "--can-send",
                "inherit",
                "--auto-download",
                "deny"
            ])
            .is_ok()
        );
        assert!(Cli::try_parse_from(["beam", "inbox", "watch", "--once"]).is_ok());
    }

    #[test]
    fn device_destinations_are_distinct_from_directed_account_destinations() {
        assert!(matches!(
            Cli::try_parse_from(["beam", "to", "Laptop", "report.pdf"]).unwrap().command,
            Some(Command::To { recipient, .. }) if recipient == "Laptop"
        ));
        assert!(matches!(
            Cli::try_parse_from(["beam", "to", "01ARZ3NDEKTSV4RRFFQ69G5FAV", "report.pdf"]).unwrap().command,
            Some(Command::To { recipient, .. }) if recipient == "01ARZ3NDEKTSV4RRFFQ69G5FAV"
        ));
        assert!(Cli::try_parse_from(["beam", "to", "", "report.pdf"]).is_err());
        assert!(Cli::try_parse_from(["beam", "to", "Laptop/other", "report.pdf"]).is_err());
    }

    #[test]
    fn device_wait_options_require_verified_receipt_waiting() {
        assert!(
            Cli::try_parse_from([
                "beam",
                "to",
                "Laptop",
                "report.pdf",
                "--wait",
                "--wait-timeout",
                "30",
                "--json",
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from(["beam", "to", "Laptop", "report.pdf", "--wait-timeout", "30",])
                .is_err()
        );
    }

    #[test]
    fn device_resolution_rejects_ambiguous_names_in_the_active_daemon_context() {
        let device = |id: &str, name: &str, status: &str| crate::daemon::Device {
            id: id.into(),
            display_name: name.into(),
            key_version: 1,
            status: status.into(),
            fingerprint: "a".repeat(64),
        };
        let resolved =
            resolve_device_from(vec![device("device-a", "Laptop", "active")], "Laptop").unwrap();
        assert_eq!(resolved.id, "device-a");
        assert!(
            resolve_device_from(
                vec![
                    device("device-a", "Laptop", "active"),
                    device("device-b", "Laptop", "active"),
                ],
                "Laptop",
            )
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
        );
        assert!(
            resolve_device_from(vec![device("device-a", "Laptop", "revoked")], "device-a")
                .unwrap_err()
                .to_string()
                .contains("not active")
        );
    }

    #[test]
    fn turbo_is_an_explicit_http_upload_option() {
        let cli = Cli::try_parse_from(["beam", "up", "--turbo", "report.pdf"]).unwrap();
        let Some(Command::Up {
            transport,
            turbo,
            password,
            retention_hours,
            ..
        }) = cli.command
        else {
            panic!("expected upload command");
        };
        let options = transport
            .upload_options(turbo, password, retention_hours)
            .unwrap();
        assert!(options.turbo);
        assert_eq!(options.transport, crate::protocol::Transport::Http);
        assert!(Transport::Webrtc.upload_options(true, false, None).is_err());
    }

    #[test]
    fn note_commands_keep_content_and_secrets_out_of_arguments() {
        let cli = Cli::try_parse_from([
            "beam",
            "note",
            "create",
            "--input",
            "note.md",
            "--password-file",
            "secret",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Note {
                command: NoteCommand::Create {
                    input: Some(_),
                    password_file: Some(_),
                    ..
                }
            })
        ));
        assert!(Cli::try_parse_from(["beam", "note", "create", "plaintext"]).is_err());
    }

    #[test]
    fn account_inbox_and_remote_lifecycle_actions_use_typed_dispatch() {
        assert!(
            matches!(Cli::try_parse_from(["beam", "up", "--username", "alice", "report.pdf"]).unwrap().command, Some(Command::Up { username: Some(value), .. }) if value == "alice")
        );
        assert!(
            matches!(Cli::try_parse_from(["beam", "inbox", "download", "delivery", "--output", "received"]).unwrap().command, Some(Command::Inbox { command: InboxCommand::Download { id, .. } }) if id == "delivery")
        );
        assert!(
            matches!(Cli::try_parse_from(["beam", "end-live", "job"]).unwrap().command, Some(Command::EndLive { id }) if id == "job")
        );
        assert!(
            matches!(Cli::try_parse_from(["beam", "revoke", "job"]).unwrap().command, Some(Command::Revoke { id }) if id == "job")
        );
    }

    #[test]
    fn password_custody_is_an_explicit_key_setup_choice() {
        let cli = Cli::try_parse_from([
            "beam",
            "account",
            "key-setup",
            "--custody",
            "password",
            "--password-stdin",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Account {
                command: super::AccountCommand::KeySetup {
                    custody: Custody::Password,
                    password_stdin: true,
                    ..
                }
            })
        ));
    }
}
