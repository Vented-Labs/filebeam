//! Headless, real DesktopClient acceptance exercised by scripts/desktop/transfer.test.py.

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use filebeam_desktop::{client::DesktopClient, model::*};
use sha2::{Digest, Sha256};

fn main() -> Result<()> {
    let instance = std::env::args()
        .nth(1)
        .context("fixture instance URL is required")?;
    let root = tempfile::tempdir().context("create private acceptance root")?;
    configure(root.path(), &instance)?;
    let client = DesktopClient::new(Some(root.path().to_owned()))?;
    basic_prepared_receive(&client, root.path()).context("prepared subset")?;
    password_separate_key(&client, root.path()).context("password separate key")?;
    zip_unicode_collision(&client, root.path()).context("ZIP Unicode collision")?;
    if std::env::var_os("FILEBEAM_NATIVE_ACCEPTANCE_PAUSE").is_some() {
        pause_resume(&client, root.path()).context("pause resume")?;
    }
    println!("RESULT native-desktop-client acceptance passed");
    Ok(())
}

fn configure(home: &Path, instance: &str) -> Result<()> {
    fs::write(
        home.join("config.toml"),
        format!(
            "schema_version = 1\n[server]\nurl = {instance:?}\n[updates]\nauto_update = false\n[transfers]\nmemory_limit_mib = 64\nmax_concurrency = 1\n"
        ),
    )?;
    Ok(())
}

fn basic_prepared_receive(client: &DesktopClient, root: &Path) -> Result<()> {
    let source = root.join("alpha.bin");
    let second = root.join("beta.txt");
    fs::write(&source, deterministic(196_731))?;
    fs::write(&second, b"unselected")?;
    let before_send = job_ids(client);
    send(client, vec![source.clone(), second], None, false, true).context("send")?;
    let link = complete_link(client, &before_send).context("complete send")?;
    client.dispatch(ClientCommand::InspectReceive { link })?;
    let preview = wait_snapshot(client, |s| {
        s.receive_previews
            .iter()
            .find(|p| p.operation_id.is_some())
            .cloned()
    })
    .context("prepare receive")?;
    let operation_id = preview.operation_id.context("prepared operation missing")?;
    let selected = preview
        .items
        .iter()
        .find(|item| item.name == "alpha.bin")
        .context("prepared alpha item missing")?;
    client
        .dispatch(ClientCommand::SelectReceiveItems {
            operation_id: operation_id.clone(),
            item_ids: vec![selected.id.clone()],
        })
        .context("observe running")?;
    let before_receive = job_ids(client);
    client.dispatch(ClientCommand::StartPreparedReceive { operation_id })?;
    let received = wait_complete(client, TransferDirection::Receive, &before_receive)
        .context("complete receive")?;
    let export_root = root.join("export-basic");
    fs::create_dir(&export_root)?;
    export(client, &received.id, export_root.clone()).context("export")?;
    if export_root.join("beta.txt").exists() {
        bail!("prepared subset exported an unselected item");
    }
    let exported = exported_match(&source, &export_root).context("verify export")?;
    println!("PASS prepared-subset {}", digest(&exported)?);
    Ok(())
}

fn password_separate_key(client: &DesktopClient, root: &Path) -> Result<()> {
    let source = root.join("password.bin");
    fs::write(&source, deterministic(73_211))?;
    let before_send = job_ids(client);
    send(
        client,
        vec![source.clone()],
        Some("fixture password".into()),
        false,
        false,
    )?;
    answer_secrets(client, "", "fixture password")?;
    let receipt = wait_complete(client, TransferDirection::Send, &before_send)?;
    let link = receipt
        .share_url
        .context("keyless desktop receipt has no link")?;
    let key = receipt
        .separate_key
        .context("keyless desktop receipt has no separate key")?;
    if link.contains('#') || !key.starts_with("v1.") {
        bail!("desktop keyless receipt did not preserve a separate recipient key");
    }
    let before_receive = job_ids(client);
    client
        .dispatch(ClientCommand::ReceiveLink {
            link,
            private_dir: root.join("private-password"),
        })
        .context("observe paused")?;
    answer_secrets(client, &key, "fixture password")?;
    let received = wait_complete(client, TransferDirection::Receive, &before_receive)?;
    let export_root = root.join("export-password");
    fs::create_dir(&export_root)?;
    export(client, &received.id, export_root.clone())?;
    let exported = exported_match(&source, &export_root)?;
    println!("PASS password-separate-key {}", digest(&exported)?);
    Ok(())
}

