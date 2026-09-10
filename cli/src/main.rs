mod app;
mod config;
mod inline;
mod input;
mod output;
mod presentation;
mod protocol;
mod terminal;
mod tui;
mod update;
mod uploads;

use std::{env, io::IsTerminal, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

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
        /// Combine the selection into one ZIP archive before encrypting.
        #[arg(long, conflicts_with = "individual")]
        zip: bool,
        /// Recursively send regular files; each counts against the file limit.
        #[arg(long, conflicts_with = "zip")]
        individual: bool,
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
            zip,
            individual,
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
            for link in inline::run(
                &config,
                &instance,
                app::Request::Upload(files, mode),
                cli.plain,
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
            )? {
                output::result(&value, cli.plain)?;
            }
        }
        Some(Command::Cancel { id }) => {
            protocol::discard_transfer(&config.home.join("transfers"), &id)?
        }
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
    use super::{Cli, Command, configured_instance};
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
        let cli =
            Cli::try_parse_from(["beam", "--max-concurrency", "4", "resume", "job-1"]).unwrap();
        assert_eq!(cli.max_concurrency, Some(4));
        assert!(matches!(cli.command, Some(Command::Resume { id }) if id == "job-1"));
        assert!(Cli::try_parse_from(["beam", "--memory-limit-mib", "32", "transfers"]).is_err());
    }
}
