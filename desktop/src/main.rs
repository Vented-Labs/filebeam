use std::{env, ffi::OsString, path::PathBuf, process::ExitCode, sync::Arc};

use anyhow::{Context, Result, bail};
use filebeam_client_config::Config;
use filebeam_client_updater::{Options, Product, Updater};
use filebeam_desktop::{
    APP_NAME, VERSION, app,
    client::DesktopClient,
    platform::ingress::{self, Instance},
};

fn main() -> ExitCode {
    // GPUI reports graphics backend and presentation failures through `log`.
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(
            "warn,filebeam_desktop=info,gpui::platform::linux::x11::client=info,blade_graphics::hal::init=info",
        ),
    )
    .format_timestamp_millis()
    .try_init();
    let mut args = env::args_os();
    let _program = args.next();
    let arguments: Vec<OsString> = args.collect();
    match run(arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{APP_NAME} could not start: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<OsString>) -> Result<()> {
    log::info!("starting {APP_NAME} {VERSION}");
    if handle_updater_worker(&arguments)? {
        return Ok(());
    }
    let (home, external, short_lived) = parse_arguments(arguments)?;
    let config = Config::load(home).context("load desktop configuration")?;
    if let Some(output) = short_lived {
        schedule_update(&config);
        print_short_lived(output);
        return Ok(());
    }
    let origin = url::Url::parse(&config.server.url).context("configured server URL is invalid")?;
    if external
        .iter()
        .any(|value| value.contains('\n') || value.contains('\r') || value.contains('\0'))
    {
        bail!("external request is invalid");
    }
    let _external = external
        .iter()
        .map(|value| ingress::parse(value, &origin))
        .collect::<Result<Vec<_>>>()?;
    let instance = ingress::acquire_inputs(&config.home, external.clone())?;
    if matches!(instance, Instance::Forwarded) {
        return Ok(());
    }
    let activation = activate_update(&config).unwrap_or_else(|error| {
        log::warn!("could not activate desktop update: {error}");
        filebeam_client_updater::Activation::None
    });
    if activation != filebeam_client_updater::Activation::None {
        drop(instance);
        #[cfg(not(windows))]
        {
            let options = Options::for_product(config.home.clone(), Product::Desktop, VERSION, "")?;
            std::process::Command::new(options.executable)
                .args(env::args_os().skip(1))
                .spawn()
                .context("relaunch updated Filebeam")?;
        }
        return Ok(());
    }
    schedule_update(&config);
    let client = Arc::new(DesktopClient::new(Some(config.home.clone()))?);
    let Instance::Owner(owner) = instance else {
        unreachable!()
    };
    app::run_with_instance(client, config.home, external, Some(owner)).map_err(anyhow::Error::msg)
}

#[derive(Clone, Copy)]
enum ShortLived {
    Help,
    Version,
}

fn parse_arguments(
    arguments: Vec<OsString>,
) -> Result<(Option<PathBuf>, Vec<String>, Option<ShortLived>)> {
    let mut home = None;
    let mut external = Vec::new();
    let mut short_lived = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let argument = argument
            .into_string()
            .map_err(|_| anyhow::anyhow!("arguments must be valid UTF-8"))?;
        if argument == "--home" {
            let value = arguments
                .next()
                .context("--home requires an absolute path")?
                .into_string()
                .map_err(|_| anyhow::anyhow!("--home must be valid UTF-8"))?;
            let value = PathBuf::from(value);
            if !value.is_absolute() {
                bail!("--home must be absolute");
            }
            home = Some(value);
        } else if let Some(value) = argument.strip_prefix("--home=") {
            let value = PathBuf::from(value);
            if !value.is_absolute() {
                bail!("--home must be absolute");
            }
            home = Some(value);
        } else if argument == "--help" || argument == "-h" {
            short_lived = Some(ShortLived::Help);
        } else if argument == "--version" || argument == "-V" {
            short_lived = Some(ShortLived::Version);
        } else if argument.starts_with('-') {
            bail!("unknown option: {argument}");
        } else {
            external.push(argument);
        }
    }
    Ok((home, external, short_lived))
}

fn updater(config: &Config) -> Result<Updater> {
    let key = option_env!("FILEBEAM_RELEASE_PUBLIC_KEY")
        .filter(|key| !key.is_empty())
        .context("this development build has no embedded release key")?;
    Ok(Updater::new(Options::for_product(
        config.home.clone(),
        Product::Desktop,
        VERSION,
        key,
    )?))
}

fn activate_update(config: &Config) -> Result<filebeam_client_updater::Activation> {
    if config.updates.auto_update {
        // Unsigned local builds deliberately have no updater rather than failing startup.
        if let Ok(updater) = updater(config) {
            return updater.activate_staged(|| {
                Config::load(Some(config.home.clone()))
                    .map(|fresh| fresh.updates.auto_update)
                    .unwrap_or(false)
            });
        }
    }
    Ok(filebeam_client_updater::Activation::None)
}

fn print_short_lived(output: ShortLived) {
    match output {
        ShortLived::Version => println!("{APP_NAME} {VERSION}"),
        ShortLived::Help => println!(
            "{APP_NAME} {VERSION}\n\nUsage: filebeam [OPTIONS] [PATH|LINK]...\n\nOptions:\n      --home <PATH> Use a Filebeam home\n  -h, --help        Print help\n  -V, --version     Print version"
        ),
    }
}
fn schedule_update(config: &Config) {
    if config.updates.auto_update {
        let _ = updater(config).and_then(|updater| updater.launch(true));
    }
}

fn handle_updater_worker(arguments: &[OsString]) -> Result<bool> {
    let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
        return Ok(false);
    };
    if command != "--filebeam-update-worker" && command != "--filebeam-apply-staged" {
        return Ok(false);
    }
    if arguments.len() < 3 || arguments[1].to_str() != Some("desktop") {
        bail!("invalid desktop update worker arguments");
    }
    let home = PathBuf::from(arguments[2].clone());
    if !home.is_absolute() {
        bail!("desktop update worker home must be absolute");
    }
    let config = Config::load(Some(home))?;
    if command == "--filebeam-update-worker" {
        if arguments.len() != 4 {
            bail!("desktop update worker is missing executable");
        }
        let key = option_env!("FILEBEAM_RELEASE_PUBLIC_KEY").context("build has no release key")?;
        let mut options =
            Options::for_product(config.home.clone(), Product::Desktop, VERSION, key)?;
        options.executable = PathBuf::from(&arguments[3]);
        Updater::new(options).bootstrap(|| {
            Config::load(Some(config.home.clone()))
                .map(|fresh| fresh.updates.auto_update)
                .unwrap_or(false)
        })?;
    } else {
        #[cfg(windows)]
        {
            if config.updates.auto_update {
                updater(&config)?.apply_windows_helper(&arguments[3..])?;
            }
        }
        #[cfg(not(windows))]
        {
            bail!("Windows update helper is unsupported on this platform");
        }
    }
    Ok(true)
}