fn zip_unicode_collision(client: &DesktopClient, root: &Path) -> Result<()> {
    let directory = root.join("unicodé");
    fs::create_dir(&directory)?;
    fs::write(directory.join("δοκιμή.txt"), deterministic(4_097))?;
    let before_send = job_ids(client);
    send(client, vec![directory], None, true, true)?;
    let link = complete_link(client, &before_send)?;
    let before_receive = job_ids(client);
    client.dispatch(ClientCommand::ReceiveLink {
        link,
        private_dir: root.join("private-zip"),
    })?;
    let received = wait_complete(client, TransferDirection::Receive, &before_receive)?;
    let export_root = root.join("export-zip");
    fs::create_dir(&export_root)?;
    fs::write(export_root.join("unicodé.zip"), b"existing")?;
    client.dispatch(ClientCommand::ExportVerified {
        transfer_id: received.id.clone(),
        destination: export_root.clone(),
    })?;
    wait_snapshot_ignore_messages(client, |s| {
        s.verified_results
            .iter()
            .find(|r| r.transfer_id == received.id && r.export_error.is_some())
            .cloned()
    })?;
    let retry_root = root.join("export-zip-retry");
    fs::create_dir(&retry_root)?;
    export(client, &received.id, retry_root.clone())?;
    let zip = fs::read_dir(&retry_root)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|x| x == "zip")
                && fs::metadata(p).map(|m| m.len() > 8).unwrap_or(false)
        })
        .context("collision-safe ZIP export missing")?;
    println!("PASS zip-unicode-collision {}", digest(&zip)?);
    Ok(())
}

