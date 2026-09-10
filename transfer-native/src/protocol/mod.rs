use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_encryption::{
    decrypt_chunk, decrypt_manifest, derive_item_key, derive_password_key,
    derive_password_protected_key,
};
#[cfg(test)]
use filebeam_encryption::{encrypt_manifest, generate_nonce_prefix};
use filebeam_transfer::{AEAD_TAG_BYTES as TAG_BYTES, MAX_CHUNKS, MAX_CIPHERTEXT_BYTES};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    checkpoint::Store,
    control::{Control, Phase, SecretKind},
    uploads::DirectoryMode,
};

mod download;
mod upload;

#[derive(Clone, Debug)]
pub struct SavedTransfer {
    pub id: String,
    pub direction: String,
    pub state: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Deserialize)]
struct SavedHeader {
    version: u8,
    id: String,
    direction: String,
    state: String,
    done: u64,
    total: u64,
}

impl SavedHeader {
    fn summary(self, directory_id: &str) -> Result<SavedTransfer> {
        if self.version != 1
            || self.id != directory_id
            || Uuid::parse_str(&self.id).is_err()
            || !matches!(self.direction.as_str(), "upload" | "download")
            || self.state.is_empty()
            || self.state.len() > 64
            || !self
                .state
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || self.done > self.total
        {
            bail!("invalid saved transfer checkpoint");
        }
        Ok(SavedTransfer {
            id: self.id,
            direction: self.direction,
            state: self.state,
            done: self.done,
            total: self.total,
        })
    }
}

pub fn saved_transfers(home: &Path) -> Result<Vec<SavedTransfer>> {
    if !home.exists() {
        return Ok(Vec::new());
    }
    let mut transfers = Vec::new();
    for entry in fs::read_dir(home)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let Ok(store) = Store::open(home, &id) else {
            continue;
        };
        // A corrupt or obsolete job must not hide valid saved transfers.
        let summary = store
            .load::<SavedHeader>()
            .and_then(|job| job.context("saved transfer checkpoint is empty"))
            .and_then(|job| job.summary(&id));
        if let Ok(summary) = summary {
            transfers.push(summary);
        }
    }
    transfers.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(transfers)
}

pub fn discard_transfer(home: &Path, id: &str) -> Result<()> {
    let id = Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let path = home.join(&id);
    if !path.exists() {
        return Ok(());
    }
    // Acquiring the Store lock rejects an active transfer and validates that the
    // directory is private rather than a symlink. Rename before releasing the
    // lock so a new opener cannot race deletion of this job.
    let store = Store::open(home, &id)?;
    let discarded = home.join(format!(".{id}.discarding-{}", Uuid::new_v4()));
    fs::rename(&path, &discarded).with_context(|| format!("discard {}", path.display()))?;
    drop(store);
    fs::remove_dir_all(&discarded).with_context(|| format!("discard {}", discarded.display()))?;
    Ok(())
}

