use std::{
    env, fs,
    io::{self, Write},
    process,
    sync::mpsc,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use filebeam_client_core::{
    control::{Control, TransferSettings},
    protocol::{self, UploadAuthentication, UploadOptions, UploadRecipient},
    services::{
        AccountKeyUpload, CreatedNote, NoteCreate, ServiceClient, export_self_key,
        generate_self_keypair, import_self_key, open_recipient_key, validate_self_key,
    },
    uploads::DirectoryMode,
};
use sha2::{Digest, Sha256};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().context(
        "usage: services-acceptance {all|notes|account|live|serve-live|create-note|open-note}",
    )?;
    let instance =
        env::var("FILEBEAM_ACCEPTANCE_INSTANCE").unwrap_or_else(|_| "http://127.0.0.1:8019".into());
    match command.as_str() {
        "all" => all(&instance),
        "notes" => native_hosted_note(&instance),
        "account" => account_inbox_round_trip(&instance),
        "live" => live_notes(&instance),
        "serve-live" => serve_live(
            &instance,
            &args.next().context("serve-live requires text")?,
            args.next().as_deref(),
            args.next().as_deref() == Some("burn"),
        ),
        "create-note" => {
            let text = args.next().context("create-note requires text")?;
            let password = args.next();
            let burn = args.next().as_deref() == Some("burn");
            let note = ServiceClient::new(&instance)?.notes().create(note_request(
                &text,
                password.as_deref(),
                burn,
            ))?;
            println!("{}", note.link);
            Ok(())
        }
        "open-note" => {
            let link = args.next().context("open-note requires link")?;
            let password = args.next();
            println!(
                "{}",
                ServiceClient::new(&instance)?
                    .notes()
                    .open(&link, password.as_deref())?
                    .text
            );
            Ok(())
        }
        _ => bail!(
            "usage: services-acceptance {{all|notes|account|live|serve-live|create-note|open-note}}"
        ),
    }
}

fn all(instance: &str) -> Result<()> {
    native_hosted_note(instance)?;
    account_inbox_round_trip(instance)
}

fn native_hosted_note(instance: &str) -> Result<()> {
    let sender = ServiceClient::new(instance)?;
    let created = sender.notes().create(note_request(
        "native hosted burn note",
        Some("note-password"),
        true,
    ))?;
    let receiver = ServiceClient::new(instance)?;
    let opened = receiver
        .notes()
        .open(&created.link, Some("note-password"))?;
    ensure!(
        opened.text == "native hosted burn note" && opened.consumed,
        "native hosted note did not decrypt and burn"
    );
    ensure!(
        ServiceClient::new(instance)?
            .notes()
            .open(&created.link, Some("note-password"))
            .is_err(),
        "burned note remained readable"
    );
    println!("native-hosted-password-burn-note=passed");
    Ok(())
}

fn live_notes(instance: &str) -> Result<()> {
    let (link, control, sender) = start_live(instance, "native first claimant", None, true)?;
    let first_link = link.clone();
    let first_instance = instance.to_owned();
    let first = thread::spawn(move || {
        ServiceClient::new(&first_instance)?
            .notes()
            .open(&first_link, None)
    });
    thread::sleep(Duration::from_millis(300));
    let denied = ServiceClient::new(instance)?.notes().open(&link, None);
    let opened = first
        .join()
        .map_err(|_| anyhow::anyhow!("first live receiver panicked"))??;
    ensure!(
        opened.text == "native first claimant" && opened.consumed,
        "first live claimant did not read and burn"
    );
    ensure!(
        denied.is_err_and(|error| error.to_string().contains("409")),
        "second live burn claimant was not rejected with a conflict"
    );
    stop_live(control, sender)?;

    let (link, control, sender) = start_live(instance, "nonburn reread", None, false)?;
    ensure!(
        ServiceClient::new(instance)?
            .notes()
            .open(&link, None)?
            .text
            == "nonburn reread",
        "first non-burn live read failed"
    );
    ensure!(
        ServiceClient::new(instance)?
            .notes()
            .open(&link, None)?
            .text
            == "nonburn reread",
        "non-burn live note was not rereadable"
    );
    stop_live(control, sender)?;

    let (link, control, sender) = start_live(
        instance,
        "password survives rejection",
        Some("live-password"),
        true,
    )?;
    ensure!(
        ServiceClient::new(instance)?
            .notes()
            .open(&link, Some("wrong-password"))
            .is_err(),
        "incorrect live password decrypted a note"
    );
    ensure!(
        ServiceClient::new(instance)?
            .notes()
            .open(&link, Some("live-password"))?
            .consumed,
        "incorrect password prematurely burned the live note"
    );
    stop_live(control, sender)?;
    println!(
        "live-single-claim=passed\nlive-nonburn-reread=passed\nlive-wrong-password-does-not-burn=passed"
    );
    Ok(())
}

