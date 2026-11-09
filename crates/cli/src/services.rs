use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use filebeam_client_core::services::{
    AccountKeyUpload, NoteCreate, ServiceClient, export_self_key, generate_self_keypair,
    import_self_key, validate_self_key, wrap_password_key,
};
use filebeam_transfer_native::checkpoint::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::config::Config;

pub fn contacts(
    config: &Config,
    instance: &str,
    command: crate::ContactsCommand,
    plain: bool,
) -> Result<()> {
    use crate::ContactsCommand as Command;
    let client = client(config, instance)?;
    let account = client.account();
    let (contacts, requests_only) = match command {
        Command::List => (account.contacts()?, false),
        Command::Requests => (account.contacts()?, true),
        Command::Set {
            username,
            can_send,
            auto_download,
        } => {
            let current = account
                .contacts()?
                .contacts
                .into_iter()
                .find(|contact| contact.username == username.trim_start_matches('@'))
                .context("accepted friend was not found")?;
            (
                account.contact_action(
                    &username,
                    "preferences",
                    can_send.map_or(current.can_send, |value| value.value()),
                    auto_download.map_or(current.auto_download, |value| value.value()),
                )?,
                false,
            )
        }
        other => {
            let (username, action) = match other {
                Command::Request { username } => (username, "request"),
                Command::Accept { username } => (username, "accept"),
                Command::Decline { username } => (username, "decline"),
                Command::Cancel { username } => (username, "cancel"),
                Command::Remove { username } => (username, "remove"),
                Command::Block { username } => (username, "block"),
                Command::Unblock { username } => (username, "unblock"),
                _ => unreachable!(),
            };
            (
                account.contact_action(&username, action, None, None)?,
                false,
            )
        }
    };
    for contact in contacts.contacts {
        if requests_only && contact.status == "accepted" {
            continue;
        }
        crate::output::result(
            &format!(
                "@{}\t{}\tcan-send={}\tauto-download={}",
                contact.username,
                contact.status,
                contact.effective.can_send,
                contact.effective.auto_download
            ),
            plain,
        )?;
    }
    if !requests_only {
        for contact in contacts.blocked {
            crate::output::result(&format!("@{}\tblocked", contact.username), plain)?;
        }
    }
    Ok(())
}

