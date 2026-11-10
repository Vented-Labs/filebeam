//! Explicit desktop entry point for the shared signed daemon component.
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::time::Duration;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use anyhow::{Context, Result, bail};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use filebeam_client_config::Config;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use filebeam_client_updater::{
    HttpFetcher, Options, Product, Updater, user_service::UserDaemonService,
};

/// Installs the signed shared component and enables the existing per-user service.
/// Desktop callers must invoke this only from an explicit background-receiving action.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn enable(config: &Config, version: &str) -> Result<String> {
    use filebeam_daemon::{IPC_VERSION, Request};

    let key = option_env!("FILEBEAM_RELEASE_PUBLIC_KEY")
        .filter(|key| !key.is_empty())
        .context("daemon service setup is blocked: this build has no embedded release key")?;
    let updater = Updater::new(Options::for_product(
        config.home.clone(),
        Product::Desktop,
        version,
        key,
    )?);
    let request = |request: Request| -> Result<()> {
        let reply = filebeam_daemon::request(&config.home, request)?;
        reply.error.map_or(Ok(()), |error| bail!(error))
    };
    let service = UserDaemonService::new(
        config.home.clone(),
        || {
            request(Request::Status {
                version: IPC_VERSION,
            })
        },
        || {
            request(Request::Shutdown {
                version: IPC_VERSION,
            })
        },
    );
    updater
        .install_daemon_component_with(
            &HttpFetcher::new(Duration::from_secs(30)),
            "desktop",
            IPC_VERSION,
            filebeam_daemon::STORAGE_SCHEMA,
            &service,
        )
        .context("could not set up the local device service")
}
