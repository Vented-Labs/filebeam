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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = config::Config::load(cli.home)?;
    config.no_color |= cli.no_color;
    config.reduced_motion |= cli.reduced_motion;
    let instance = instance();
    if !matches!(&cli.command, Some(Command::Update)) {
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
    use super::configured_instance;

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
}