fn pause_resume(client: &DesktopClient, root: &Path) -> Result<()> {
    let source = root.join("pause.bin");
    fs::write(&source, deterministic(262_147))?;
    let before_send = job_ids(client);
    send(client, vec![source], None, false, true)?;
    wait_for_pause_fixture()?;
    let running = wait_snapshot_ignore_messages(client, |s| {
        s.jobs
            .iter()
            .find(|j| {
                !before_send.contains(&j.id)
                    && j.direction == TransferDirection::Send
                    && j.state == TransferState::Running
            })
            .cloned()
    })
    .context("observe running")?;
    client.dispatch(ClientCommand::Pause {
        id: running.id.clone(),
    })?;
    wait_snapshot_ignore_messages(client, |s| {
        s.jobs
            .iter()
            .find(|j| j.id == running.id && j.state == TransferState::PauseRequested)
            .cloned()
    })
    .context("observe pause acknowledgement")?;
    if let Some(marker) = std::env::var_os("FILEBEAM_NATIVE_ACCEPTANCE_PAUSE_ACK") {
        fs::write(marker, b"pause-requested")?;
    }
    let paused = wait_snapshot_ignore_messages(client, |s| {
        s.jobs
            .iter()
            .find(|j| j.id == running.id && j.state == TransferState::Paused)
            .cloned()
    })
    .context("observe paused")?;
    client.dispatch(ClientCommand::Resume { id: paused.id })?;
    answer_secrets(client, "", "fixture password")?;
    let resumed = wait_resumed_upload(client, &before_send).context("complete resumed upload")?;
    if resumed.state != TransferState::Complete {
        bail!("resumed upload failed");
    }
    println!("PASS pause-resume-checkpoint");
    Ok(())
}
fn wait_for_pause_fixture() -> Result<()> {
    let Some(marker) = std::env::var_os("FILEBEAM_NATIVE_ACCEPTANCE_PAUSE_READY") else {
        return Ok(());
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !Path::new(&marker).exists() {
        if Instant::now() >= deadline {
            bail!("pause fixture did not block the upload request");
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn wait_resumed_upload(client: &DesktopClient, before: &[String]) -> Result<JobSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let snapshot = client.snapshot();
        if let Some(job) = snapshot.jobs.iter().find(|job| {
            !before.contains(&job.id)
                && matches!(job.state, TransferState::Complete | TransferState::Failed)
        }) {
            return Ok(job.clone());
        }
        if Instant::now() >= deadline {
            let states = snapshot
                .jobs
                .iter()
                .filter(|job| !before.contains(&job.id))
                .map(|job| {
                    format!(
                        "{}:{}:{}",
                        job.id,
                        job.progress.phase,
                        job.error
                            .as_ref()
                            .map(|error| error.detail.as_str())
                            .unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            bail!("resumed upload did not finish ({states})");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn send(
    client: &DesktopClient,
    paths: Vec<PathBuf>,
    password: Option<String>,
    archive: bool,
    include_key: bool,
) -> Result<()> {
    client.dispatch(ClientCommand::SendFiles(SendFiles {
        paths,
        directory_mode: if archive {
            DirectoryMode::Zip
        } else {
            DirectoryMode::Individual
        },
        transport: SendTransport::Http,
        retention_hours: None,
        turbo: false,
        include_key,
        password,
        recipient: None,
    }))
}
fn complete_link(client: &DesktopClient, before: &[String]) -> Result<String> {
    answer_secrets(client, "", "fixture password")?;
    wait_complete(client, TransferDirection::Send, before)?
        .share_url
        .context("completed upload has no link")
}
fn answer_secrets(client: &DesktopClient, key: &str, password: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        for prompt in client.snapshot().pending_prompts {
            match prompt.kind {
                PromptKind::ShareKey if !key.is_empty() => {
                    client.dispatch(ClientCommand::AnswerPrompt {
                        transfer_id: prompt.transfer_id,
                        prompt_id: prompt.id,
                        answer: PromptAnswer::Secret(key.into()),
                    })?
                }
                PromptKind::Password => client.dispatch(ClientCommand::AnswerPrompt {
                    transfer_id: prompt.transfer_id,
                    prompt_id: prompt.id,
                    answer: PromptAnswer::Secret(password.into()),
                })?,
                _ => continue,
            }
        }
        if Instant::now() >= deadline {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
}
fn job_ids(client: &DesktopClient) -> Vec<String> {
    client
        .snapshot()
        .jobs
        .into_iter()
        .map(|job| job.id)
        .collect()
}
fn wait_complete(
    client: &DesktopClient,
    direction: TransferDirection,
    before: &[String],
) -> Result<JobSnapshot> {
    wait_snapshot_ignore_messages(client, |s| {
        s.jobs
            .iter()
            .rev()
            .find(|j| {
                !before.contains(&j.id)
                    && j.direction == direction
                    && j.state == TransferState::Complete
            })
            .cloned()
    })
}
fn export(client: &DesktopClient, id: &str, destination: PathBuf) -> Result<()> {
    let message_cursor = client.snapshot().messages.len();
    wait_snapshot_since(client, message_cursor, |s| {
        s.verified_results
            .iter()
            .find(|r| r.transfer_id == id)
            .cloned()
    })?;
    client.dispatch(ClientCommand::ExportVerified {
        transfer_id: id.into(),
        destination,
    })?;
    wait_snapshot_since(client, message_cursor, |s| {
        s.verified_results
            .iter()
            .find(|r| {
                r.transfer_id == id && !r.exported_paths.is_empty() && r.export_error.is_none()
            })
            .cloned()
    })
    .map(|_| ())
}
fn wait_snapshot<T>(
    client: &DesktopClient,
    get: impl Fn(&DesktopSnapshot) -> Option<T>,
) -> Result<T> {
    wait_snapshot_since(client, 0, get)
}
fn wait_snapshot_since<T>(
    client: &DesktopClient,
    message_cursor: usize,
    get: impl Fn(&DesktopSnapshot) -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let snapshot = client.snapshot();
        if let Some(value) = get(&snapshot) {
            return Ok(value);
        }
        if let Some(error) = snapshot.messages.get(message_cursor) {
            bail!("{}: {}", error.operation, error.error.detail);
        }
        if Instant::now() >= deadline {
            bail!("desktop acceptance timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}
fn wait_snapshot_ignore_messages<T>(
    client: &DesktopClient,
    get: impl Fn(&DesktopSnapshot) -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let snapshot = client.snapshot();
        if let Some(value) = get(&snapshot) {
            return Ok(value);
        }
        if let Some(job) = snapshot
            .jobs
            .iter()
            .find(|job| job.state == TransferState::Failed)
        {
            bail!(
                "desktop transfer failed: {}",
                job.error
                    .as_ref()
                    .map(|error| error.detail.as_str())
                    .unwrap_or("unknown error")
            );
        }
        if Instant::now() >= deadline {
            bail!(
                "desktop acceptance timed out: {}; jobs: {}",
                snapshot
                    .messages
                    .iter()
                    .map(|message| format!("{}: {}", message.operation, message.error.detail))
                    .collect::<Vec<_>>()
                    .join("; "),
                snapshot.jobs.iter().map(|job| format!("{}: {} ({:?})", job.id, job.progress.phase, job.state as u8)).collect::<Vec<_>>().join("; ")
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}
fn deterministic(len: usize) -> Vec<u8> {
    (0..len).map(|n| (n % 251) as u8).collect()
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn verify(source: &Path, destination: &Path) -> Result<()> {
    if fs::read(source)? == fs::read(destination)? {
        Ok(())
    } else {
        bail!("export hash mismatch")
    }
}
fn exported_match(source: &Path, destination: &Path) -> Result<PathBuf> {
    fs::read_dir(destination)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| path.is_file() && verify(source, path).is_ok())
        .context("exported byte hash is missing")
}
