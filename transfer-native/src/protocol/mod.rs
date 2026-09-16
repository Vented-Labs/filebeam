use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_encryption::{
    decrypt_chunk, decrypt_manifest, derive_item_key, derive_password_key,
    derive_password_protected_key,
};
#[cfg(test)]
use filebeam_encryption::{encrypt_manifest, generate_nonce_prefix};
use filebeam_transfer::{
    AEAD_TAG_BYTES as TAG_BYTES,
    capabilities::DriverLimits,
    link::parse_share_link,
    manifest::{Manifest, ManifestServerItem, validate_manifest as validate_portable_manifest},
};
#[cfg(test)]
use filebeam_transfer::{
    capabilities::select_driver_limits,
    manifest::{DigestValue, ManifestItem},
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    control::{Control, Phase, SecretKind},
    source::UploadSource,
    uploads::DirectoryMode,
};

mod download;
mod live;
mod revocation;
mod upload;

pub(crate) const PASSWORD_KDF_BYTES: u64 = 64 * 1024 * 1024;

/// Covers plaintext, AEAD output, base64 output, and JSON serialization buffers
/// held simultaneously while constructing or opening a manifest.
pub(crate) fn manifest_crypto_bytes(plain_bytes: usize) -> Result<u64> {
    u64::try_from(plain_bytes)
        .context("manifest is too large for memory accounting")?
        .checked_mul(4)
        .context("manifest memory accounting overflow")
}

#[cfg(test)]
use crate::checkpoint::Store;

#[derive(Clone, Debug)]
pub struct SavedTransfer {
    pub id: String,
    pub direction: String,
    pub state: String,
    pub done: u64,
    pub total: u64,
}

/// Authenticated checkpoint presentation data. Tokens and encrypted keys are
/// inspected only to determine capabilities and never leave this module.
#[derive(Clone, Debug)]
pub struct SavedTransferDetails {
    pub id: String,
    pub direction: String,
    pub kind: String,
    pub transport: String,
    pub state: String,
    pub done: u64,
    pub total: u64,
    pub verified_privately: bool,
    pub exported: bool,
    pub expires_at: Option<String>,
    pub can_resume: bool,
    pub can_retry_save: bool,
    pub can_end_live: bool,
    pub can_revoke_remote: bool,
    pub can_remove_local: bool,
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
    saved_transfers_with_secret_store(
        home,
        crate::checkpoint::FilesystemSecretStore::for_state_root(home),
    )
}

/// Enumerate records using the same per-client custody provider used to create
/// them. Unauthenticated records are deliberately omitted rather than exposed.
pub fn saved_transfers_with_secret_store(
    home: &Path,
    secret_store: Arc<dyn crate::checkpoint::SecretStore>,
) -> Result<Vec<SavedTransfer>> {
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
        let Ok(store) =
            crate::checkpoint::Store::open_with_secret_store(home, &id, secret_store.clone())
        else {
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

pub fn saved_transfer_details_with_secret_store(
    home: &Path,
    id: &str,
    secret_store: Arc<dyn crate::checkpoint::SecretStore>,
) -> Result<SavedTransferDetails> {
    let id = Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let store = crate::checkpoint::Store::open_with_secret_store(home, &id, secret_store)?;
    let value = store
        .load::<serde_json::Value>()?
        .context("saved transfer checkpoint is empty")?;
    let header: SavedHeader =
        serde_json::from_value(value.clone()).context("invalid saved transfer checkpoint")?;
    let summary = header.summary(&id)?;
    let object = value
        .as_object()
        .context("invalid saved transfer checkpoint")?;
    let text = |name: &str| object.get(name).and_then(serde_json::Value::as_str);
    let has_token = |name: &str| text(name).is_some_and(|value| !value.is_empty());
    let transport = text("driver").unwrap_or("http").to_owned();
    let is_upload = summary.direction == "upload";
    let published = object
        .get("items")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|items| {
            !items.is_empty()
                && items.iter().all(|item| {
                    item.get("published").and_then(serde_json::Value::as_bool) == Some(true)
                })
        });
    let verified = object
        .get("items")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|items| {
            !items.is_empty()
                && items.iter().all(|item| {
                    item.get("verified")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|bits| bits.iter().all(|bit| bit.as_bool() == Some(true)))
                })
        });
    let ended = summary.state == "ended";
    Ok(SavedTransferDetails {
        id: summary.id,
        direction: summary.direction,
        kind: "files".into(),
        transport: transport.clone(),
        state: summary.state.clone(),
        done: summary.done,
        total: summary.total,
        verified_privately: !is_upload && verified && !published,
        exported: !is_upload && published,
        // File checkpoints do not persist server expiry. Do not fabricate one.
        expires_at: None,
        can_resume: matches!(
            summary.state.as_str(),
            "paused" | "running" | "preparing" | "sending" | "receiving"
        ),
        can_retry_save: !is_upload && verified && !published,
        can_end_live: is_upload && transport == "webrtc" && !ended && has_token("upload_token"),
        can_revoke_remote: is_upload && !ended && has_token("delete_token"),
        can_remove_local: true,
    })
}