pub(crate) fn receiver(
    config: &Config,
    instance: &str,
    account_id: u64,
) -> Result<filebeam_client_core::services::inbox_receiver::InboxReceiver> {
    let root = config.home.join("transfers/inbox-staging");
    filebeam_client_core::services::inbox_receiver::InboxReceiver::open(
        &root,
        instance,
        account_id,
        filebeam_transfer_native::checkpoint::FilesystemSecretStore::for_state_root(&root),
    )
}
pub(crate) fn receiver_control(config: &Config) -> filebeam_client_core::control::Control {
    let (sender, _prompts) = std::sync::mpsc::channel();
    filebeam_client_core::control::Control::new(
        filebeam_client_core::control::TransferSettings {
            state_home: config.home.join("transfers"),
            memory_budget: config.memory_limit_mib * 1024 * 1024,
            max_concurrency: config.max_concurrency,
            client_user_agent: None,
            webrtc_relay_only: true,
            checkpoint_secret_store: None,
            source_resolver: None,
        },
        sender,
    )
}
pub fn watch_inbox(config: &Config, instance: &str, once: bool) -> Result<()> {
    use std::sync::atomic::Ordering;
    let client = client(config, instance)?;
    let account_id = client.account().session()?.id;
    let control = receiver_control(config);
    let _interrupt = crate::terminal::watch_interrupt(control.cancelled.clone())?;
    {
        receiver(config, instance, account_id)?.set_enabled(true)?;
    }
    while !control.cancelled.load(Ordering::Relaxed) {
        let state = receiver(config, instance, account_id)?.sweep(&client, &control);
        match state {
            Ok(state) => {
                if once {
                    println!("{}", serde_json::to_string(&state)?);
                    return Ok(());
                }
            }
            Err(error) if once => return Err(error),
            Err(error) if !control.cancelled.load(Ordering::Relaxed) => {
                eprintln!("Inbox receiving paused: {error}")
            }
            Err(_) => break,
        }
        for _ in 0..60 {
            if control.cancelled.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    Ok(())
}
pub fn staged_inbox(
    config: &Config,
    instance: &str,
    id: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    let receiver = receiver(config, instance, cached_account_id(config, instance)?)?;
    if let (Some(id), Some(output)) = (id, output) {
        let key = stored_private_key(config, instance)?;
        for path in receiver.export(id, &key, output, &receiver_control(config))? {
            println!("{path}");
        }
    } else {
        for entry in receiver.state()?.entries {
            println!("{}\t{}\t{}", entry.id, entry.state, entry.bytes);
        }
    }
    Ok(())
}
pub fn dismiss_staged(config: &Config, instance: &str, id: &str) -> Result<()> {
    receiver(config, instance, cached_account_id(config, instance)?)?.dismiss(id)
}

#[derive(Serialize, Deserialize)]
struct Session {
    origin: String,
    cookies: String,
    #[serde(default)]
    account_id: u64,
}

#[derive(Serialize, Deserialize)]
struct PrivateKey {
    value: String,
}

#[derive(Serialize, Deserialize)]
struct PendingDeviceEnrollment {
    id: String,
    display_name: String,
    public_key: String,
    signing_key: String,
    deployment: String,
    account_id: u64,
    generation: u64,
    challenge: String,
    #[serde(default)]
    approval: Option<filebeam_daemon::DeviceApproval>,
}

impl PendingDeviceEnrollment {
    fn provision(self, instance: &str, credential: String) -> filebeam_daemon::DeviceProvision {
        filebeam_daemon::DeviceProvision {
            instance: instance.into(),
            deployment: self.deployment,
            account_id: self.account_id,
            generation: self.generation,
            device_id: self.id,
            key_version: 1,
            public_key: self.public_key,
            signing_key: self.signing_key,
            credential,
        }
    }

    fn awaiting(&self) -> Result<DeviceAdd> {
        Ok(DeviceAdd::Pending {
            id: self.id.clone(),
            fingerprint: device_fingerprint(&self.public_key)?,
            approval: self.approval.clone(),
        })
    }
}

pub enum DeviceAdd {
    /// Waiting for an active device to approve this one. `approval` is the
    /// payload to pass to `beam devices approve` on that device.
    Pending {
        id: String,
        fingerprint: String,
        approval: Option<filebeam_daemon::DeviceApproval>,
    },
    Provision(filebeam_daemon::DeviceProvision),
}

fn session_id(instance: &str) -> String {
    format!(
        "account-{}",
        hex::encode(Sha256::digest(instance.as_bytes()))[..24].to_owned()
    )
}

fn session_store(config: &Config, instance: &str, create: bool) -> Result<Option<Store>> {
    let root = config.home.join("accounts");
    let id = session_id(instance);
    if create {
        Ok(Some(Store::create(&root, &id)?))
    } else if root.join(&id).exists() {
        Ok(Some(Store::open(&root, &id)?))
    } else {
        Ok(None)
    }
}

pub fn client(config: &Config, instance: &str) -> Result<ServiceClient> {
    let session = match session_store(config, instance, false)? {
        Some(store) => store.load_named::<Session>("session")?,
        None => None,
    };
    let cookies = session
        .filter(|session| session.origin == instance)
        .map(|session| session.cookies);
    ServiceClient::new_with_cookie_context(instance, cookies.as_deref())
}

pub fn history_row(entry: &filebeam_client_core::services::HistoryEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        entry.id,
        entry.kind,
        entry.driver,
        entry.status,
        entry.item_count,
        entry.ciphertext_bytes.max(entry.declared_ciphertext_bytes),
        entry.expires_at
    )
}

pub fn save_session(config: &Config, instance: &str, client: &ServiceClient) -> Result<()> {
    let account_id = client.account().session()?.id;
    let cookies = client
        .cookie_context()
        .context("login did not return a session cookie")?;
    let store = match session_store(config, instance, false)? {
        Some(store) => store,
        None => session_store(config, instance, true)?.expect("create requested a store"),
    };
    // A session is encrypted and authenticated by the existing owner-only filesystem store.
    store.save_named(
        "session",
        &Session {
            origin: instance.into(),
            cookies,
            account_id,
        },
    )
}

pub fn clear_session(config: &Config, instance: &str) -> Result<()> {
    let root = config.home.join("accounts");
    let id = session_id(instance);
    if root.join(&id).exists() {
        Store::discard(&root, &id)?;
    }
    Ok(())
}

pub fn begin_or_resume_device_add(
    config: &Config,
    instance: &str,
    display_name: &str,
) -> Result<DeviceAdd> {
    let client = client(config, instance)?;
    client
        .account()
        .session()
        .context("sign in with `beam account login` before adding a device")?;
    let store = session_store(config, instance, true)?.expect("create requested a store");
    let existing = store.load_named::<PendingDeviceEnrollment>("pending-device-enrollment")?;
    if let Some(pending) = existing.as_ref() {
        let key = signing_key(&pending.signing_key)?;
        let challenge = device_challenge(pending);
        let redemption = client.devices().account_device_complete_enrollment(
            &challenge,
            &filebeam_client_core::services::enrollment_signature(
                &key,
                &pending.id,
                pending.generation,
                &pending.challenge,
            )?,
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        match redemption {
            Ok(redemption) => {
                let pending = existing.expect("pending enrollment was loaded");
                return Ok(DeviceAdd::Provision(
                    pending.provision(instance, redemption.credential),
                ));
            }
            Err(_)
                if pending
                    .approval
                    .as_ref()
                    .is_some_and(|approval| approval.expires_at_unix > now) =>
            {
                return pending.awaiting();
            }
            // The enrollment window closed before approval. Request a fresh
            // challenge for the same device key below.
            Err(_) => {}
        }
    }
    let (id, seed, display_name) = match existing {
        Some(pending) => {
            let seed: [u8; 32] = STANDARD
                .decode(&pending.signing_key)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("pending device signing key is invalid"))?;
            (pending.id, seed, pending.display_name)
        }
        None => {
            let mut seed = [0_u8; 32];
            seed[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
            seed[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
            (ulid::Ulid::new().to_string(), seed, display_name.to_owned())
        }
    };
    let key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let public_key = STANDARD.encode(key.verifying_key().to_bytes());
    let challenge = client.devices().account_device_challenge(
        &id,
        &display_name,
        device_platform(),
        &public_key,
        1,
        &["peer-transfer-v1".into()],
    )?;
    let context = &challenge.approval_context;
    let pending = PendingDeviceEnrollment {
        id: id.clone(),
        display_name,
        public_key: public_key.clone(),
        signing_key: STANDARD.encode(seed),
        deployment: context.deployment_id.clone(),
        account_id: context.account_id,
        generation: challenge.device.generation,
        challenge: challenge.challenge.clone(),
        approval: Some(filebeam_daemon::DeviceApproval {
            instance_origin: context.instance_origin.clone(),
            deployment_id: context.deployment_id.clone(),
            account_id: context.account_id,
            generation: context.generation,
            candidate_device_id: context.candidate_device_id.clone(),
            candidate_key_version: context.candidate_key_version,
            candidate_public_key: public_key.clone(),
            challenge_sha256: context.challenge_sha256.clone(),
            expires_at_unix: context.expires_at_unix,
        }),
    };
    // Persist before enrollment: a second device may need to approve this candidate.
    store.save_named("pending-device-enrollment", &pending)?;
    let signature = filebeam_client_core::services::enrollment_signature(
        &key,
        &pending.id,
        pending.generation,
        &pending.challenge,
    )?;
    let enrolled = client
        .devices()
        .account_device_enroll(&challenge, &signature)?;
    match enrolled.credential {
        Some(credential) => Ok(DeviceAdd::Provision(
            pending.provision(instance, credential),
        )),
        None => pending.awaiting(),
    }
}

pub fn finish_device_add(config: &Config, instance: &str) -> Result<()> {
    let store = session_store(config, instance, false)?
        .context("pending device enrollment state was not found")?;
    store.remove_named("pending-device-enrollment")
}

fn device_challenge(
    pending: &PendingDeviceEnrollment,
) -> filebeam_client_core::services::DeviceEnrollmentChallenge {
    filebeam_client_core::services::DeviceEnrollmentChallenge {
        device: filebeam_client_core::services::AccountDevice {
            id: pending.id.clone(),
            display_name: pending.display_name.clone(),
            platform: device_platform().into(),
            public_key: pending.public_key.clone(),
            key_version: 1,
            status: "pending".into(),
            generation: pending.generation,
            last_seen_at: None,
            presence_expires_at: None,
            revoked_at: None,
        },
        challenge: pending.challenge.clone(),
        expires_at: None,
        approval_context: filebeam_client_core::services::DeviceApprovalContext {
            instance_origin: String::new(),
            deployment_id: pending.deployment.clone(),
            account_id: pending.account_id,
            generation: pending.generation,
            candidate_device_id: pending.id.clone(),
            candidate_key_version: 1,
            candidate_public_key: pending.public_key.clone(),
            challenge_sha256: String::new(),
            expires_at_unix: 0,
            approver_device_id: String::new(),
            approver_key_version: 0,
        },
    }
}

fn signing_key(value: &str) -> Result<ed25519_dalek::SigningKey> {
    let bytes: [u8; 32] = STANDARD
        .decode(value)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("pending device signing key is invalid"))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&bytes))
}

