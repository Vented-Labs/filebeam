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
use filebeam_client_core::services::{NoteCreate, ServiceClient};
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
