use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use ed25519_dalek::SigningKey;

use crate::{
    DaemonPeerReceiver, DeviceJob, DeviceProvision, daemon_store, provision_signing_key,
    registry_scope, registry_store, unix_now_i64,
};

const LAN_ATTEMPT_ID: &str = "lan-v1";

/// Owns the opt-in listener independently from IPC clients. Advertisements are
/// retained only while policy allows LAN receiving.
pub fn start(
    home: PathBuf,
    runtime: filebeam_client_core::DeviceRuntime,
    receive_enabled: Arc<AtomicBool>,
    lan_enabled: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            if !lan_enabled.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            let result = filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
                run_listener(&home, &runtime, &receive_enabled, &lan_enabled, &stop),
            );
            if result.is_err() {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    })
}

async fn run_listener(
    home: &std::path::Path,
    runtime: &filebeam_client_core::DeviceRuntime,
    receive_enabled: &Arc<AtomicBool>,
    lan_enabled: &Arc<AtomicBool>,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let listener =
        filebeam_transfer_native::lan::LanListener::bind("0.0.0.0:0".parse::<SocketAddr>()?)
            .await?;
    let _advertisement = listener.advertise()?;
    while lan_enabled.load(Ordering::Relaxed) && !stop.load(Ordering::Acquire) {
        let setup =
            match tokio::time::timeout(Duration::from_millis(250), listener.accept_setup()).await {
                Ok(Ok(setup)) => setup,
                Ok(Err(_)) | Err(_) => continue,
            };
        if !receive_enabled.load(Ordering::Relaxed) {
            setup.peer.close().await;
            continue;
        }
        let Some(provision) =
            daemon_store(home)?.load_named::<DeviceProvision>("device-provision-v1.json")?
        else {
            setup.peer.close().await;
            continue;
        };
        let scope = registry_scope(&provision)?;
        let peer = match filebeam_client_core::device_registry::DeviceId::parse(setup.route_id) {
            Ok(peer) => peer,
            Err(_) => {
                setup.peer.close().await;
                continue;
            }
        };
        // The native session retains the canonical cache; do not hold an
        // encrypted Store while waiting for a peer handshake.
        let session = match (|| -> Result<_> {
            let store = registry_store(home)?;
            let registry = filebeam_client_core::device_registry::DeviceRegistry::open(
                &store,
                scope.clone(),
                Duration::from_secs(7 * 24 * 60 * 60),
            )?;
            registry.peer_session(
                &scope,
                &peer,
                filebeam_client_core::device_registry::RegistryRoute::OfflineLan,
                unix_now_i64(),
            )
        })() {
            Ok(session) => session,
            Err(_) => {
                setup.peer.close().await;
                continue;
            }
        };
        let signer = provision_signing_key(&provision)?;
        match session
            .authenticate(
                setup.peer,
                setup.channels,
                Arc::new(signer.clone()),
                LAN_ATTEMPT_ID,
            )
            .await
        {
            Ok(authenticated) => {
                runtime.start_peer_receiver(
                    authenticated,
                    Arc::new(DaemonPeerReceiver::with_policy(
                        home.to_path_buf(),
                        receive_enabled.clone(),
                    )),
                    Arc::new(signer),
                );
            }
            Err(error) => eprintln!("filebeamd: LAN device connection: {error:#}"),
        }
    }
    Ok(())
}

pub fn try_send(
    runtime: &filebeam_client_core::DeviceRuntime,
    registry: &filebeam_client_core::device_registry::DeviceRegistry<'_>,
    scope: &filebeam_client_core::device_registry::RegistryScope,
    peer: &filebeam_client_core::device_registry::DeviceId,
    signer: &SigningKey,
    home: &std::path::Path,
    job: &DeviceJob,
) -> Result<Option<filebeam_client_core::JobObservation>> {
    let local = registry.local_device()?;
    let endpoints = match filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
        filebeam_transfer_native::lan::discover(Duration::from_secs(2)),
    ) {
        Ok(endpoints) => endpoints,
        Err(_) => return Ok(None),
    };
    for endpoint in endpoints {
        let setup = match filebeam_transfer_native::runtime::shared_tokio_runtime().block_on(
            filebeam_transfer_native::lan::connect_setup(endpoint.address, local.id()),
        ) {
            Ok(setup) => setup,
            Err(_) => continue,
        };
        let session = match registry.peer_session(
            scope,
            peer,
            filebeam_client_core::device_registry::RegistryRoute::OfflineLan,
            unix_now_i64(),
        ) {
            Ok(session) => session,
            Err(_) => return Ok(None),
        };
        let authenticated = match filebeam_transfer_native::runtime::shared_tokio_runtime()
            .block_on(session.authenticate(
                setup.peer,
                setup.channels,
                Arc::new(signer.clone()),
                LAN_ATTEMPT_ID,
            )) {
            Ok(authenticated) => authenticated,
            Err(_) => continue,
        };
        let store = filebeam_transfer_native::checkpoint::Store::open(
            &home.join("transfers"),
            &job.id.to_string(),
        )?;
        let prepared = filebeam_transfer_native::protocol::peer::load_peer_job(&store)?;
        return Ok(Some(runtime.start_peer_sender(
            authenticated,
            prepared,
            job.grant_id.clone(),
            None,
            job.expires_at_unix_seconds,
        )));
    }
    Ok(None)
}