fn serve_live(instance: &str, text: &str, password: Option<&str>, burn: bool) -> Result<()> {
    let (link, control, sender) = start_live(instance, text, password, burn)?;
    println!("{link}");
    io::stdout().flush()?;
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    stop_live(control, sender)
}

fn start_live(
    instance: &str,
    text: &str,
    password: Option<&str>,
    burn_on_read: bool,
) -> Result<(String, Control, thread::JoinHandle<Result<CreatedNote>>)> {
    let (prompts, prompt_receiver) = mpsc::channel();
    let (events, receiver) = mpsc::channel();
    let control = Control::with_events(live_settings(), prompts, events);
    thread::spawn(move || {
        while let Ok(prompt) = prompt_receiver.recv() {
            let _ = prompt.reply.send("yes".to_owned().into());
        }
    });
    let worker = control.clone();
    let instance = instance.to_owned();
    let request = note_request(text, password, burn_on_read);
    let sender = thread::spawn(move || {
        ServiceClient::new(&instance)?.notes().create_live(
            request,
            &worker,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .context("live sender did not publish a share link")?;
        if let filebeam_client_core::control::TransferEvent::ShareReady(ready) = receiver
            .recv_timeout(remaining)
            .context("live sender did not publish a share link")?
        {
            return Ok((ready.share_url, control, sender));
        }
    }
}

fn stop_live(control: Control, sender: thread::JoinHandle<Result<CreatedNote>>) -> Result<()> {
    control.cancel();
    let _ = sender
        .join()
        .map_err(|_| anyhow::anyhow!("live sender panicked"))?;
    Ok(())
}

fn account_inbox_round_trip(instance: &str) -> Result<()> {
    let suffix = format!("{}{}", process::id(), nonce());
    let password = "acceptance-password-123";
    let sender_email = format!("sender-{suffix}@example.test");
    let receiver_email = format!("receiver-{suffix}@example.test");
    let sender = ServiceClient::new(instance)?;
    sender.account().register(
        &format!("send{suffix}"),
        Some("Acceptance sender"),
        &sender_email,
        password,
    )?;
    let sender = ServiceClient::new(instance)?;
    ensure!(
        sender.account().login(&sender_email, password, true)?.email == sender_email,
        "native login did not restore the registered sender"
    );
    let receiver = ServiceClient::new(instance)?;
    receiver.account().register(
        &format!("recv{suffix}"),
        Some("Acceptance receiver"),
        &receiver_email,
        password,
    )?;
    let material = generate_self_keypair()?;
    let exported = export_self_key(&material.private_key)?;
    let imported = import_self_key(&exported)?;
    ensure!(
        imported.as_slice() == material.private_key.as_slice(),
        "fbsk1 import/export changed the private key"
    );
    let bundle = receiver.account().upload_key(&AccountKeyUpload {
        public_key: material.public_key.clone(),
        fingerprint: material.fingerprint.clone(),
        custody_mode: "self".into(),
        encrypted_private_key: None,
        current_password: None,
        replace: false,
    })?;
    validate_self_key(&imported, &bundle.public_key)?;
    let recipient = sender.account().recipient(&format!("recv{suffix}"))?;
    ensure!(
        recipient.account_key_bundle_id == bundle.id,
        "recipient lookup returned the wrong key bundle"
    );
    let cookie = sender.cookie_context()?;
    let root = env::var("FILEBEAM_ACCEPTANCE_RESULTS")
        .unwrap_or_else(|_| "/tmp/filebeam-services-acceptance".into());
    let state = format!("{root}/state-{suffix}");
    let output = format!("{root}/downloads-{suffix}");
    fs::create_dir_all(&state)?;
    fs::create_dir_all(&output)?;
    let source = format!("{root}/inbox-{suffix}.txt");
    fs::write(&source, b"native sender to native recipient inbox\n")?;
    protocol::upload(
        instance,
        &[source.clone().into()],
        DirectoryMode::Individual,
        UploadOptions {
            authentication: UploadAuthentication::SessionCookie(cookie),
            recipient: Some(UploadRecipient {
                username: recipient.username,
                user_id: recipient.id,
                account_key_bundle_id: recipient.account_key_bundle_id,
                public_key: recipient.public_key,
            }),
            ..Default::default()
        },
        &control(&state),
    )?;
    let transfer = receiver
        .account()
        .inbox()?
        .first()
        .cloned()
        .context("recipient inbox has no completed transfer")?;
    let opened = receiver.account().open_inbox(&transfer.id, &imported)?;
    let working_key = open_recipient_key(
        &imported,
        &receiver
            .account()
            .inbox_metadata(&transfer.id)?
            .recipient_key,
        &transfer.id,
    )?;
    ensure!(
        opened.key_bundle_id == bundle.id && opened.filenames == [format!("inbox-{suffix}.txt")],
        "inbox metadata did not decrypt with the uploaded key"
    );
    let paths = protocol::download_inbox(
        instance,
        &opened.transfer_id,
        &working_key,
        &receiver.cookie_context()?,
        output.as_ref(),
        &control(&state),
    )?;
    ensure!(
        paths.len() == 1 && digest(&source)? == digest(&paths[0])?,
        "recipient download hash differs from sender source"
    );
    ensure!(
        protocol::download_inbox(
            instance,
            &opened.transfer_id,
            &working_key,
            &sender.cookie_context()?,
            output.as_ref(),
            &control(&state),
        )
        .is_err(),
        "a non-recipient account read inbox ciphertext"
    );
    receiver.account().logout()?;
    ensure!(
        receiver.account().session().is_err(),
        "native logout retained the receiver session"
    );
    sender.account().logout()?;
    println!("native-hosted-note=passed\naccount-inbox-decrypt-download-hash=passed");
    Ok(())
}

fn note_request(text: &str, password: Option<&str>, burn_on_read: bool) -> NoteCreate {
    NoteCreate {
        text: text.into(),
        title: Some("Acceptance note".into()),
        language: "plain".into(),
        password: password.map(str::to_owned),
        burn_on_read,
        retention_hours: Some(1),
    }
}

fn control(state_home: &str) -> Control {
    Control::new(
        TransferSettings {
            state_home: state_home.into(),
            max_concurrency: Some(2),
            memory_budget: 256 * 1024 * 1024,
            client_user_agent: Some("filebeam-services-acceptance".into()),
            webrtc_relay_only: false,
            checkpoint_secret_store: None,
            source_resolver: None,
        },
        mpsc::channel().0,
    )
}

fn live_settings() -> TransferSettings {
    TransferSettings {
        state_home: env::var("FILEBEAM_ACCEPTANCE_RESULTS")
            .unwrap_or_else(|_| "/tmp/filebeam-services-acceptance".into())
            .into(),
        max_concurrency: Some(2),
        memory_budget: 256 * 1024 * 1024,
        client_user_agent: Some("filebeam-services-acceptance".into()),
        webrtc_relay_only: false,
        checkpoint_secret_store: None,
        source_resolver: None,
    }
}

fn digest(path: impl AsRef<std::path::Path>) -> Result<String> {
    Ok(Sha256::digest(fs::read(path)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        % 1_000_000_000
}