pub fn discard_transfer(home: &Path, id: &str) -> Result<()> {
    let id = Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let path = home.join(&id);
    if !path.exists() {
        return Ok(());
    }
    crate::checkpoint::Store::discard(home, &id)
}

pub fn resume(id: &str, control: &Control) -> Result<Vec<String>> {
    let id = Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let store = control.open_checkpoint_store(&id)?;
    let header = store
        .load::<SavedHeader>()?
        .context("saved transfer checkpoint is empty")?;
    let header = header.summary(&id)?;
    control.set_checkpoint_id(id);
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
    #[serde(default)]
    pub file_retention_options: Vec<u64>,
    pub anonymous_uploads_enabled: bool,
    #[serde(default)]
    pub enabled_drivers: Vec<String>,
    pub maximum_transfer_bytes: Option<u64>,
    pub maximum_file_count: Option<usize>,
    #[serde(default)]
    pub transport_limits: HashMap<String, DriverLimits>,
}

impl Info {
    pub fn fixture() -> Self {
        Self {
            name: "Filebeam".into(),
            file_retention_hours: 24,
            file_retention_options: vec![24],
            anonymous_uploads_enabled: true,
            enabled_drivers: vec!["http".into()],
            maximum_transfer_bytes: Some(2 * 1024 * 1024 * 1024),
            maximum_file_count: Some(20),
            transport_limits: HashMap::new(),
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
    #[serde(default = "http_driver")]
    driver: String,
    id: String,
    protocol_version: u8,
    chunk_bytes: u64,
    encrypted_manifest: Option<String>,
    #[serde(default)]
    encrypted_descriptor: Option<String>,
    #[serde(default)]
    items: Vec<MetadataItem>,
    #[serde(default)]
    download_concurrency: Option<u32>,
    #[serde(default)]
    transfer_capabilities: TransferCapabilities,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u8,
    nonce_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    salt: Option<String>,
    ciphertext: String,
}

fn http_driver() -> String {
    "http".into()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transport {
    #[default]
    Http,
    WebRtc,
}

#[derive(Clone, Default)]
pub struct UploadOptions {
    pub transport: Transport,
    /// Publish an authenticated early descriptor so receivers can fetch chunks
    /// while this HTTP file upload is still pending.
    pub turbo: bool,
    pub password: bool,
    pub retention_hours: Option<u64>,
    /// Authentication is request-scoped and is never checkpointed.
    pub authentication: UploadAuthentication,
    /// Inbox recipients require HTTP and cannot be password-protected.
    pub recipient: Option<UploadRecipient>,
}

#[derive(Clone, Default)]
pub enum UploadAuthentication {
    #[default]
    Anonymous,
    Bearer(String),
    SessionCookie(String),
}

#[derive(Clone)]
pub struct UploadRecipient {
    pub username: String,
    pub user_id: u64,
    pub account_key_bundle_id: u64,
    pub public_key: String,
}

pub fn upload(
    instance: &str,
    paths: &[PathBuf],
    mode: DirectoryMode,
    options: UploadOptions,
    control: &Control,
) -> Result<String> {
    if options.turbo && (options.transport != Transport::Http || options.recipient.is_some()) {
        bail!("Turbo requires HTTP file uploads without an inbox recipient");
    }
    match options.transport {
        Transport::Http => upload::run(instance, paths, mode, options, control),
        Transport::WebRtc => {
            if !control.webrtc_relay_only() {
                control.request_peer_consent(instance.to_owned())?;
            }
            upload::run_webrtc(instance, paths, mode, options, control)
        }
    }
}

/// Upload provider-backed sources. Their opaque identities, bounds, and
/// mutation tokens are checkpointed; descriptor handles are reopened by the
/// host resolver for each read and are never serialized.
pub fn upload_sources(
    instance: &str,
    sources: &[UploadSource],
    mode: DirectoryMode,
    options: UploadOptions,
    control: &Control,
) -> Result<String> {
    if options.turbo && (options.transport != Transport::Http || options.recipient.is_some()) {
        bail!("Turbo requires HTTP file uploads without an inbox recipient");
    }
    if options.transport == Transport::WebRtc && !control.webrtc_relay_only() {
        control.request_peer_consent(instance.to_owned())?;
    }
    upload::run_sources(instance, sources, mode, options, control)
}

/// Permanently delete an uploaded transfer using its durable delete capability.
pub fn revoke_upload(id: &str, control: &Control) -> Result<()> {
    revocation::run(id, control)
}

/// End a live share while preserving its local recovery checkpoint.
pub fn end_live(id: &str, control: &Control) -> Result<()> {
    live::run(id, control)
}

pub fn download(
    instance: &str,
    raw: &str,
    output: &Path,
    control: &Control,
) -> Result<Vec<PathBuf>> {
    download::run(instance, raw, output, control)
}

/// Download a recipient delivery through the authenticated inbox endpoints.
/// The cookie and opened delivery key are request-scoped and are never saved.
pub fn download_inbox(
    instance: &str,
    transfer_id: &str,
    working_key: &[u8],
    cookie: &str,
    output: &Path,
    control: &Control,
) -> Result<Vec<PathBuf>> {
    download::run_inbox(instance, transfer_id, working_key, cookie, output, control)
}

/// Resume an inbox job after the host has restored a session for its recorded
/// origin and supplied the recipient key again.
pub fn resume_inbox(
    id: &str,
    instance: &str,
    working_key: &[u8],
    cookie: &str,
    control: &Control,
) -> Result<Vec<String>> {
    download::resume_inbox(id, instance, working_key, cookie, control)
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
    let portable = parse_share_link(&format!(
        "{id_candidate}{}",
        fragment.map_or(String::new(), |value| format!("#{value}"))
    ))
    .map_err(anyhow::Error::msg)?;
    let key = portable.key.as_deref().map(decode).transpose()?;
    Ok(Link {
        id: portable.id,
        key,
        instance: configured.origin().ascii_serialization(),
    })
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

fn validate_manifest(manifest: &Manifest, transfer: &Transfer) -> Result<()> {
    let items = transfer
        .items
        .iter()
        .map(|item| ManifestServerItem {
            id: item.id.clone(),
            position: item.position,
            chunk_count: item.chunk_count,
        })
        .collect::<Vec<_>>();
    validate_portable_manifest(manifest, &transfer.driver, transfer.chunk_bytes, &items)
        .map_err(anyhow::Error::msg)
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

    #[test]
    fn live_limits_are_independent_of_legacy_http_limits() {
        let mut advertised = HashMap::new();
        advertised.insert(
            "webrtc".into(),
            DriverLimits {
                maximum_transfer_bytes: Some(8 * 1024 * 1024 * 1024),
                maximum_file_count: None,
            },
        );
        assert_eq!(
            select_driver_limits("webrtc", &advertised, Some(512), Some(2)).maximum_transfer_bytes,
            Some(8 * 1024 * 1024 * 1024)
        );
        assert_eq!(
            select_driver_limits("http", &advertised, Some(512), Some(2)).maximum_transfer_bytes,
            Some(512)
        );
        assert_eq!(
            select_driver_limits("webrtc", &HashMap::new(), Some(512), Some(2))
                .maximum_transfer_bytes,
            None
        );
    }

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
            driver: "http".into(),
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            protocol_version: 1,
            chunk_bytes: 10,
            encrypted_manifest: None,
            encrypted_descriptor: None,
            items: vec![MetadataItem {
                id: "item".into(),
                position: 1,
                chunk_count: 1,
            }],
            download_concurrency: None,
            transfer_capabilities: TransferCapabilities::default(),
        };
        let manifest = Manifest {
            join_token: None,
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
