use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
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

#[derive(Serialize, Deserialize)]
struct Session {
    origin: String,
    cookies: String,
}

#[derive(Serialize, Deserialize)]
struct PrivateKey {
    value: String,
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

pub fn save_session(config: &Config, instance: &str, client: &ServiceClient) -> Result<()> {
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
