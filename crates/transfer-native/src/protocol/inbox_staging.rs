//! Keyless, private inbox ciphertext storage. HTTP completion is not verification.

use super::background::{BackgroundHeader, BackgroundWork};
use crate::{
    checkpoint::{SecretStore, Store},
    control::Control,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub driver: String,
    pub protocol_version: u8,
    pub chunk_bytes: u64,
    pub ciphertext_bytes: u64,
    pub expires_at: String,
    pub encrypted_manifest: String,
    pub recipient_key: RecipientKey,
    pub items: Vec<Item>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecipientKey {
    pub bundle: Bundle,
    pub encrypted_key: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub id: u64,
    pub user_id: u64,
    pub public_key: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub position: u64,
    pub chunk_count: u64,
    pub ciphertext_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Staging {
    pub version: u8,
    pub instance: String,
    pub account_id: u64,
    pub metadata: Metadata,
    pub state: String,
    #[serde(default)]
    next_item: usize,
    #[serde(default)]
    next_chunk: u64,
}

pub fn identity(instance: &str, account_id: u64, transfer: &str) -> String {
    hex::encode(Sha256::digest(format!(
        "{instance}\0{account_id}\0{transfer}"
    )))
}

pub fn open(
    root: &Path,
    instance: &str,
    account_id: u64,
    transfer: &str,
    secrets: Arc<dyn SecretStore>,
) -> Result<Store> {
    let id = identity(instance, account_id, transfer);
    if root.join(&id).exists() {
        Store::open_with_secret_store(root, &id, secrets)
    } else {
        Store::create_with_secret_store(root, &id, secrets)
    }
}

pub fn prepare(
    store: &Store,
    instance: &str,
    account_id: u64,
    metadata: Metadata,
) -> Result<Staging> {
    validate(&metadata, account_id)?;
    if store.path().file_name().and_then(|name| name.to_str())
        != Some(identity(instance, account_id, &metadata.id).as_str())
    {
        bail!("inbox staging checkpoint belongs to a different delivery");
    }
    if let Some(saved) = store.load::<Staging>()? {
        if saved.version != 1
            || saved.instance != instance
            || saved.account_id != account_id
            || serde_json::to_vec(&saved.metadata)? != serde_json::to_vec(&metadata)?
        {
            bail!("staged inbox identity or encrypted metadata changed");
        }
        return Ok(saved);
    }
    let saved = Staging {
        version: 1,
        instance: instance.into(),
        account_id,
        metadata,
        state: "downloading".into(),
        next_item: 0,
        next_chunk: 0,
    };
    store.save(&saved)?;
    Ok(saved)
}

fn validate(metadata: &Metadata, account_id: u64) -> Result<()> {
    if metadata.driver != "http"
        || metadata.protocol_version != 1
        || metadata.recipient_key.bundle.user_id != account_id
        || !(1..=24_999_984).contains(&metadata.chunk_bytes)
        || metadata.items.is_empty()
        || metadata.items.len() > 1000
    {
        bail!("unsupported inbox staging metadata");
    }
    validate_id(&metadata.id)?;
    let mut total = 0u64;
    for (position, item) in metadata.items.iter().enumerate() {
        validate_id(&item.id)?;
        let plain = item
            .ciphertext_bytes
            .checked_sub(
                item.chunk_count
                    .checked_mul(16)
                    .context("chunk count overflow")?,
            )
            .context("invalid ciphertext geometry")?;
        if item.position != position as u64
            || item.chunk_count != plain.div_ceil(metadata.chunk_bytes).max(1)
            || item.chunk_count > u32::MAX as u64
        {
            bail!("invalid inbox chunk geometry");
        }
        total = total
            .checked_add(item.ciphertext_bytes)
            .context("inbox size overflow")?;
    }
    if total != metadata.ciphertext_bytes {
        bail!("invalid inbox ciphertext total");
    }
    Ok(())
}

pub fn expected(item: &Item, chunk: u64, chunk_bytes: u64) -> u64 {
    let plain = item.ciphertext_bytes - item.chunk_count * 16;
    plain.saturating_sub(chunk * chunk_bytes).min(chunk_bytes) + 16
}

fn validate_id(value: &str) -> Result<()> {
    let parsed = filebeam_transfer::link::parse_share_link(value).map_err(anyhow::Error::msg)?;
    if parsed.id != value || parsed.key.is_some() {
        bail!("inbox identifiers must be bare ULIDs");
    }
    Ok(())
}

pub fn artifact(store: &Store, item: usize, chunk: u64) -> PathBuf {
    store.path().join(format!("inbox-cipher-{item}-{chunk}"))
}

pub fn pending(store: &Store, cookie: &str, limit: usize) -> Result<Vec<BackgroundWork>> {
    if cookie.is_empty() || cookie.contains(['\r', '\n']) {
        bail!("invalid inbox session credentials");
    }
    let mut saved = store
        .load::<Staging>()?
        .context("inbox staging record is empty")?;
    validate(&saved.metadata, saved.account_id)?;
    if saved.state != "downloading" {
        return Ok(Vec::new());
    }
    reconcile(store, &mut saved)?;
    store.save(&saved)?;
    let mut work = Vec::new();
    for (position, item) in saved
        .metadata
        .items
        .iter()
        .enumerate()
        .skip(saved.next_item)
    {
        let first = if position == saved.next_item {
            saved.next_chunk
        } else {
            0
        };
        for chunk in first..item.chunk_count {
            let path = artifact(store, position, chunk);
            let size = expected(item, chunk, saved.metadata.chunk_bytes);
            if fs::metadata(&path)
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() == size)
            {
                continue;
            }
            work.push(BackgroundWork {
                operation_id: format!(
                    "{}:{position}:{chunk}",
                    identity(&saved.instance, saved.account_id, &saved.metadata.id)
                ),
                transfer_id: saved.metadata.id.clone(),
                method: "GET".into(),
                url: format!(
                    "{}/api/native/v1/inbox/{}/items/{}/chunks/{chunk}?automatic=1",
                    saved.instance, saved.metadata.id, item.id
                ),
                headers: vec![BackgroundHeader {
                    name: "Cookie".into(),
                    value: cookie.into(),
                }],
                body_path: String::new(),
                expected_response_bytes: size,
            });
            if work.len() >= limit.max(1) {
                return Ok(work);
            }
        }
    }
    Ok(work)
}

pub fn ingest(store: &Store, operation: &str, bytes: &[u8]) -> Result<()> {
    let mut saved = store
        .load::<Staging>()?
        .context("inbox staging record is empty")?;
    if saved.state != "downloading" {
        bail!("automatic download is no longer active");
    }
    let parts: Vec<_> = operation.split(':').collect();
    if parts.len() != 3
        || parts[0] != identity(&saved.instance, saved.account_id, &saved.metadata.id)
    {
        bail!("wrong staging operation");
    }
    let position: usize = parts[1].parse()?;
    let chunk: u64 = parts[2].parse()?;
    let item = saved
        .metadata
        .items
        .get(position)
        .context("wrong staging item")?;
    if chunk >= item.chunk_count
        || bytes.len() as u64 != expected(item, chunk, saved.metadata.chunk_bytes)
    {
        bail!("inbox ciphertext length changed");
    }
    let name = format!("inbox-cipher-{position}-{chunk}");
    if !store.path().join(&name).exists() {
        store.persist_immutable(&name, bytes)?;
    }
    reconcile(store, &mut saved)?;
    store.save(&saved)?;
    Ok(())
}

fn reconcile(store: &Store, saved: &mut Staging) -> Result<()> {
    while let Some(item) = saved.metadata.items.get(saved.next_item) {
        if saved.next_chunk >= item.chunk_count {
            saved.next_item += 1;
            saved.next_chunk = 0;
            continue;
        }
        let path = artifact(store, saved.next_item, saved.next_chunk);
        if !fs::symlink_metadata(path).is_ok_and(|metadata| {
            metadata.is_file()
                && metadata.len() == expected(item, saved.next_chunk, saved.metadata.chunk_bytes)
        }) {
            break;
        }
        saved.next_chunk += 1;
    }
    if saved.next_item == saved.metadata.items.len() {
        saved.state = "staged-locked".into();
    }
    Ok(())
}

pub fn fetch(store: &Store, cookie: &str, control: &Control) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(120))
        .build()?;
    loop {
        let work = pending(store, cookie, 1)?;
        let Some(work) = work.first() else {
            return Ok(());
        };
        control.check()?;
        let _memory = control.reserve_memory(work.expected_response_bytes)?;
        if fs2::available_space(store.path())?
            < work
                .expected_response_bytes
                .saturating_add(32 * 1024 * 1024)
        {
            bail!("automatic receiving paused: insufficient free space");
        }
        let response = client.get(&work.url).header("Cookie", cookie).send()?;
        if !response.status().is_success() {
            bail!("automatic inbox download returned {}", response.status());
        }
        let mut bytes = Vec::new();
        response
            .take(work.expected_response_bytes + 1)
            .read_to_end(&mut bytes)?;
        control.check()?;
        ingest(store, &work.operation_id, &bytes)?;
    }
}