pub fn resume(id: &str, control: &Control) -> Result<Vec<String>> {
    let id = Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let store = Store::open(&control.transfer_home(), &id)?;
    let header = store
        .load::<SavedHeader>()?
        .context("saved transfer checkpoint is empty")?;
    let header = header.summary(&id)?;
    match header.direction.as_str() {
        "upload" => upload::resume(store, control),
        "download" => download::resume(store, control),
        _ => unreachable!("validated checkpoint direction"),
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct TransferCapabilities {
    #[serde(default)]
    pub upload_status: bool,
    #[serde(default)]
    pub download_ranges: bool,
}

#[derive(Clone, Deserialize)]
pub struct Info {
    pub name: String,
    pub file_retention_hours: u64,
    pub anonymous_uploads_enabled: bool,
    #[serde(default)]
    pub enabled_drivers: Vec<String>,
    pub maximum_transfer_bytes: Option<u64>,
    pub maximum_file_count: Option<usize>,
}

impl Info {
    pub fn fixture() -> Self {
        Self {
            name: "Filebeam".into(),
            file_retention_hours: 24,
            anonymous_uploads_enabled: true,
            enabled_drivers: vec!["http".into()],
            maximum_transfer_bytes: Some(2 * 1024 * 1024 * 1024),
            maximum_file_count: Some(20),
        }
    }
}

pub fn instance_info(instance: &str) -> Result<Info> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(3))
        .user_agent(concat!(
            "filebeam-transfer-native/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;
    let response = client.get(format!("{instance}/api/v1/info")).send()?;
    if !response.status().is_success() {
        bail!(
            "server returned {} for {}",
            response.status(),
            response.url()
        );
    }
    Ok(response.json::<Api<Info>>()?.data)
}

#[derive(Deserialize)]
struct Api<T> {
    data: T,
}

#[derive(Clone, Deserialize)]
struct MetadataItem {
    id: String,
    position: u64,
    chunk_count: u64,
}

#[derive(Deserialize)]
struct Transfer {
    id: String,
    protocol_version: u8,
    chunk_bytes: u64,
    encrypted_manifest: Option<String>,
    #[serde(default)]
    items: Vec<MetadataItem>,
    #[serde(default)]
    download_concurrency: Option<u32>,
    #[serde(default)]
    transfer_capabilities: TransferCapabilities,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Manifest {
    version: u8,
    items: Vec<ManifestItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ManifestItem {
    id: String,
    name: String,
    #[serde(rename = "type")]
    mime: String,
    size: u64,
    nonce_prefix: String,
    chunk_count: u64,
    digest: DigestValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DigestValue {
    algorithm: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u8,
    nonce_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    salt: Option<String>,
    ciphertext: String,
}

pub fn upload(
    instance: &str,
    paths: &[PathBuf],
    mode: DirectoryMode,
    control: &Control,
) -> Result<String> {
    upload::run(instance, paths, mode, control)
}

pub fn download(
    instance: &str,
    raw: &str,
    output: &Path,
    control: &Control,
) -> Result<Vec<PathBuf>> {
    download::run(instance, raw, output, control)
}

#[derive(Debug, PartialEq)]
pub struct Link {
    pub id: String,
    pub key: Option<Vec<u8>>,
    pub instance: String,
}

#[cfg(test)]
pub fn parse_link(input: &str) -> Result<Link> {
    parse_link_for_instance(input, "https://filebeam.io")
}

pub fn parse_link_for_instance(input: &str, instance: &str) -> Result<Link> {
    let mut value = input
        .trim()
        .trim_matches(|c| matches!(c, '\'' | '"' | '`' | '<' | '>' | '[' | ']'));
    if let Some((_, markdown)) = value.rsplit_once("](") {
        value = markdown.trim().trim_end_matches(')').trim();
    }
    value = value.trim_matches(|c| matches!(c, '\'' | '"' | '`' | '<' | '>'));
    let explicit_url = if value.starts_with("http://") || value.starts_with("https://") {
        Some(Url::parse(value).context("transfer link is invalid")?)
    } else {
        None
    };
    let configured = match &explicit_url {
        Some(url) => Url::parse(&url.origin().ascii_serialization())?,
        None => Url::parse(instance).context("configured Filebeam instance is invalid")?,
    };
    if !matches!(configured.scheme(), "http" | "https")
        || configured.host_str().is_none()
        || configured.username() != ""
        || configured.password().is_some()
        || configured.query().is_some()
        || configured.fragment().is_some()
        || configured.path() != "/"
    {
        bail!("configured Filebeam instance must be an HTTP origin");
    }
    let authority = match configured.port() {
        Some(port) => format!("{}:{port}", configured.host_str().unwrap()),
        None => configured.host_str().unwrap().to_owned(),
    };
    let candidate_url = if explicit_url.is_some() {
        explicit_url
    } else if value
        .get(..authority.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&authority))
        && value.as_bytes().get(authority.len()) == Some(&b'/')
    {
        Some(
            Url::parse(&format!("{}://{value}", configured.scheme()))
                .context("transfer link is invalid")?,
        )
    } else {
        None
    };
    let (id_candidate, fragment) = if let Some(url) = candidate_url {
        if url.username() != "" || url.password().is_some() || url.query().is_some() {
            bail!("transfer links must not contain URL credentials or a query");
        }
        let segments = url
            .path_segments()
            .context("transfer link has an invalid path")?
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        if segments.len() != 1 {
            bail!("transfer link has an invalid path");
        }
        (segments[0].to_owned(), url.fragment().map(str::to_owned))
    } else {
        let (id, fragment) = value
            .split_once('#')
            .map_or((value, None), |(id, fragment)| (id, Some(fragment)));
        if id.contains('/') || id.contains('?') || id.is_empty() {
            bail!("transfer link has an invalid path");
        }
        (id.to_owned(), fragment.map(str::to_owned))
    };
    let id = id_candidate.to_ascii_uppercase();
    if is_uuid(&id_candidate) {
        bail!("UUIDs are not Filebeam transfer IDs; provide a 26-character ULID");
    }
    if !is_ulid(&id) {
        bail!("transfer ID must be a canonical 26-character ULID");
    }
    let key = match fragment.as_deref() {
        None => None,
        Some(value) => {
            let key = value
                .strip_prefix("k=")
                .unwrap_or(value)
                .strip_prefix("v1.")
                .context("share key must use v1.<base64url>")?;
            let key = decode(key)?;
            if key.len() != 32 {
                bail!("share key must decode to 32 bytes");
            }
            Some(key)
        }
    };
    Ok(Link {
        id,
        key,
        instance: configured.origin().ascii_serialization(),
    })
}

fn is_ulid(value: &str) -> bool {
    value.len() == 26
        && matches!(value.as_bytes()[0], b'0'..=b'7')
        && value.bytes().skip(1).all(|c| matches!(c, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .enumerate()
            .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) && c == b'-' || c.is_ascii_hexdigit())
}

pub fn safe_filename(value: &str) -> String {
    let mut name: String = value
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    while name.ends_with(['.', ' ']) {
        name.pop();
    }
    if name.is_empty() || name == "." || name == ".." || is_windows_device_name(&name) {
        "download".into()
    } else {
        name
    }
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn collision_free(directory: &Path, name: &str) -> PathBuf {
    let candidate = directory.join(name);
    if !candidate.exists() {
        return candidate;
    }
    for i in 1.. {
        let candidate = directory.join(format!("{name} ({i})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_manifest(manifest: &Manifest, transfer: &Transfer) -> Result<()> {
    if manifest.version != 1
        || manifest.items.is_empty()
        || manifest.items.len() != transfer.items.len()
    {
        bail!("manifest does not match transfer");
    }
    if transfer.chunk_bytes == 0 || transfer.chunk_bytes > MAX_CIPHERTEXT_BYTES - TAG_BYTES {
        bail!("transfer has an invalid chunk size");
    }
    let mut server_ids = HashSet::new();
    let mut server_positions = HashSet::new();
    for server in &transfer.items {
        if server.id.is_empty()
            || !server_ids.insert(&server.id)
            || !server_positions.insert(server.position)
            || server.position as usize >= transfer.items.len()
            || server.chunk_count == 0
            || server.chunk_count > MAX_CHUNKS
        {
            bail!("transfer has invalid item metadata");
        }
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for item in &manifest.items {
        let server = transfer
            .items
            .iter()
            .find(|server| server.id == item.id)
            .context("unknown item")?;
        let count = item
            .size
            .checked_add(transfer.chunk_bytes.saturating_sub(1))
            .context("manifest chunk count overflow")?
            / transfer.chunk_bytes;
        if item.id.is_empty()
            || item.name.is_empty()
            || !ids.insert(&item.id)
            || !names.insert(safe_filename(&item.name))
            || !valid_digest(&item.digest.value)
            || item.digest.algorithm != "sha256"
            || item.chunk_count == 0
            || item.chunk_count > MAX_CHUNKS
            || item.chunk_count != count.max(1)
            || server.chunk_count != item.chunk_count
            || decode(&item.nonce_prefix)?.len() != 16
        {
            bail!("manifest item is invalid");
        }
    }
    Ok(())
}

fn aad(transfer: &str, item: &str, index: &str) -> String {
    format!("filebeam:v1:{transfer}:{item}:{index}")
}

#[cfg(test)]
fn encode(value: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(value)
}

fn decode(value: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(value)
        .context("invalid base64url data")
}

fn decode_share_key(value: &str) -> Result<Vec<u8>> {
    let decoded = decode(value.trim().strip_prefix("v1.").unwrap_or(value.trim()))?;
    if decoded.len() != 32 {
        bail!("share key must decode to 32 bytes");
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> String {
        encode(&[1; 32])
    }

    #[test]
    fn parser_accepts_wrappers_scheme_and_lowercase() {
        for value in [
            format!(
                "'https://filebeam.io/01arz3ndektsv4rrffq69g5fav/#k=v1.{}'",
                key()
            ),
            format!("`filebeam.io/01arz3ndektsv4rrffq69g5fav#k=v1.{}`", key()),
            format!("<01arz3ndektsv4rrffq69g5fav#v1.{}>", key()),
            format!(
                "[download](https://filebeam.io/01arz3ndektsv4rrffq69g5fav#k=v1.{})",
                key()
            ),
        ] {
            assert_eq!(parse_link(&value).unwrap().id, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        }
    }

    #[test]
    fn parser_rejects_bad_input() {
        assert!(
            parse_link("550e8400-e29b-41d4-a716-446655440000")
                .unwrap_err()
                .to_string()
                .contains("UUID")
        );
        assert!(parse_link("https://filebeam.io/01ARZ3NDEKTSV4RRFFQ69G5FAV?x=1").is_err());
        assert!(parse_link("https://user:pass@host.test/01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
        assert!(parse_link("01ARZ3NDEKTSV4RRFFQ69G5FAV#k=nope").is_err());
    }

    #[test]
    fn full_links_select_their_origin_and_ids_use_the_configured_instance() {
        let raw = "http://localhost:8000/01M23GEFKC2WNXBASCS675XJRD#k=v1.KGGOo1frIIN3t1kAgQ2STIKjGXOFPKiPBATHBKDpzxY";
        assert_eq!(
            parse_link_for_instance(raw, "https://filebeam.io")
                .unwrap()
                .instance,
            "http://localhost:8000"
        );
        assert_eq!(
            parse_link_for_instance("01M23GEFKC2WNXBASCS675XJRD", "http://localhost:8000/")
                .unwrap()
                .instance,
            "http://localhost:8000"
        );
    }

    #[test]
    fn filenames_are_portable_without_changing_existing_safe_names() {
        assert_eq!(safe_filename("../../x"), ".._.._x");
        assert_eq!(safe_filename(".."), "download");
        assert_eq!(safe_filename("con.txt"), "download");
        assert_eq!(safe_filename("bad:name"), "bad_name");
    }

    #[test]
    fn manifest_crypto_round_trip() {
        let key = vec![4; 32];
        let prefix = generate_nonce_prefix().unwrap();
        let aad = aad("01ARZ3NDEKTSV4RRFFQ69G5FAV", "manifest", "manifest");
        let ciphertext = encrypt_manifest(&key, &prefix, b"{}", aad.as_bytes()).unwrap();
        assert_eq!(
            decrypt_manifest(&key, &prefix, &ciphertext, aad.as_bytes()).unwrap(),
            b"{}"
        );
    }

    #[test]
    fn manifest_rejects_noncanonical_server_items_and_chunk_overflow() {
        let transfer = Transfer {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            protocol_version: 1,
            chunk_bytes: 10,
            encrypted_manifest: None,
            items: vec![MetadataItem {
                id: "item".into(),
                position: 1,
                chunk_count: 1,
            }],
            download_concurrency: None,
            transfer_capabilities: TransferCapabilities::default(),
        };
        let manifest = Manifest {
            version: 1,
            items: vec![ManifestItem {
                id: "item".into(),
                name: "file".into(),
                mime: "application/octet-stream".into(),
                size: 1,
                nonce_prefix: encode(&[0; 16]),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: "0".repeat(64),
                },
            }],
        };
        assert!(validate_manifest(&manifest, &transfer).is_err());
    }

    #[test]
    fn checkpoint_header_dispatch_is_strict() {
        let id = Uuid::new_v4().to_string();
        let header = SavedHeader {
            version: 1,
            id: id.clone(),
            direction: "download".into(),
            state: "paused".into(),
            done: 2,
            total: 3,
        };
        assert_eq!(header.summary(&id).unwrap().direction, "download");
        let invalid = SavedHeader {
            version: 1,
            id,
            direction: "other".into(),
            state: "paused".into(),
            done: 0,
            total: 0,
        };
        let id = invalid.id.clone();
        assert!(invalid.summary(&id).is_err());
    }

    #[test]
    fn malformed_saved_job_does_not_hide_valid_jobs() {
        let root = tempfile::tempdir().unwrap();
        let good = Uuid::new_v4().to_string();
        let store = Store::create(root.path(), &good).unwrap();
        store.save(&serde_json::json!({"version":1,"id":good,"direction":"upload","state":"paused","done":1,"total":2})).unwrap();
        drop(store);
        let bad = Uuid::new_v4().to_string();
        let store = Store::create(root.path(), &bad).unwrap();
        store.save(&serde_json::json!({"version":9,"id":bad,"direction":"upload","state":"paused","done":1,"total":2})).unwrap();
        drop(store);
        assert_eq!(saved_transfers(root.path()).unwrap().len(), 1);
    }

    #[test]
    fn active_transfer_cannot_be_discarded() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4().to_string();
        let _active = Store::create(root.path(), &id).unwrap();
        assert!(discard_transfer(root.path(), &id).is_err());
        assert!(root.path().join(id).exists());
    }
}