pub fn device_fingerprint(public_key: &str) -> Result<String> {
    Ok(hex::encode(sha2::Sha256::digest(
        STANDARD.decode(public_key)?,
    )))
}

fn device_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub(crate) fn cached_account_id(config: &Config, instance: &str) -> Result<u64> {
    let cached = session_store(config, instance, false)?
        .and_then(|store| store.load_named::<Session>("session").transpose())
        .transpose()?;
    if let Some(session) =
        cached.filter(|session| session.origin == instance && session.account_id != 0)
    {
        return Ok(session.account_id);
    }
    client(config, instance)?
        .account()
        .session()
        .map(|session| session.id)
}

pub fn stored_private_key(config: &Config, instance: &str) -> Result<Zeroizing<Vec<u8>>> {
    let store = session_store(config, instance, false)?
        .context("no local custody key; run beam account key-setup or key-import")?;
    let key = store
        .load_named::<PrivateKey>("private-key")?
        .context("no local custody key; run beam account key-setup or key-import")?;
    import_self_key(&key.value)
}

fn save_private_key(config: &Config, instance: &str, private_key: &[u8]) -> Result<()> {
    let store = match session_store(config, instance, false)? {
        Some(store) => store,
        None => session_store(config, instance, true)?.expect("create requested a store"),
    };
    store.save_named(
        "private-key",
        &PrivateKey {
            value: export_self_key(private_key)?,
        },
    )
}