pub fn export(
    store: &Store,
    private_key: &[u8],
    output: &Path,
    control: &Control,
) -> Result<Vec<String>> {
    let saved = store
        .load::<Staging>()?
        .context("inbox staging record is empty")?;
    if saved.state != "staged-locked" {
        bail!("inbox ciphertext has not finished staging");
    }
    let bundle = &saved.metadata.recipient_key.bundle;
    let master = zeroize::Zeroizing::new(filebeam_encryption::open_recipient_envelope(
        private_key,
        &super::decode(&saved.metadata.recipient_key.encrypted_key)?,
        format!(
            "filebeam:recipient:v1:{}:{}:{}",
            saved.metadata.id, bundle.user_id, bundle.id
        )
        .as_bytes(),
    )?);
    super::download::export_staged(store, &saved.metadata, &master, output, control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use filebeam_encryption::{
        derive_item_key, encrypt_chunk, encrypt_manifest, generate_account_keypair,
        seal_key_for_recipient,
    };

    fn fixture(root: &Path) -> (Store, Vec<u8>, Vec<Vec<u8>>, Vec<u8>) {
        let instance = "https://files.example";
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let item_id = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
        let bytes = b"Contacts can catch up while the receiving key is locked.".to_vec();
        let master = [7u8; 32];
        let pair = generate_account_keypair().unwrap();
        let prefix = [9u8; 16];
        let item_key = derive_item_key(&master, id, item_id).unwrap();
        let chunks: Vec<_> = bytes
            .chunks(8)
            .enumerate()
            .map(|(index, plain)| {
                encrypt_chunk(
                    &item_key,
                    &prefix,
                    index as u32,
                    plain,
                    format!("filebeam:v1:{id}:{item_id}:{index}").as_bytes(),
                )
                .unwrap()
            })
            .collect();
        let manifest = serde_json::json!({"version": 1, "items": [{"id": item_id, "name": "../offline.txt", "type": "text/plain", "size": bytes.len(), "nonce_prefix": URL_SAFE_NO_PAD.encode(prefix), "chunk_count": chunks.len(), "digest": {"algorithm": "sha256", "value": hex::encode(Sha256::digest(&bytes))}}]});
        let encrypted = encrypt_manifest(
            &master,
            &prefix,
            &serde_json::to_vec(&manifest).unwrap(),
            format!("filebeam:v1:{id}:manifest:manifest").as_bytes(),
        )
        .unwrap();
        let metadata = Metadata { id: id.into(), driver: "http".into(), protocol_version: 1, chunk_bytes: 8,
            ciphertext_bytes: chunks.iter().map(|chunk| chunk.len() as u64).sum(), expires_at: "2000-01-01T00:00:00Z".into(),
            encrypted_manifest: serde_json::json!({"v": 1, "nonce_prefix": URL_SAFE_NO_PAD.encode(prefix), "ciphertext": URL_SAFE_NO_PAD.encode(encrypted)}).to_string(),
            recipient_key: RecipientKey { bundle: Bundle { id: 42, user_id: 23, public_key: URL_SAFE_NO_PAD.encode(&pair[32..]) }, encrypted_key: URL_SAFE_NO_PAD.encode(seal_key_for_recipient(&pair[32..], &master, format!("filebeam:recipient:v1:{id}:23:42").as_bytes()).unwrap()) },
            items: vec![Item { id: item_id.into(), position: 0, chunk_count: chunks.len() as u64, ciphertext_bytes: chunks.iter().map(|chunk| chunk.len() as u64).sum() }] };
        let store = Store::create(root, &identity(instance, 23, id)).unwrap();
        prepare(&store, instance, 23, metadata).unwrap();
        (store, pair[..32].to_vec(), chunks, bytes)
    }

    #[test]
    fn keyless_staging_survives_restart_and_verifies_offline_before_export() {
        let directory = tempfile::tempdir().unwrap();
        let (store, private_key, chunks, plain) = fixture(directory.path());
        let path = store.path().to_path_buf();
        for bytes in chunks {
            let work = pending(&store, "session=secret", 1).unwrap();
            ingest(&store, &work[0].operation_id, &bytes).unwrap();
        }
        assert_eq!(
            store.load::<Staging>().unwrap().unwrap().state,
            "staged-locked"
        );
        assert!(
            !fs::read(path.join("checkpoint.json"))
                .unwrap()
                .windows(plain.len())
                .any(|window| window == plain)
        );
        assert!(!path.join("plain-0").exists());
        let identity = path.file_name().unwrap().to_str().unwrap().to_owned();
        drop(store);
        let store = Store::open(directory.path(), &identity).unwrap();
        let output = directory.path().join("saved");
        let wrong = generate_account_keypair().unwrap();
        assert!(export(&store, &wrong[..32], &output, &Control::test_factory()).is_err());
        assert!(!output.exists());
        let paths = export(&store, &private_key, &output, &Control::test_factory()).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(fs::read(&paths[0]).unwrap(), plain);
        assert!(Path::new(&paths[0]).starts_with(&output));
    }

    #[test]
    fn malformed_operation_or_corrupt_staged_ciphertext_never_publishes_plaintext() {
        let directory = tempfile::tempdir().unwrap();
        let (store, private_key, chunks, _) = fixture(directory.path());
        assert!(ingest(&store, "other:0:0", &chunks[0]).is_err());
        let work = pending(&store, "session=secret", 1).unwrap();
        assert!(ingest(&store, &work[0].operation_id, &[0]).is_err());
        for bytes in chunks {
            let work = pending(&store, "session=secret", 1).unwrap();
            ingest(&store, &work[0].operation_id, &bytes).unwrap();
        }
        let path = artifact(&store, 0, 0);
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 1;
        fs::write(path, bytes).unwrap();
        let output = directory.path().join("saved");
        assert!(export(&store, &private_key, &output, &Control::test_factory()).is_err());
        assert_eq!(fs::read_dir(output).unwrap().count(), 0);
    }

    #[test]
    fn receiving_identity_is_account_and_origin_scoped() {
        assert!(validate_id("https://other.example/01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
        assert_ne!(
            identity("https://one.example", 1, "transfer"),
            identity("https://two.example", 1, "transfer")
        );
        assert_ne!(
            identity("https://one.example", 1, "transfer"),
            identity("https://one.example", 2, "transfer")
        );
    }
}
