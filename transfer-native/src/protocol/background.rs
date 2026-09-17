//! File-backed work descriptors for an OS-owned HTTP executor.
//!
//! The executor never learns plaintext or protocol keys. Rust issues a
//! descriptor only after its immutable ciphertext artifact is durable, and it
//! authenticates the operation identity and artifact again before committing an
//! OS completion.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, bail};

use crate::{
    checkpoint::{SecretStore, Store},
    control::Control,
    source::UploadSource,
    uploads::DirectoryMode,
};

use super::{UploadOptions, download, upload};

const UPLOAD_RESPONSE_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct BackgroundWork {
    pub operation_id: String,
    pub transfer_id: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<BackgroundHeader>,
    pub body_path: String,
    pub expected_response_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct BackgroundHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct BackgroundCapabilities {
    pub upload: bool,
    pub download: bool,
    pub turbo: bool,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct BackgroundStatus {
    pub state: String,
    pub needs_execution: bool,
    pub awaiting_unlock: bool,
    pub done: u64,
    pub total: u64,
    pub direction: String,
    pub kind: String,
    pub transport: String,
    pub remote_id: Option<String>,
}

/// Start an HTTP upload through the normal reservation and v1 encryption path,
/// stopping only after every ciphertext record is immutable on disk.
pub fn prepare_upload(
    instance: &str,
    paths: &[std::path::PathBuf],
    mode: DirectoryMode,
    options: UploadOptions,
    control: &Control,
) -> Result<String> {
    upload::run_background(instance, paths, mode, options, control)
}

/// Prepare immutable HTTP upload artifacts from bounded source snapshots.
pub fn prepare_upload_sources(
    instance: &str,
    sources: &[UploadSource],
    mode: DirectoryMode,
    options: UploadOptions,
    control: &Control,
) -> Result<String> {
    upload::run_background_sources(instance, sources, mode, options, control)
}

pub fn capabilities() -> BackgroundCapabilities {
    BackgroundCapabilities {
        upload: true,
        download: true,
        turbo: true,
        detail: "direct HTTP uploads, Turbo descriptors, and downloads; downloads require foreground final verification and unlock".into(),
    }
}

pub fn status(home: &Path, id: &str, secrets: Arc<dyn SecretStore>) -> Result<BackgroundStatus> {
    let store = Store::open_with_secret_store(home, id, secrets)?;
    let value = store
        .load::<serde_json::Value>()?
        .context("saved transfer checkpoint is empty")?;
    let state = value
        .get("state")
        .and_then(serde_json::Value::as_str)
        .context("invalid background transfer checkpoint")?
        .to_owned();
    let is_download =
        value.get("direction").and_then(serde_json::Value::as_str) == Some("download");
    let awaiting_unlock = is_download
        && value
            .get("envelope")
            .and_then(serde_json::Value::as_str)
            .and_then(|envelope| serde_json::from_str::<serde_json::Value>(envelope).ok())
            .is_some_and(|envelope| envelope.get("salt").is_some());
    Ok(BackgroundStatus {
        needs_execution: state == "awaiting-execution",
        awaiting_unlock,
        state,
        done: value
            .get("done")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        total: value
            .get("total")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        direction: value
            .get("direction")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .into(),
        kind: "files".into(),
        transport: value
            .get("driver")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("http")
            .into(),
        remote_id: value
            .get("transfer_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

/// Resolve and unlock the manifest while foregrounded, then leave only
/// ciphertext fetch work for URLSession. The existing checkpoint stores the
/// public share key under its configured SecretStore; password-derived material
/// is never persisted.
pub fn prepare_download(
    instance: &str,
    link: &str,
    output: &Path,
    control: &Control,
) -> Result<String> {
    download::run_background(instance, link, output, control)
}

pub fn prepare_inbox_download(
    instance: &str,
    transfer_id: &str,
    working_key: &[u8],
    cookie: &str,
    output: &Path,
    control: &Control,
) -> Result<String> {
    download::run_inbox_background(instance, transfer_id, working_key, cookie, output, control)
}

pub fn pending_download_work(
    home: &Path,
    id: &str,
    cookie: Option<&str>,
    secrets: Arc<dyn SecretStore>,
) -> Result<Vec<BackgroundWork>> {
    let store = Store::open_with_secret_store(home, id, secrets)?;
    download::background_work(&store, cookie)
}

/// Returns decrypted, authenticated manifest display metadata from a prepared
/// checkpoint. This is local checkpoint I/O only and never contacts a peer.
pub fn download_items(
    home: &Path,
    id: &str,
    secrets: Arc<dyn SecretStore>,
) -> Result<Vec<download::DownloadItem>> {
    let store = Store::open_with_secret_store(home, id, secrets)?;
    download::background_items(&store)
}

/// Persists a non-empty, unique subset before URLSession descriptors are first
/// issued. The complete authenticated manifest remains in the checkpoint.
pub fn select_download_items(
    home: &Path,
    id: &str,
    item_ids: &[String],
    secrets: Arc<dyn SecretStore>,
) -> Result<()> {
    let store = Store::open_with_secret_store(home, id, secrets)?;
    download::select_background_items(&store, item_ids)
}

pub fn ingest_download_completion(
    home: &Path,
    id: &str,
    operation_id: &str,
    status: u16,
    response_headers: &[BackgroundHeader],
    response_path: &Path,
    secrets: Arc<dyn SecretStore>,
) -> Result<()> {
    let store = Store::open_with_secret_store(home, id, secrets)?;
    download::ingest_background_completion(
        &store,
        operation_id,
        status,
        response_headers,
        response_path,
    )
}

/// Run existing resume authentication, plaintext construction, whole-file
/// digest verification, and atomic publication. Password links reprompt here.
pub fn finalize_download(id: &str, control: &Control) -> Result<Vec<String>> {
    let id = uuid::Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    control.set_checkpoint_id(id.clone());
    let store = control.open_checkpoint_store(&id)?;
    let pending = download::background_work(&store, None)?;
    if !pending.is_empty() {
        bail!("background download still has unreceived operations");
    }
    download::resume(store, control)
}

pub fn finalize_inbox_download(
    id: &str,
    instance: &str,
    working_key: &[u8],
    cookie: &str,
    control: &Control,
) -> Result<Vec<String>> {
    let id = uuid::Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    control.set_checkpoint_id(id.clone());
    let store = control.open_checkpoint_store(&id)?;
    let pending = download::background_work(&store, Some(cookie))?;
    if !pending.is_empty() {
        bail!("background inbox download still has unreceived operations");
    }
    drop(store);
    download::resume_inbox(&id, instance, working_key, cookie, control)
}

pub fn pending_upload_work(
    home: &Path,
    id: &str,
    secrets: Arc<dyn SecretStore>,
) -> Result<Vec<BackgroundWork>> {
    let store = Store::open_with_secret_store(home, id, secrets)?;
    upload::background_work(&store)
}

/// Commit one OS completion. A non-201 result is retained as pending work so
/// URLSession retry policy can run it again; it never advances protocol state.
pub fn ingest_upload_completion(
    home: &Path,
    id: &str,
    operation_id: &str,
    status: u16,
    response_path: Option<&Path>,
    secrets: Arc<dyn SecretStore>,
) -> Result<()> {
    if let Some(path) = response_path {
        let metadata = std::fs::metadata(path).context("inspect background response file")?;
        if !metadata.is_file() || metadata.len() > UPLOAD_RESPONSE_LIMIT {
            bail!("upload completion has an unexpected response body");
        }
    }
    let store = Store::open_with_secret_store(home, id, secrets)?;
    upload::ingest_background_completion(&store, operation_id, status)
}

pub fn reconcile_upload(id: &str, control: &Control) -> Result<()> {
    let id = uuid::Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    control.set_checkpoint_id(id.clone());
    let store = control.open_checkpoint_store(&id)?;
    upload::reconcile_background(store, control)
}

/// Complete the normal v1 manifest request after every external chunk
/// acknowledgement has been durably committed. This deliberately reuses the
/// existing restart-safe upload finalization and never invents success.
pub fn finalize_upload(id: &str, control: &Control) -> Result<String> {
    let id = uuid::Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let store = control.open_checkpoint_store(&id)?;
    control.set_checkpoint_id(id.clone());
    let work = upload::background_work(&store)?;
    if !work.is_empty() {
        bail!("background upload still has unacknowledged operations");
    }
    let result = upload::resume(store, control)?;
    result
        .into_iter()
        .next()
        .context("background upload finalization returned no receipt")
}