pub fn setup_self_key(config: &Config, instance: &str, replace: bool) -> Result<()> {
    let client = client(config, instance)?;
    let session = client.account().session()?;
    let material = generate_self_keypair()?;
    client.account().upload_key(&AccountKeyUpload {
        public_key: material.public_key,
        fingerprint: material.fingerprint,
        custody_mode: "self".into(),
        encrypted_private_key: None,
        current_password: None,
        replace,
    })?;
    save_private_key(config, instance, &material.private_key)?;
    ensure!(session.id > 0, "invalid account session");
    Ok(())
}

/// Creates a recoverable custody key. The password only wraps the private key;
/// it is never retained in the local account store.
pub fn setup_password_key(
    config: &Config,
    instance: &str,
    password: &str,
    replace: bool,
) -> Result<()> {
    let client = client(config, instance)?;
    let session = client.account().session()?;
    let material = generate_self_keypair()?;
    let encrypted_private_key = wrap_password_key(
        &material.private_key,
        password.as_bytes(),
        session.id,
        &material.public_key,
    )?;
    client.account().upload_key(&AccountKeyUpload {
        public_key: material.public_key,
        fingerprint: material.fingerprint,
        custody_mode: "password".into(),
        encrypted_private_key: Some(encrypted_private_key),
        current_password: Some(password.into()),
        replace,
    })?;
    Ok(())
}

pub fn import_self_key_for_account(
    config: &Config,
    instance: &str,
    value: &str,
    replace: bool,
) -> Result<()> {
    let key = import_self_key(value)?;
    let client = client(config, instance)?;
    let bundle = client
        .account()
        .account_keys()?
        .into_iter()
        .find(|key| key.is_active)
        .context("account has no active key to validate")?;
    validate_self_key(&key, &bundle.public_key)?;
    client.account().upload_key(&AccountKeyUpload {
        public_key: bundle.public_key,
        fingerprint: bundle.fingerprint,
        custody_mode: "self".into(),
        encrypted_private_key: None,
        current_password: None,
        replace,
    })?;
    save_private_key(config, instance, &key)
}

pub fn export_stored_key(config: &Config, instance: &str) -> Result<String> {
    export_self_key(&stored_private_key(config, instance)?)
}

pub struct NoteRequest<'a> {
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub password_file: Option<&'a Path>,
    pub password_stdin: bool,
    pub password: bool,
    pub burn_on_read: bool,
    pub retention_hours: Option<u64>,
    pub separate_key: bool,
}

