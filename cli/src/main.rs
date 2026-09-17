mod app;
mod config;
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

use std::{env, io::IsTerminal, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::config::{MAX_CONCURRENCY, MAX_MEMORY_LIMIT_MIB, MIN_MEMORY_LIMIT_MIB};

const INSTANCE: &str = "https://filebeam.io";

fn instance() -> String {
    configured_instance(env::var("FILEBEAM_INSTANCE").ok())
}

fn configured_instance(value: Option<String>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| INSTANCE.to_owned())
        .trim_end_matches('/')
        .to_owned()
}

#[derive(Parser)]
#[command(
    name = "beam",
    version = env!("BEAM_VERSION"),
    about = "Private, end-to-end encrypted file sharing"
)]
struct Cli {
    /// Use this directory instead of ~/.filebeam for configuration and cache data.
    #[arg(long, global = true, env = "FILEBEAM_HOME")]
    home: Option<PathBuf>,
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
    },
    /// Download and verify a shared link.
    Down {
        link: String,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },
    /// Check the signed release catalog for an update.
    Update,
    /// List resumable transfers stored on this device.
    Transfers,
    /// Resume a saved transfer.
    Resume { id: String },
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
enum AccountCommand {
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
    /// List private deliveries without decrypting their metadata.
    List,
    /// Decrypt metadata and download a delivery with the stored custody key.
    Download {
        id: String,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },
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
    let instance = instance();
    if !matches!(
        &cli.command,
        Some(Command::Update | Command::Transfers | Command::Cancel { .. })
    ) {
        update::notify_if_available(&config);
    }
    match cli.command {
        Some(Command::Up {
            files,
            transport,
            turbo,
            password,
            retention_hours,
            zip,
            individual,
            username,
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
        Some(Command::Down { link, output }) => {
            let paths = inline::run(
                &config,
                &instance,
                app::Request::Download { link, output },
                cli.plain,
                cli.accept_peer_address_exposure,
            )?;
            for path in paths {
                output::result(&path, cli.plain)?;
            }
        }
        Some(Command::Update) => println!("{}", update::check(&config)?),
        Some(Command::Transfers) => {
            for transfer in protocol::saved_transfers(&config.home.join("transfers"))? {
                println!(
                    "{}\t{}\t{}\t{}/{}",
                    transfer.id, transfer.direction, transfer.state, transfer.done, transfer.total
                );
            }
        }
        Some(Command::Resume { id }) => {
            let transfer = protocol::saved_transfers(&config.home.join("transfers"))?
                .into_iter()
                .find(|transfer| transfer.id == id)
                .context("saved transfer was not found or is malformed")?;
            let direction = match transfer.direction.as_str() {
                "upload" => app::Direction::Upload,
                "download" => app::Direction::Download,
                _ => unreachable!("saved transfer direction is validated"),
            };
            for value in inline::run(
                &config,
                &instance,
                app::Request::Resume {
                    id: transfer.id,
                    direction,
                },
                cli.plain,
                cli.accept_peer_address_exposure,
            )? {
                output::result(&value, cli.plain)?;
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
            } => {
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
                        id,
                        output: destination,
                        key: key.to_vec(),
                        cookie: client.cookie_context()?,
                    },
                    cli.plain,
                    cli.accept_peer_address_exposure,
                )? {
                    output::result(&path, cli.plain)?;
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

#[cfg(test)]
mod tests {
    use super::{Cli, Command, Custody, InboxCommand, NoteCommand, Transport, configured_instance};
    use clap::Parser;

    #[test]
    fn production_is_the_default_and_explicit_instances_are_preserved() {
        assert_eq!(configured_instance(None), "https://filebeam.io");
        assert_eq!(
            configured_instance(Some("  ".into())),
            "https://filebeam.io"
        );
        assert_eq!(
            configured_instance(Some("http://localhost:8017/".into())),
            "http://localhost:8017"
        );
        assert_eq!(
            configured_instance(Some("https://files.company.test/".into())),
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
        assert!(matches!(cli.command, Some(Command::Resume { id }) if id == "job-1"));
        assert!(Cli::try_parse_from(["beam", "--memory-limit-mib", "32", "transfers"]).is_err());
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
