#![cfg(unix)]

mod support;

use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use filebeam_daemon::{ApprovalMode, DeviceJobState, Request};
use filebeam_transfer_native::{checkpoint::Store, device::load_device_receipt};
use sha2::{Digest, Sha256};

use support::{
    Coordinator, make_private_directory, pin_peer, profile, provision, run_daemon_child, shutdown,
    spawn_daemon_child, wait_for_socket, wait_until,
};

#[test]
fn daemon_child_owner() -> Result<()> {
    if std::env::var_os("FILEBEAM_D2D_CHILD_CONFIG").is_some() {
        run_daemon_child()?;
    }
    Ok(())
}

/// Real daemon IPC, TLS rendezvous, WebRTC, receipt, and commit.
#[test]
fn two_daemons_deliver_prepared_bytes_after_sender_observer_detaches() -> Result<()> {
    let coordinator = Coordinator::start()?;
    let root = tempfile::tempdir()?;
    let sender_home = profile(root.path(), "sender-profile")?;
    let receiver_home = profile(root.path(), "receiver-profile")?;
    let source_root = root.path().join("source");
    let destination = root.path().join("receiver-dropzone");
    make_private_directory(&source_root)?;
    make_private_directory(&destination)?;

    let mut sender = Some(spawn_daemon_child(
        &sender_home,
        coordinator.certificate_pem(),
    )?);
    let receiver = spawn_daemon_child(&receiver_home, coordinator.certificate_pem())?;
    wait_for_socket(&sender_home)?;
    wait_for_socket(&receiver_home)?;

    let result = (|| -> Result<()> {
        let sender_provision = provision(&coordinator, &coordinator.sender)?;
        let receiver_provision = provision(&coordinator, &coordinator.receiver)?;
        request(
            &sender_home,
            Request::ProvisionDevice {
                version: filebeam_daemon::IPC_VERSION,
                provision: sender_provision,
            },
        )?;
        request(
            &receiver_home,
            Request::ProvisionDevice {
                version: filebeam_daemon::IPC_VERSION,
                provision: receiver_provision,
            },
        )?;

        pin_peer(
            &sender_home,
            &coordinator,
            &coordinator.sender,
            &coordinator.receiver,
        )?;
        pin_peer(
            &receiver_home,
            &coordinator,
            &coordinator.receiver,
            &coordinator.sender,
        )?;
        request(
            &receiver_home,
            Request::SetReceiveEnabled {
                version: filebeam_daemon::IPC_VERSION,
                enabled: true,
            },
        )?;
        let grant = request(
            &receiver_home,
            Request::AddDestination {
                version: filebeam_daemon::IPC_VERSION,
                alias: "d2d".into(),
                path: destination.clone(),
                approval: ApprovalMode::Auto,
            },
        )?
        .destinations
        .into_iter()
        .next()
        .context("receiver did not return the destination grant")?;

        let source = source_root.join("d2d-tree");
        fs::create_dir(&source)?;
        fs::create_dir(source.join("empty"))?;
        let bytes = (0..(filebeam_transfer_native::webrtc::FRAME_PAYLOAD_BYTES * 2 + 3))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(source.join("payload.bin"), &bytes)?;
        fs::write(source.join(".hidden"), b"hidden canary")?;
        let job = request(
            &sender_home,
            Request::EnqueueJob {
                version: filebeam_daemon::IPC_VERSION,
                target: coordinator.receiver.device_id.clone(),
                grant_id: grant.id.to_string(),
                sources: vec![source.clone()],
            },
        )?
        .jobs
        .into_iter()
        .next()
        .context("sender did not return the queued job")?;

        wait_until("durable peer preparation", Duration::from_secs(30), || {
            job_state(&sender_home, job.id)
                == Some((DeviceJobState::AwaitingPeerConfiguration, true))
        })
        .with_context(|| {
            format!(
                "sender job after preparation: {:?}",
                job_state(&sender_home, job.id)
            )
        })?;
        fs::remove_dir_all(&source)?;

        // A prepared checkpoint is self-contained: restarting its only owner
        // must not re-enumerate the source that has now been removed.
        shutdown(&sender_home);
        anyhow::ensure!(
            sender.take().expect("sender is present").wait()?.success(),
            "sender did not stop cleanly"
        );
        sender = Some(spawn_daemon_child(
            &sender_home,
            coordinator.certificate_pem(),
        )?);
        wait_for_socket(&sender_home)?;

        // The IPC request ends immediately; the daemon remains the sole job owner.
        wait_until(
            "receiver commit and sender receipt",
            Duration::from_secs(90),
            || {
                matches!(
                    job_state(&sender_home, job.id),
                    Some((DeviceJobState::Delivered | DeviceJobState::Failed, true))
                )
            },
        )
        .with_context(|| format!("sender final state: {:?}", job_state(&sender_home, job.id)))?;
        anyhow::ensure!(
            job_state(&sender_home, job.id) == Some((DeviceJobState::Delivered, true)),
            "sender failed before delivery"
        );
        assert!(!source.exists(), "the prepared source must stay removed");
        let delivered = destination.join("d2d-tree");
        assert_eq!(fs::read(delivered.join("payload.bin"))?, bytes);
        assert_eq!(
            Sha256::digest(fs::read(delivered.join("payload.bin"))?),
            Sha256::digest(bytes)
        );
        assert_eq!(fs::read(delivered.join(".hidden"))?, b"hidden canary");
        assert!(delivered.join("empty").is_dir());

        let store = Store::open(&sender_home.join("transfers"), &job.id.to_string())?;
        let receipt = load_device_receipt(&store)?.context("sender did not persist a receipt")?;
        let receiver_key = provision(&coordinator, &coordinator.receiver)?.public_key()?;
        receipt.verify(&receiver_key)?;
        assert_eq!(receipt.grant_id, grant.id.to_string());
        assert_eq!(receipt.items.len(), 2);
        Ok(())
    })();

    shutdown(&sender_home);
    shutdown(&receiver_home);
    let sender_result = sender.expect("sender is present").wait()?;
    let receiver_result = receiver.wait()?;
    coordinator.stop();
    result.context(format!(
        "sender daemon: {sender_result:?}; receiver daemon: {receiver_result:?}"
    ))
}

fn request(home: &Path, request: Request) -> Result<filebeam_daemon::Reply> {
    let reply = filebeam_daemon::request(home, request)?;
    if let Some(error) = &reply.error {
        anyhow::bail!("daemon request failed: {error}");
    }
    Ok(reply)
}

fn job_state(home: &Path, id: uuid::Uuid) -> Option<(DeviceJobState, bool)> {
    let reply = request(
        home,
        Request::ListJobs {
            version: filebeam_daemon::IPC_VERSION,
        },
    );
    let reply = match reply {
        Ok(reply) => reply,
        Err(_) => return None,
    };
    reply
        .jobs
        .into_iter()
        .find(|job| job.id == id)
        .map(|job| (job.state, job.prepared))
}

trait ProvisionPublicKey {
    fn public_key(&self) -> Result<ed25519_dalek::VerifyingKey>;
}

impl ProvisionPublicKey for filebeam_daemon::DeviceProvision {
    fn public_key(&self) -> Result<ed25519_dalek::VerifyingKey> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let bytes: [u8; 32] = STANDARD
            .decode(&self.public_key)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("receiver public key has an invalid length"))?;
        Ok(ed25519_dalek::VerifyingKey::from_bytes(&bytes)?)
    }
}