pub fn secret(
    password_file: Option<&Path>,
    password_stdin: bool,
    label: &str,
) -> Result<Zeroizing<String>> {
    ensure!(
        !(password_file.is_some() && password_stdin),
        "choose either --password-file or --password-stdin"
    );
    let value = if let Some(path) = password_file {
        private_file(path)?
    } else if password_stdin {
        let mut value = String::new();
        io::stdin().read_to_string(&mut value)?;
        value
    } else {
        prompt_secret(label)?
    };
    let value = value.trim_end_matches(['\r', '\n']).to_owned();
    ensure!(!value.is_empty(), "{label} cannot be empty");
    Ok(Zeroizing::new(value))
}

pub fn note_create(
    config: &Config,
    instance: &str,
    request: NoteRequest<'_>,
) -> Result<Vec<String>> {
    let password = request
        .password
        .then(|| {
            secret(
                request.password_file,
                request.password_stdin,
                "Note password",
            )
        })
        .transpose()?;
    let created = client(config, instance)?.notes().create(NoteCreate {
        text: request.text,
        title: request.title,
        language: request.language,
        password: password.as_deref().map(|value| value.as_str().to_owned()),
        burn_on_read: request.burn_on_read,
        retention_hours: request.retention_hours,
    })?;
    if !request.separate_key {
        return Ok(vec![created.link]);
    }
    let (link, key) = created
        .link
        .split_once('#')
        .context("created note link has no key")?;
    Ok(vec![link.into(), key.to_owned()])
}

pub fn note_open(
    config: &Config,
    instance: &str,
    link: &str,
    password_file: Option<&Path>,
    password_stdin: bool,
    password: bool,
) -> Result<Vec<String>> {
    let password = password
        .then(|| secret(password_file, password_stdin, "Note password"))
        .transpose()?;
    let note = client(config, instance)?
        .notes()
        .open(link, password.as_deref().map(|value| value.as_str()))?;
    let mut values = Vec::new();
    if let Some(title) = note.title {
        values.push(format!("title: {title}"));
    }
    values.push(format!("language: {}", note.language));
    values.push(note.text);
    if note.consumed {
        values.push("burned after this read".into());
    }
    Ok(values)
}

fn private_file(path: &Path) -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)?.permissions().mode();
        ensure!(
            mode & 0o077 == 0,
            "password file must be owner-only (chmod 600 {})",
            path.display()
        );
    }
    fs::read_to_string(path).with_context(|| format!("read password file {}", path.display()))
}

pub fn private_text_file(path: &Path) -> Result<String> {
    bounded_text(private_file(path)?, "key file")
}

pub const MAX_NOTE_TEXT_BYTES: usize = 64 * 1024;

pub fn bounded_text(value: String, label: &str) -> Result<String> {
    ensure!(
        value.len() <= MAX_NOTE_TEXT_BYTES,
        "{label} must be at most {} KiB",
        MAX_NOTE_TEXT_BYTES / 1024
    );
    Ok(value)
}

pub fn read_note_text(input: impl Read, label: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(MAX_NOTE_TEXT_BYTES.min(4096));
    input
        .take((MAX_NOTE_TEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {label}"))?;
    ensure!(
        bytes.len() <= MAX_NOTE_TEXT_BYTES,
        "note text must be at most 64 KiB"
    );
    String::from_utf8(bytes).context("note text must be valid UTF-8")
}

fn prompt_secret(label: &str) -> Result<String> {
    if !io::stdin().is_terminal() {
        bail!("{label} required; use --password-stdin or --password-file");
    }
    eprint!("{label}: ");
    io::stderr().flush()?;
    enable_raw_mode()?;
    struct Raw;
    impl Drop for Raw {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
        }
    }
    let _raw = Raw;
    let mut value = String::new();
    loop {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => break,
                KeyCode::Esc => bail!("secret entry cancelled"),
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(character) => value.push(character),
                _ => {}
            }
        }
    }
    eprintln!();
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_input_is_utf8_and_bounded_to_64_kib() {
        assert_eq!(
            read_note_text(&b"native note"[..], "note").unwrap(),
            "native note"
        );
        assert!(read_note_text(vec![b'x'; MAX_NOTE_TEXT_BYTES + 1].as_slice(), "note").is_err());
        assert!(read_note_text(&[0xff][..], "note").is_err());
    }
}
