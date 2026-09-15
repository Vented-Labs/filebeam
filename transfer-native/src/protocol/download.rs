//! Restart-safe native download implementation.
//!
//! Ciphertext is retained privately until a whole AEAD record is present.  A
//! partial response is deliberately never passed to the decryptor.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::checkpoint::Store;
use crate::webrtc::{NativePeer, ReceiverSession, Signaling, connect_receiver, request_chunk};
use anyhow::{Context, Result, bail};
use base64::Engine;
use filebeam_transfer::{AdaptiveConcurrency, concurrency_limit, retry_delay_ms, retryable_status};
use futures_util::{StreamExt, stream::FuturesUnordered};
use reqwest::{
    Client, StatusCode,
    header::{
        CONTENT_LENGTH, CONTENT_RANGE, COOKIE, ETAG, HeaderMap, HeaderValue, IF_RANGE, RANGE,
        RETRY_AFTER,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::*;

const VERSION: u8 = 1;
const MAX_ATTEMPTS: u32 = 6;
const BODY_IDLE: Duration = Duration::from_secs(20);
const LIVE_HEARTBEAT: Duration = Duration::from_secs(2);

#[derive(Clone, Serialize, Deserialize)]
struct DownloadJob {
    // These fields are intentionally top-level: protocol::saved_transfers
    // reads summaries without knowing this direction's internals.
    version: u8,
    id: String,
    direction: String,
    state: String,
    instance: String,
    done: u64,
    total: u64,
    transfer_id: String,
    chunk_bytes: u64,
    server_concurrency: u32,
    ranges: bool,
    #[serde(default = "http_driver")]
    driver: String,
    envelope: String,
    #[serde(default)]
    source: DownloadSource,
    #[serde(default)]
    turbo: Option<TurboJob>,
    output: PathBuf,
    items: Vec<SavedItem>,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
enum DownloadSource {
    #[default]
    Public,
    Inbox,
}

#[derive(Clone, Serialize, Deserialize)]
struct TurboJob {
    descriptor: String,
    session_id: String,
    session_token: String,
    sequence: u64,
}

#[derive(Deserialize)]
struct TurboDescriptor {
    version: u8,
    purpose: String,
    chunk_bytes: u64,
    items: Vec<TurboDescriptorItem>,
}

#[derive(Deserialize)]
struct TurboDescriptorItem {
    id: String,
    name: String,
    #[serde(rename = "type")]
    mime: String,
    size: u64,
    nonce_prefix: String,
    chunk_count: u64,
}

#[derive(Deserialize)]
struct TurboSession {
    id: String,
    token: String,
}

fn http_driver() -> String {
    "http".into()
}

#[derive(Deserialize)]
struct DownloadCapabilities {
    #[serde(default)]
    download_concurrency: Option<u32>,
    #[serde(default)]
    transfer_capabilities: Option<RangeCapabilities>,
}

#[derive(Deserialize)]
struct RangeCapabilities {
    #[serde(default)]
    download_ranges: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedItem {
    id: String,
    name: String,
    size: u64,
    nonce_prefix: String,
    chunk_count: u64,
    digest: String,
    target: PathBuf,
    verified: Vec<bool>,
    published: bool,
}

struct UnlockedKeys {
    master: Zeroizing<Vec<u8>>,
    share_key: Zeroizing<Vec<u8>>,
}

struct FetchChunkRequest<'a> {
    client: &'a Client,
    url: &'a str,
    directory: &'a Path,
    item: usize,
    chunk: u64,
    plain: u64,
    ranges: bool,
    cancel: &'a CancellationToken,
    adaptive: &'a Arc<std::sync::Mutex<AdaptiveConcurrency>>,
    receiving: &'a Arc<std::sync::Mutex<ReceiveProgress>>,
    control: &'a Control,
    item_name: &'a str,
    item_size: u64,
    fresh: bool,
    turbo: bool,
}

struct AuthenticateAndWriteRequest<'a> {
    base: &'a Path,
    item_index: usize,
    index: u64,
    chunk_bytes: u64,
    transfer_id: &'a str,
    item: &'a SavedItem,
    master: &'a [u8],
    ciphertext: Vec<u8>,
    retain_ciphertext: bool,
}

struct LiveConnection {
    peer: NativePeer,
    session: ReceiverSession,
    channel: Arc<dyn webrtc::data_channel::DataChannel>,
}

/// WebRTC's request/response/ACK contract permits exactly one outstanding
/// chunk per channel. A failed request is never resumed on that channel: a new
/// receiver session is registered, which also makes resume independent of an
/// expired signaling session.
struct LiveDownload {
    signaling: Signaling,
    join_token: String,
    relay_only: bool,
    connection: Option<LiveConnection>,
    sequence: u32,
}

impl LiveDownload {
    fn new(
        client: Client,
        instance: &str,
        transfer_id: &str,
        join_token: String,
        relay_only: bool,
    ) -> Result<Self> {
        Ok(Self {
            signaling: Signaling::new(client, instance, transfer_id)?,
            join_token,
            relay_only,
            connection: None,
            sequence: 0,
        })
    }

    async fn reconnect(&mut self, control: &Control, progress: u8) -> Result<()> {
        if let Some(connection) = self.connection.take() {
            let reported = self
                .signaling
                .report(&connection.session, "failed", progress)
                .await;
            connection.peer.close().await;
            reported.context("report failed live receiver session")?;
        }
        control.phase(Phase::Connecting)?;
        let (peer, session, channel) =
            connect_receiver(&self.signaling, &self.join_token, self.relay_only).await?;
        self.signaling.report(&session, "active", progress).await?;
        self.connection = Some(LiveConnection {
            peer,
            session,
            channel,
        });
        Ok(())
    }

    async fn fetch(
        &mut self,
        item_id: String,
        index: u64,
        expected: u64,
        progress: u8,
        cancel: &CancellationToken,
        control: &Control,
    ) -> Result<Vec<u8>> {
        for attempt in 0..MAX_ATTEMPTS {
            if self.connection.is_none() {
                if attempt > 0 {
                    control.phase(Phase::Reconnecting)?;
                }
                if let Err(error) = self.reconnect(control, progress).await {
                    retry_wait(None, attempt, cancel).await?;
                    if attempt + 1 == MAX_ATTEMPTS {
                        return Err(error);
                    }
                    continue;
                }
            }
            let connection = self
                .connection
                .as_ref()
                .context("live receiver is unavailable")?;
            let sequence = self.sequence;
            self.sequence = self.sequence.wrapping_add(1);
            let request = request_chunk(
                connection.channel.clone(),
                sequence,
                item_id.clone(),
                index,
                expected,
            );
            tokio::pin!(request);
            let mut heartbeat = tokio::time::interval(LIVE_HEARTBEAT);
            heartbeat.tick().await;
            let received = loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        self.finish("cancelled", progress)
                            .await
                            .context("report cancelled live receiver session")?;
                        bail!("transfer cancelled");
                    }
                    _ = heartbeat.tick() => {
                        let Some(connection) = self.connection.as_ref() else { break Err(anyhow::anyhow!("live receiver disconnected")); };
                        if let Err(error) = self.signaling.report(&connection.session, "active", progress).await {
                            break Err(error);
                        }
                    }
                    result = &mut request => break result,
                }
            };
            match received {
                Ok(ciphertext) => {
                    control.phase(Phase::Receiving)?;
                    return Ok(ciphertext);
                }
                Err(error) => {
                    if let Some(connection) = self.connection.take() {
                        let reported = self
                            .signaling
                            .report(&connection.session, "failed", progress)
                            .await;
                        connection.peer.close().await;
                        reported.context("report failed live receiver session")?;
                    }
                    control.phase(Phase::Retrying)?;
                    retry_wait(None, attempt, cancel).await?;
                    if attempt + 1 == MAX_ATTEMPTS {
                        return Err(error);
                    }
                }
            }
        }
        bail!("live chunk retry window exhausted")
    }

    async fn finish(&mut self, status: &str, progress: u8) -> Result<()> {
        if let Some(connection) = self.connection.take() {
            let reported = self
                .signaling
                .report(&connection.session, status, progress)
                .await;
            connection.peer.close().await;
            reported.with_context(|| format!("report {status} live receiver session"))?;
        }
        Ok(())
    }
}

fn live_progress(done: u64, total: u64) -> u8 {
    done.saturating_mul(100)
        .checked_div(total)
        .unwrap_or(0)
        .min(99) as u8
}

/// Tracks uncommitted body bytes across concurrent requests. UI `done` may
/// include these bytes, while `committed` is advanced only after AEAD and disk
/// persistence have completed.
struct ReceiveProgress {
    base_done: u64,
    verified: u64,
    item_verified: Vec<u64>,
    active: HashMap<(usize, u64), u64>,
}

impl ReceiveProgress {
    fn new(base_done: u64, item_verified: Vec<u64>) -> Self {
        Self {
            base_done,
            verified: 0,
            item_verified,
            active: HashMap::new(),
        }
    }

    fn report(&mut self, item: usize, chunk: u64, bytes: u64) -> (u64, u64) {
        self.active.insert((item, chunk), bytes);
        self.snapshot(item)
    }

    fn restart(&mut self, item: usize, chunk: u64) -> (u64, u64) {
        self.active.insert((item, chunk), 0);
        self.snapshot(item)
    }

    fn verified(&mut self, item: usize, chunk: u64, bytes: u64) -> (u64, u64) {
        self.active.remove(&(item, chunk));
        self.verified = self.verified.saturating_add(bytes);
        self.item_verified[item] = self.item_verified[item].saturating_add(bytes);
        self.snapshot(item)
    }

    fn snapshot(&self, item: usize) -> (u64, u64) {
        let active = self.active.values().copied().sum::<u64>();
        let item_active = self
            .active
            .iter()
            .filter(|((active_item, _), _)| *active_item == item)
            .map(|(_, bytes)| *bytes)
            .sum::<u64>();
        (
            self.base_done
                .saturating_add(self.verified)
                .saturating_add(active),
            self.item_verified[item].saturating_add(item_active),
        )
    }
}

impl DownloadJob {
    fn checked(&self) -> Result<()> {
        if self.version != VERSION
            || self.direction != "download"
            || self.id.is_empty()
            || self.chunk_bytes == 0
            || !matches!(self.driver.as_str(), "http" | "webrtc")
            || !self.output.is_absolute()
            || self.items.iter().any(|i| {
                i.verified.len() != i.chunk_count as usize
                    || i.target.parent() != Some(self.output.as_path())
                    || i.target.file_name().is_none()
            })
        {
            bail!("invalid download checkpoint");
        }
        Ok(())
    }
}

/// Start a download in a small async runtime.  The synchronous UI is bridged
/// to a cancellation token by a 50ms watcher, so an in-flight request is
/// dropped instead of waiting for a global request timeout.
pub(super) fn run(
    instance: &str,
    raw: &str,
    output: &Path,
    control: &Control,
) -> Result<Vec<PathBuf>> {
    let parsed = parse_link_for_instance(raw, instance)?;
    let id = uuid::Uuid::new_v4().to_string();
    let store = control.create_checkpoint_store(&id)?;
    crate::runtime::shared_tokio_runtime().block_on(async_run(
        parsed,
        DownloadSource::Public,
        None,
        output.to_path_buf(),
        store,
        control.clone(),
    ))
}

pub(super) fn run_inbox(
    instance: &str,
    transfer_id: &str,
    working_key: &[u8],
    cookie: &str,
    output: &Path,
    control: &Control,
) -> Result<Vec<PathBuf>> {
    if working_key.len() != 32 || cookie.is_empty() || cookie.contains(['\r', '\n']) {
        bail!("invalid authenticated inbox download credentials");
    }
    let parsed = parse_link_for_instance(
        &format!(
            "{transfer_id}#k=v1.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(working_key)
        ),
        instance,
    )?;
    let id = uuid::Uuid::new_v4().to_string();
    let store = control.create_checkpoint_store(&id)?;
    crate::runtime::shared_tokio_runtime().block_on(async_run(
        parsed,
        DownloadSource::Inbox,
        Some(cookie),
        output.to_path_buf(),
        store,
        control.clone(),
    ))
}

pub(super) fn resume(store: Store, control: &Control) -> Result<Vec<String>> {
    let job = store
        .load::<DownloadJob>()?
        .context("saved download checkpoint is empty")?;
    job.checked()?;
    if job.source == DownloadSource::Inbox {
        bail!("inbox download resume requires reauthentication");
    }
    crate::runtime::shared_tokio_runtime().block_on(async_resume(
        store,
        job,
        None,
        None,
        control.clone(),
    ))
}

pub(super) fn resume_inbox(
    id: &str,
    instance: &str,
    working_key: &[u8],
    cookie: &str,
    control: &Control,
) -> Result<Vec<String>> {
    if working_key.len() != 32 || cookie.is_empty() || cookie.contains(['\r', '\n']) {
        bail!("invalid authenticated inbox download credentials");
    }
    let id = uuid::Uuid::parse_str(id)
        .context("invalid saved transfer id")?
        .to_string();
    let store = control.open_checkpoint_store(&id)?;
    let job = store
        .load::<DownloadJob>()?
        .context("saved download checkpoint is empty")?;
    job.checked()?;
    if job.source != DownloadSource::Inbox {
        bail!("saved transfer is not an inbox download");
    }
    if job.instance != instance {
        bail!("inbox session origin does not match the saved download");
    }
    control.set_checkpoint_id(id);
    crate::runtime::shared_tokio_runtime().block_on(async_resume(
        store,
        job,
        Some(Zeroizing::new(working_key.to_vec())),
        Some(cookie),
        control.clone(),
    ))
}

async fn async_run(
    link: Link,
    source: DownloadSource,
    cookie: Option<&str>,
    output: PathBuf,
    store: Store,
    control: Control,
) -> Result<Vec<PathBuf>> {
    let cancel = cancellation_bridge(control.clone());
    let client = async_client(&control, cookie)?;
    control.phase(Phase::Connecting)?;
    let capabilities = capabilities(&client, &link.instance, &cancel).await?;
    let transfer = metadata(
        &client,
        &link.instance,
        &link.id,
        &source,
        &cancel,
        &control,
    )
    .await?;
    validate_transfer(&transfer, &link.id)?;
    reject_live_note(&transfer)?;
    let descriptor = transfer
        .encrypted_manifest
        .is_none()
        .then(|| transfer.encrypted_descriptor.clone())
        .flatten();
    let envelope = transfer
        .encrypted_manifest
        .clone()
        .or_else(|| descriptor.clone())
        .context("transfer has no encrypted manifest or Turbo descriptor")?;
    let unlock_link = Link {
        id: link.id.clone(),
        key: link.key.clone(),
        instance: link.instance.clone(),
    };
    let UnlockedKeys { master, share_key } =
        unlock_async(unlock_link, envelope.clone(), control.clone()).await?;
    // The share key is secret state, but the checkpoint Store guarantees a
    // private, owner-only file. Passwords and password-derived keys are never stored.
    if source == DownloadSource::Public {
        store.save_secret("share-key", &share_key)?;
    }
    fs::create_dir_all(&output)?;
    let output = fs::canonicalize(&output).context("canonicalize download output directory")?;
    let manifest = if descriptor.is_some() {
        turbo_descriptor_for(&master, &link.id, &envelope, &transfer, &control)?
    } else {
        manifest_for(&master, &link.id, &envelope, &transfer, &control)?
    };
    let live_join_token = live_join_token(&transfer, &manifest)?;
    let total = manifest
        .items
        .iter()
        .try_fold(0u64, |n, i| n.checked_add(i.size))
        .context("transfer is too large")?;
    let mut names = HashSet::new();
    let items = manifest
        .items
        .iter()
        .map(|item| {
            if !names.insert(item.name.clone()) {
                bail!("manifest has duplicate filenames");
            }
            Ok(SavedItem {
                id: item.id.clone(),
                name: item.name.clone(),
                size: item.size,
                nonce_prefix: item.nonce_prefix.clone(),
                chunk_count: item.chunk_count,
                digest: if descriptor.is_some() {
                    String::new()
                } else {
                    item.digest.value.clone()
                },
                target: collision_free(&output, &safe_filename(&item.name)),
                verified: vec![false; item.chunk_count as usize],
                published: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let turbo = if let Some(descriptor) = descriptor {
        let session =
            create_turbo_session(&client, &link.instance, &link.id, &items, &cancel).await?;
        Some(TurboJob {
            descriptor,
            session_id: session.id,
            session_token: session.token,
            sequence: 0,
        })
    } else {
        None
    };
    let job = DownloadJob {
        version: VERSION,
        id: store
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        direction: "download".into(),
        state: "running".into(),
        instance: link.instance,
        done: 0,
        total,
        transfer_id: link.id,
        chunk_bytes: transfer.chunk_bytes,
        server_concurrency: server_download_limit(&capabilities, &transfer),
        ranges: server_ranges(&capabilities, &transfer),
        driver: transfer.driver.clone(),
        envelope,
        source,
        turbo,
        output,
        items,
    };
    // The checkpoint is durable before network data can arrive.
    store.save(&job)?;
    control.set_checkpoint_id(job.id.clone());
    Ok(
        download(store, job, master, client, cancel, control, live_join_token)
            .await?
            .into_iter()
            .map(PathBuf::from)
            .collect(),
    )
}

async fn async_resume(
    store: Store,
    mut job: DownloadJob,
    inbox_key: Option<Zeroizing<Vec<u8>>>,
    cookie: Option<&str>,
    control: Control,
) -> Result<Vec<String>> {
    if job.state == "complete" {
        return completed_paths(&job);
    }
    let cancel = cancellation_bridge(control.clone());
    let client = async_client(&control, cookie)?;
    let capabilities = capabilities(&client, &job.instance, &cancel).await?;
    let share_key = if job.source == DownloadSource::Inbox {
        inbox_key.context("inbox download resume requires reauthentication")?
    } else {
        Zeroizing::new(store.load_secret("share-key").and_then(|key| {
            key.context("saved download key is unavailable; restart with the original link")
        })?)
    };
    if share_key.len() != 32 {
        bail!("saved download key is invalid");
    }
    if job.source == DownloadSource::Public {
        store.save_secret("share-key", &share_key)?;
    }
    // Re-authenticate the original manifest before trusting any retained data.
    let transfer = metadata(
        &client,
        &job.instance,
        &job.transfer_id,
        &job.source,
        &cancel,
        &control,
    )
    .await?;
    validate_transfer(&transfer, &job.transfer_id)?;
    reject_live_note(&transfer)?;
    if transfer.chunk_bytes != job.chunk_bytes
        || transfer.driver != job.driver
        || (job.turbo.is_none() && transfer.encrypted_manifest.as_deref() != Some(&job.envelope))
        || (job.turbo.is_some()
            && transfer.encrypted_descriptor.as_deref()
                != job.turbo.as_ref().map(|turbo| turbo.descriptor.as_str()))
    {
        bail!("transfer identity changed; refusing saved ciphertext");
    }
    // Password-protected links always reprompt. The password and the derived
    // master key never cross the checkpoint boundary.
    let master = unlock_saved_async(share_key, job.envelope.clone(), control.clone()).await?;
    job.server_concurrency = server_download_limit(&capabilities, &transfer);
    job.ranges = server_ranges(&capabilities, &transfer);
    let manifest = if job.turbo.is_some() {
        turbo_descriptor_for(
            &master,
            &job.transfer_id,
            &job.envelope,
            &transfer,
            &control,
        )?
    } else {
        manifest_for(
            &master,
            &job.transfer_id,
            &job.envelope,
            &transfer,
            &control,
        )?
    };
    let live_join_token = live_join_token(&transfer, &manifest)?;
    if manifest.items.len() != job.items.len() {
        bail!("saved download layout changed");
    }
    let transfer_id = job.transfer_id.clone();
    let chunk_bytes = job.chunk_bytes;
    for (item_index, (saved, item)) in job.items.iter_mut().zip(&manifest.items).enumerate() {
        if saved.id != item.id
            || saved.size != item.size
            || saved.chunk_count != item.chunk_count
            || (job.turbo.is_none() && saved.digest != item.digest.value)
        {
            bail!("saved download layout changed");
        }
        if saved.published {
            if !saved.target.is_file() || sha256_file(&saved.target)? != saved.digest {
                bail!("published download target is missing or corrupted");
            }
            continue;
        }
        // A journal bit is only trusted after the retained ciphertext decrypts again.
        // Rewrite the scratch prefix from authenticated ciphertext: a crash may
        // occur after its checkpoint bit is durable but before an earlier sparse
        // write is visible, and scratch plaintext is never a trust boundary.
        for index in 0..saved.chunk_count {
            if saved.verified[index as usize]
                && !restore_retained(
                    &store,
                    item_index,
                    &transfer_id,
                    chunk_bytes,
                    saved,
                    index,
                    &master,
                )?
            {
                saved.verified[index as usize] = false;
            }
        }
    }
    job.done = job
        .items
        .iter()
        .map(|i| {
            i.verified
                .iter()
                .enumerate()
                .filter(|(_, ok)| **ok)
                .map(|(n, _)| plaintext_len(i.size, job.chunk_bytes, n as u64))
                .sum::<u64>()
        })
        .sum();
    store.save(&job)?;
    download(store, job, master, client, cancel, control, live_join_token).await
}

async fn download(
    store: Store,
    mut job: DownloadJob,
    master: Zeroizing<Vec<u8>>,
    client: Client,
    cancel: CancellationToken,
    control: Control,
    live_join_token: Option<String>,
) -> Result<Vec<String>> {
    control.totals(job.total, job.items.len());
    let configured = control.max_concurrency().unwrap_or(4);
    let slots = concurrency_limit(
        configured,
        job.chunk_bytes,
        control.memory_budget(),
        job.server_concurrency,
    )
    .min(job.server_concurrency) as usize;
    let live = job.driver == "webrtc";
    if !matches!(job.driver.as_str(), "http" | "webrtc") {
        bail!("unsupported download driver");
    }
    // Peer transport exposes the receiver's IP address to the sender. The
    // caller's shared consent flow must approve that before registration.
    if live && !control.webrtc_relay_only() {
        control.request_peer_consent(job.transfer_id.clone())?;
    }
    let slots = if live { 1 } else { slots };
    let rtc = if live {
        Some(Arc::new(tokio::sync::Mutex::new(LiveDownload::new(
            client.clone(),
            &job.instance,
            &job.transfer_id,
            live_join_token.context("live transfer has no join capability")?,
            control.webrtc_relay_only(),
        )?)))
    } else {
        None
    };
    let adaptive = Arc::new(std::sync::Mutex::new(AdaptiveConcurrency::new(
        slots as u32,
    )));
    let item_progress = job
        .items
        .iter()
        .map(|item| {
            item.verified
                .iter()
                .enumerate()
                .filter(|(_, ok)| **ok)
                .map(|(index, _)| plaintext_len(item.size, job.chunk_bytes, index as u64))
                .sum()
        })
        .collect();
    let receiving = Arc::new(std::sync::Mutex::new(ReceiveProgress::new(
        job.done,
        item_progress,
    )));
    // A crash after the final checkpoint bit but before publication leaves no
    // queue work on resume, yet still needs the authenticated scratch file
    // published before the job can become complete.
    for item_index in 0..job.items.len() {
        if job.turbo.is_none()
            && !job.items[item_index].published
            && job.items[item_index].verified.iter().all(|ok| *ok)
        {
            control.phase(Phase::Finalizing)?;
            let store_path = store.path().to_path_buf();
            let item = job.items[item_index].clone();
            tokio::task::spawn_blocking(move || publish_item(&store_path, &item, item_index))
                .await
                .context("download publication worker stopped")??;
            job.items[item_index].published = true;
            store.save(&job)?;
            let store_path = store.path().to_path_buf();
            let chunk_count = job.items[item_index].chunk_count;
            tokio::task::spawn_blocking(move || cleanup_item(&store_path, item_index, chunk_count))
                .await
                .context("download cleanup worker stopped")?;
        }
    }
    // One queue prevents a long first file from serializing every later item.
    // Index-major insertion starts each file promptly and stays fair thereafter.
    let mut pending = VecDeque::new();
    let max_chunks = job
        .items
        .iter()
        .map(|item| item.chunk_count)
        .max()
        .unwrap_or(0);
    for chunk in 0..max_chunks {
        for (item_index, item) in job.items.iter().enumerate() {
            if chunk < item.chunk_count && !item.verified[chunk as usize] {
                pending.push_back((item_index, chunk));
            }
        }
    }
    let mut fetched = FuturesUnordered::new();
    let turbo_download = job.turbo.is_some();
    while !pending.is_empty() || !fetched.is_empty() {
        // This is the sole request budget for every item. New slots are filled
        // immediately after any body/completion sample raises the adaptive limit.
        let limit = adaptive.lock().unwrap().limit().min(slots as u32) as usize;
        while fetched.len() < limit {
            let Some((item_index, index)) = pending.pop_front() else {
                break;
            };
            let client = client.clone();
            let cancel = cancel.clone();
            let store_path = store.path().to_path_buf();
            let item = job.items[item_index].clone();
            let url = chunk_url(
                &job.instance,
                &job.transfer_id,
                &item.id,
                index,
                &job.source,
            );
            let adaptive = adaptive.clone();
            let receiving = receiving.clone();
            let control = control.clone();
            let ranges = job.ranges;
            let chunk_bytes = job.chunk_bytes;
            let live = rtc.clone();
            let done = job.done;
            let total = job.total;
            fetched.push(async move {
                let plain = plaintext_len(item.size, chunk_bytes, index);
                let data = if let Some(live) = live {
                    let data = live
                        .lock()
                        .await
                        .fetch(
                            item.id.clone(),
                            index,
                            plain + TAG_BYTES,
                            live_progress(done, total),
                            &cancel,
                            &control,
                        )
                        .await?;
                    let (display_done, item_done) =
                        receiving.lock().unwrap().report(item_index, index, plain);
                    control.item(item.name.clone(), item_index + 1, item.size);
                    control.advance(display_done, item_done, data.len() as u64);
                    data
                } else {
                    fetch_chunk(FetchChunkRequest {
                        client: &client,
                        url: &url,
                        directory: &store_path,
                        item: item_index,
                        chunk: index,
                        plain,
                        ranges,
                        cancel: &cancel,
                        adaptive: &adaptive,
                        receiving: &receiving,
                        control: &control,
                        item_name: &item.name,
                        item_size: item.size,
                        fresh: false,
                        turbo: turbo_download,
                    })
                    .await?
                };
                Ok::<_, anyhow::Error>((item_index, index, data))
            });
        }
        let Some(result) = fetched.next().await else {
            continue;
        };
        if let Err(error) = control.check() {
            job.state = "paused".into();
            let _ = store.save(&job);
            if let Some(live) = &rtc {
                live.lock()
                    .await
                    .finish("cancelled", live_progress(job.done, job.total))
                    .await?;
            }
            return Err(error);
        }
        let (item_index, index, ciphertext) = match result {
            Ok(value) => value,
            Err(error) => {
                job.state = "paused".into();
                let _ = store.save(&job);
                if let Some(live) = &rtc {
                    let status = if cancel.is_cancelled() {
                        "cancelled"
                    } else {
                        "failed"
                    };
                    live.lock()
                        .await
                        .finish(status, live_progress(job.done, job.total))
                        .await?;
                }
                return Err(error);
            }
        };
        control.phase(Phase::Verifying)?;
        let saved = job.items[item_index].clone();
        let store_path = store.path().to_path_buf();
        let transfer_id = job.transfer_id.clone();
        let master_copy = Zeroizing::new(master.to_vec());
        let chunk_bytes = job.chunk_bytes;
        let expected = tokio::task::spawn_blocking(move || {
            authenticate_and_write(AuthenticateAndWriteRequest {
                base: &store_path,
                item_index,
                index,
                chunk_bytes,
                transfer_id: &transfer_id,
                item: &saved,
                master: &master_copy,
                ciphertext,
                retain_ciphertext: live,
            })
        })
        .await
        .context("download verification worker stopped")?;
        let expected = match expected {
            Ok(expected) => expected,
            Err(_) => {
                // Retained data is only a resumable transport prefix, never a
                // trust boundary. One whole-record retry repairs a corrupt
                // prefix after a power loss without masking persistent bad
                // server ciphertext or protocol failures.
                let store_path = store.path().to_path_buf();
                fs_blocking(move || {
                    let _ = fs::remove_file(cipher_path(&store_path, item_index, index));
                    let _ = fs::remove_file(etag_path(&store_path, item_index, index));
                    Ok(())
                })
                .await?;
                let (done, item_done) = receiving.lock().unwrap().restart(item_index, index);
                control.item(
                    job.items[item_index].name.clone(),
                    item_index + 1,
                    job.items[item_index].size,
                );
                control.advance(done, item_done, 0);
                let item = job.items[item_index].clone();
                let plain = plaintext_len(item.size, job.chunk_bytes, index);
                let ciphertext = if let Some(live) = &rtc {
                    let data = live
                        .lock()
                        .await
                        .fetch(
                            item.id.clone(),
                            index,
                            plain + TAG_BYTES,
                            live_progress(job.done, job.total),
                            &cancel,
                            &control,
                        )
                        .await?;
                    let (display_done, item_done) =
                        receiving.lock().unwrap().report(item_index, index, plain);
                    control.item(item.name.clone(), item_index + 1, item.size);
                    control.advance(display_done, item_done, data.len() as u64);
                    data
                } else {
                    let url = chunk_url(
                        &job.instance,
                        &job.transfer_id,
                        &item.id,
                        index,
                        &job.source,
                    );
                    fetch_chunk(FetchChunkRequest {
                        client: &client,
                        url: &url,
                        directory: store.path(),
                        item: item_index,
                        chunk: index,
                        plain,
                        ranges: job.ranges,
                        cancel: &cancel,
                        adaptive: &adaptive,
                        receiving: &receiving,
                        control: &control,
                        item_name: &item.name,
                        item_size: item.size,
                        fresh: true,
                        turbo: turbo_download,
                    })
                    .await?
                };
                let store_path = store.path().to_path_buf();
                let transfer_id = job.transfer_id.clone();
                let master = Zeroizing::new(master.to_vec());
                let chunk_bytes = job.chunk_bytes;
                tokio::task::spawn_blocking(move || {
                    authenticate_and_write(AuthenticateAndWriteRequest {
                        base: &store_path,
                        item_index,
                        index,
                        chunk_bytes,
                        transfer_id: &transfer_id,
                        item: &item,
                        master: &master,
                        ciphertext,
                        retain_ciphertext: live,
                    })
                })
                .await
                .context("download verification retry worker stopped")??
            }
        };
        // Data fsync precedes the checkpoint bit, making a post-crash skip safe.
        job.items[item_index].verified[index as usize] = true;
        job.done = job
            .done
            .checked_add(expected)
            .context("download progress overflow")?;
        store.save(&job)?;
        report_turbo(&client, &mut job, "downloading", &cancel).await?;
        store.save(&job)?;
        let (display_done, item_done) = receiving
            .lock()
            .unwrap()
            .verified(item_index, index, expected);
        control.item(
            job.items[item_index].name.clone(),
            item_index + 1,
            job.items[item_index].size,
        );
        control.advance(display_done, item_done, 0);
        control.commit(job.done);
        if job.turbo.is_none()
            && !job.items[item_index].published
            && job.items[item_index].verified.iter().all(|ok| *ok)
        {
            control.phase(Phase::Finalizing)?;
            let store_path = store.path().to_path_buf();
            let item = job.items[item_index].clone();
            tokio::task::spawn_blocking(move || publish_item(&store_path, &item, item_index))
                .await
                .context("download publication worker stopped")??;
            job.items[item_index].published = true;
            store.save(&job)?;
            let store_path = store.path().to_path_buf();
            let chunk_count = job.items[item_index].chunk_count;
            tokio::task::spawn_blocking(move || cleanup_item(&store_path, item_index, chunk_count))
                .await
                .context("download cleanup worker stopped")?;
        }
    }
    if job.turbo.is_some() {
        finalize_turbo_manifest(&client, &mut job, &master, &cancel, &control).await?;
        for item_index in 0..job.items.len() {
            let store_path = store.path().to_path_buf();
            let item = job.items[item_index].clone();
            tokio::task::spawn_blocking(move || publish_item(&store_path, &item, item_index))
                .await
                .context("Turbo publication worker stopped")??;
            job.items[item_index].published = true;
            store.save(&job)?;
        }
        report_turbo(&client, &mut job, "completed", &cancel).await?;
        store.save(&job)?;
    }
    job.state = "complete".into();
    store.save(&job)?;
    if let Some(live) = &rtc {
        live.lock().await.finish("completed", 100).await?;
    }
    let share_key = store.path().join("share-key");
    let _ = fs_blocking(move || {
        let _ = fs::remove_file(share_key);
        Ok(())
    })
    .await;
    Ok(job
        .items
        .iter()
        .map(|i| i.target.display().to_string())
        .collect())
}

fn publish_item(store_path: &Path, item: &SavedItem, index: usize) -> Result<()> {
    if item.verified.iter().any(|ok| !ok) {
        bail!("cannot publish incomplete download");
    }
    let source = plain_path(store_path, index);
    let mut input = File::open(&source)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        bytes += n as u64;
    }
    if bytes != item.size || hex::encode(hash.finalize()) != item.digest {
        bail!("SHA-256 verification failed for {}", item.name);
    }
    // Publication is staged in the output directory so it can be atomically
    // renamed without crossing the private job filesystem.
    if item.target.exists() {
        let target_digest = sha256_file(&item.target)?;
        if target_digest != item.digest {
            bail!("download target already exists");
        }
    } else {
        let parent = item
            .target
            .parent()
            .context("download target has no parent")?;
        let name = item
            .target
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let mut stage = tempfile::Builder::new()
            .prefix(&format!(".{name}.filebeam-{index}-"))
            .suffix(".part")
            .tempfile_in(parent)?;
        restrict(stage.path())?;
        let mut input = File::open(&source)?;
        std::io::copy(&mut input, stage.as_file_mut())?;
        stage.as_file_mut().sync_all()?;
        match persist_noclobber(stage, &item.target) {
            Ok(()) => {
                sync_parent(&item.target)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if sha256_file(&item.target)? != item.digest {
                    bail!("download target was created concurrently");
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Atomically publish a same-directory tempfile without replacing a name that
/// appeared after collision selection. Linux and Windows have native rename
/// primitives for this; tempfile's no-clobber implementation is retained for
/// platforms that do not expose one (where its filesystem support applies).
fn persist_noclobber(stage: tempfile::NamedTempFile, target: &Path) -> std::io::Result<()> {
    let stage = stage.into_temp_path();
    #[cfg(target_os = "linux")]
    {
        match rename_noclobber(stage.as_ref(), target) {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() != Some(38) => return Err(error), // ENOSYS
            Err(_) => {}
        }
    }
    #[cfg(windows)]
    {
        return move_file_noclobber(stage.as_ref(), target);
    }
    stage.persist_noclobber(target).map_err(|error| error.error)
}

#[cfg(target_os = "linux")]
fn rename_noclobber(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    unsafe extern "C" {
        fn renameat2(
            olddirfd: std::ffi::c_int,
            oldpath: *const std::ffi::c_char,
            newdirfd: std::ffi::c_int,
            newpath: *const std::ffi::c_char,
            flags: std::ffi::c_uint,
        ) -> std::ffi::c_int;
    }
    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in stage path"))?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in target path"))?;
    // RENAME_NOREPLACE is supported by Linux filesystems such as FAT/exFAT
    // that do not implement hard links.
    if unsafe { renameat2(-100, source.as_ptr(), -100, target.as_ptr(), 1) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn move_file_noclobber(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // MOVEFILE_WRITE_THROUGH (8), deliberately without MOVEFILE_REPLACE_EXISTING.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 8) } != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn cleanup_item(store_path: &Path, index: usize, chunk_count: u64) {
    // The durable publication bit is written by the caller before cleanup.
    for chunk in 0..chunk_count {
        let _ = fs::remove_file(cipher_path(store_path, index, chunk));
        let _ = fs::remove_file(etag_path(store_path, index, chunk));
    }
}

async fn fs_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .context("download file worker stopped")?
}

async fn fetch_chunk(request: FetchChunkRequest<'_>) -> Result<Vec<u8>> {
    let FetchChunkRequest {
        client,
        url,
        directory,
        item,
        chunk,
        plain,
        ranges,
        cancel,
        adaptive,
        receiving,
        control,
        item_name,
        item_size,
        fresh,
        turbo,
    } = request;
    let expected = plain + TAG_BYTES;
    let path = cipher_path(directory, item, chunk);
    let etag_path = etag_path(directory, item, chunk);
    let attempts = if turbo {
        MAX_ATTEMPTS.saturating_mul(100)
    } else {
        MAX_ATTEMPTS
    };
    for attempt in 0..attempts {
        let attempt_started = Instant::now();
        let metadata_path = path.clone();
        let mut existing =
            fs_blocking(move || Ok(fs::metadata(metadata_path).map(|m| m.len()).unwrap_or(0)))
                .await?;
        if fresh && attempt == 0 && existing > 0 {
            let path = path.clone();
            let etag_path = etag_path.clone();
            fs_blocking(move || {
                let _ = fs::remove_file(path);
                let _ = fs::remove_file(etag_path);
                Ok(())
            })
            .await?;
            existing = 0;
        }
        if existing > expected {
            let path = path.clone();
            let _ = fs_blocking(move || {
                let _ = fs::remove_file(path);
                Ok(())
            })
            .await;
            continue;
        }
        let etag_file = etag_path.clone();
        let etag = fs_blocking(move || Ok(fs::read_to_string(etag_file).ok())).await?;
        if existing == expected
            && etag
                .as_deref()
                .is_some_and(|tag| !tag.is_empty() && !tag.starts_with("W/"))
        {
            // A complete but unjournaled ciphertext record is authenticated by
            // the caller before it can advance progress.
            let path = path.clone();
            return fs_blocking(move || Ok(fs::read(path)?)).await;
        }
        let mut request = client.get(url);
        if existing > 0 && ranges {
            if let Some(tag) = &etag {
                request = request
                    .header(RANGE, format!("bytes={existing}-{}", expected - 1))
                    .header(IF_RANGE, tag);
            } else {
                let path = path.clone();
                let _ = fs_blocking(move || {
                    let _ = fs::remove_file(path);
                    Ok(())
                })
                .await;
                continue;
            }
        }
        let response = match tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), response = request.send() => response }
        {
            Ok(response) => response,
            Err(_) => {
                control.phase(Phase::Reconnecting)?;
                retry_wait(None, attempt, cancel).await?;
                continue;
            }
        };
        if response.status() == StatusCode::ACCEPTED {
            control.phase(Phase::Retrying)?;
            retry_wait(response.headers().get(RETRY_AFTER), attempt, cancel).await?;
            continue;
        }
        if retryable_status(response.status().as_u16(), false) {
            control.phase(Phase::Retrying)?;
            retry_wait(response.headers().get(RETRY_AFTER), attempt, cancel).await?;
            continue;
        }
        if !response.status().is_success() && response.status() != StatusCode::PARTIAL_CONTENT {
            bail!("chunk download failed: {}", response.status());
        }
        let append = existing > 0 && ranges && response.status() == StatusCode::PARTIAL_CONTENT;
        if response.status() == StatusCode::PARTIAL_CONTENT && !append {
            bail!("unexpected partial chunk response");
        }
        if append {
            validate_range(
                response.headers().get(CONTENT_RANGE),
                existing,
                expected - 1,
                expected,
            )?;
            if response.headers().get(ETAG).and_then(|v| v.to_str().ok()) != etag.as_deref() {
                bail!("range ETag changed");
            }
        } else if existing > 0 {
            /* If-Range 200 is a safe explicit reset. */
            let path = path.clone();
            let _ = fs_blocking(move || {
                let _ = fs::remove_file(path);
                Ok(())
            })
            .await;
            let (done, item_done) = receiving.lock().unwrap().restart(item, chunk);
            control.item(item_name.to_owned(), item + 1, item_size);
            control.advance(done, item_done, 0);
        }
        let length: u64 = response
            .headers()
            .get(CONTENT_LENGTH)
            .context("chunk response lacks Content-Length")?
            .to_str()?
            .parse()
            .context("invalid chunk Content-Length")?;
        if length
            != if append {
                expected - existing
            } else {
                expected
            }
        {
            bail!("chunk Content-Length mismatch");
        }
        if !append {
            let tag = response
                .headers()
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .context("chunk response lacks strong ETag")?;
            if tag.starts_with("W/") {
                bail!("chunk ETag must be strong");
            }
            // The entity validator is durable before any resumable prefix is
            // written. A kill between these operations merely causes a fresh
            // full request; it can never append a prefix to another entity.
            let etag_path = etag_path.clone();
            let tag = tag.to_owned();
            fs_blocking(move || {
                fs::write(&etag_path, tag)?;
                restrict(&etag_path)
            })
            .await?;
        }
        let output_path = path.clone();
        let mut out = fs_blocking(move || {
            let out = OpenOptions::new()
                .create(true)
                .append(append)
                .write(true)
                .truncate(!append)
                .open(&output_path)?;
            restrict(&output_path)?;
            Ok(out)
        })
        .await?;
        let mut response = response;
        let mut received = 0u64;
        let mut interrupted = false;
        let mut body_sample = Instant::now();
        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => bail!("transfer cancelled"),
                item = tokio::time::timeout(BODY_IDLE, response.chunk()) => item,
            };
            let bytes = match next {
                Ok(Ok(Some(bytes))) => bytes,
                Ok(Ok(None)) => break,
                // A timeout or transport EOF/error can safely resume the
                // authenticated entity prefix. Header/layout violations above
                // remain protocol failures and are deliberately not retried.
                Ok(Err(_)) | Err(_) => {
                    interrupted = true;
                    break;
                }
            };
            let byte_len = bytes.len() as u64;
            out = fs_blocking(move || {
                out.write_all(&bytes)?;
                Ok(out)
            })
            .await?;
            received += byte_len;
            control.phase(Phase::Receiving)?;
            let (done, item_done) = receiving.lock().unwrap().report(
                item,
                chunk,
                ciphertext_to_plain(plain, expected, existing.saturating_add(received)),
            );
            control.item(item_name.to_owned(), item + 1, item_size);
            control.advance(done, item_done, byte_len);
            let elapsed = body_sample.elapsed().as_millis() as u64;
            // Scheduler observations are made only for received body bytes.
            // A monotonic timestamp prevents wall-clock jumps from creating
            // spurious cooldowns or an epoch-zero congestion sample.
            adaptive
                .lock()
                .unwrap()
                .observe(byte_len, elapsed.max(1), now_ms());
            body_sample = Instant::now();
            if received > length {
                bail!("chunk response exceeds Content-Length");
            }
        }
        fs_blocking(move || {
            out.sync_all()?;
            Ok(())
        })
        .await?;
        if interrupted || received != length {
            control.phase(Phase::Retrying)?;
            retry_wait(None, attempt, cancel).await?;
            continue;
        }
        let path = path.clone();
        let data = fs_blocking(move || Ok(fs::read(path)?)).await?;
        if data.len() as u64 != expected {
            bail!("retained ciphertext is truncated");
        }
        // Small records may not produce enough body samples to open a second
        // slot. Completion supplies one conservative aggregate observation.
        adaptive.lock().unwrap().observe(
            expected,
            attempt_started.elapsed().as_millis().max(1) as u64,
            now_ms(),
        );
        return Ok(data);
    }
    bail!("chunk retry window exhausted")
}

fn unlock(link: &Link, envelope: &str, control: &Control) -> Result<UnlockedKeys> {
    let mut key = Zeroizing::new(match &link.key {
        Some(key) => key.clone(),
        None => decode_share_key(&control.secret(SecretKind::ShareKey)?)?,
    });
    let share_key = Zeroizing::new(key.to_vec());
    let envelope: Envelope =
        serde_json::from_str(envelope).context("invalid encrypted manifest envelope")?;
    if envelope.v != 1 {
        bail!("unsupported encrypted manifest version");
    }
    if let Some(salt) = envelope.salt {
        let password = control.secret(SecretKind::Password)?;
        let _kdf_memory = control.reserve_memory(PASSWORD_KDF_BYTES)?;
        let p = derive_password_key(password.as_bytes(), &decode(&salt)?, 65_536, 3, 1)?;
        key = Zeroizing::new(derive_password_protected_key(&key, &p)?);
    }
    Ok(UnlockedKeys {
        master: key,
        share_key,
    })
}
async fn unlock_async(link: Link, envelope: String, control: Control) -> Result<UnlockedKeys> {
    tokio::task::spawn_blocking(move || unlock(&link, &envelope, &control))
        .await
        .context("download unlock worker stopped")?
}
fn unlock_saved(share_key: &[u8], envelope: &str, control: &Control) -> Result<Zeroizing<Vec<u8>>> {
    let envelope: Envelope =
        serde_json::from_str(envelope).context("invalid encrypted manifest envelope")?;
    let mut key = Zeroizing::new(share_key.to_vec());
    if let Some(salt) = envelope.salt {
        let password = control.secret(SecretKind::Password)?;
        let _kdf_memory = control.reserve_memory(PASSWORD_KDF_BYTES)?;
        let p = derive_password_key(password.as_bytes(), &decode(&salt)?, 65_536, 3, 1)?;
        key = Zeroizing::new(derive_password_protected_key(&key, &p)?);
    }
    Ok(key)
}
async fn unlock_saved_async(
    share_key: Zeroizing<Vec<u8>>,
    envelope: String,
    control: Control,
) -> Result<Zeroizing<Vec<u8>>> {
    tokio::task::spawn_blocking(move || unlock_saved(&share_key, &envelope, &control))
        .await
        .context("download unlock worker stopped")?
}
fn manifest_for(
    key: &[u8],
    id: &str,
    envelope: &str,
    transfer: &Transfer,
    control: &Control,
) -> Result<Manifest> {
    let envelope: Envelope = serde_json::from_str(envelope)?;
    let _crypto_memory =
        control.reserve_memory(manifest_crypto_bytes(envelope.ciphertext.len())?)?;
    let bytes = decrypt_manifest(
        key,
        &decode(&envelope.nonce_prefix)?,
        &decode(&envelope.ciphertext)?,
        aad(id, "manifest", "manifest").as_bytes(),
    )
    .context("could not decrypt manifest")?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).context("invalid decrypted manifest")?;
    validate_manifest(&manifest, transfer)?;
    Ok(manifest)
}

fn turbo_descriptor_for(
    key: &[u8],
    id: &str,
    envelope: &str,
    transfer: &Transfer,
    control: &Control,
) -> Result<Manifest> {
    if transfer.driver != "http" {
        bail!("Turbo descriptors require HTTP transfers");
    }
    let envelope: Envelope =
        serde_json::from_str(envelope).context("invalid Turbo descriptor envelope")?;
    let _memory = control.reserve_memory(manifest_crypto_bytes(envelope.ciphertext.len())?)?;
    let bytes = decrypt_manifest(
        key,
        &decode(&envelope.nonce_prefix)?,
        &decode(&envelope.ciphertext)?,
        format!("filebeam:v1:{id}:descriptor:descriptor").as_bytes(),
    )
    .context("could not decrypt Turbo descriptor")?;
    let descriptor: TurboDescriptor =
        serde_json::from_slice(&bytes).context("invalid Turbo descriptor")?;
    if descriptor.version != 1
        || descriptor.purpose != "turbo-descriptor"
        || descriptor.chunk_bytes != transfer.chunk_bytes
        || descriptor.items.len() != transfer.items.len()
    {
        bail!("Turbo descriptor does not match transfer");
    }
    let mut ids = HashSet::new();
    let mut value = serde_json::json!({"version": 1, "items": []});
    for item in descriptor.items {
        let server = transfer
            .items
            .iter()
            .find(|server| server.id == item.id)
            .context("Turbo descriptor references an unknown item")?;
        if item.id.is_empty()
            || item.name.is_empty()
            || item.mime.len() > 255
            || !ids.insert(item.id.clone())
            || item.chunk_count == 0
            || item.chunk_count != server.chunk_count
            || item.chunk_count != item.size.div_ceil(transfer.chunk_bytes).max(1)
            || decode(&item.nonce_prefix)?.len() != 16
        {
            bail!("Turbo descriptor item is invalid");
        }
        value["items"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": item.id, "name": item.name, "type": item.mime, "size": item.size,
                "nonce_prefix": item.nonce_prefix, "chunk_count": item.chunk_count,
                "digest": {"algorithm":"sha256", "value": ""}
            }));
    }
    Ok(serde_json::from_value(value)?)
}

async fn create_turbo_session(
    client: &Client,
    instance: &str,
    id: &str,
    items: &[SavedItem],
    cancel: &CancellationToken,
) -> Result<TurboSession> {
    let response = tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), r = client.post(format!("{instance}/api/v1/transfers/{id}/download-sessions"))
    .json(&serde_json::json!({"item_ids": items.iter().map(|item| &item.id).collect::<Vec<_>>() })).send() => r? };
    if !response.status().is_success() {
        bail!(
            "create Turbo download session returned {}",
            response.status()
        );
    }
    let session = response_json::<Api<TurboSession>>(response, cancel, "Turbo session response")
        .await?
        .data;
    if session.id.is_empty() || session.token.is_empty() {
        bail!("invalid Turbo download session");
    }
    Ok(session)
}

async fn report_turbo(
    client: &Client,
    job: &mut DownloadJob,
    status: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let Some(turbo) = &mut job.turbo else {
        return Ok(());
    };
    turbo.sequence = turbo
        .sequence
        .checked_add(1)
        .context("Turbo session sequence overflow")?;
    let response = tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), r = client.patch(format!("{}/api/v1/transfers/{}/download-sessions/{}", job.instance, job.transfer_id, turbo.session_id))
    .header("X-Filebeam-Session-Token", &turbo.session_token)
    .json(&serde_json::json!({"sequence": turbo.sequence, "progress": if job.total == 0 { 0.0 } else { 100.0 * job.done as f64 / job.total as f64 }, "status": status})).send() => r? };
    if response.status().as_u16() != 204 {
        bail!("Turbo session update returned {}", response.status());
    }
    Ok(())
}

async fn finalize_turbo_manifest(
    client: &Client,
    job: &mut DownloadJob,
    master: &[u8],
    cancel: &CancellationToken,
    control: &Control,
) -> Result<()> {
    loop {
        let transfer = metadata(
            client,
            &job.instance,
            &job.transfer_id,
            &job.source,
            cancel,
            control,
        )
        .await?;
        if let Some(envelope) = transfer.encrypted_manifest.clone() {
            let manifest = manifest_for(master, &job.transfer_id, &envelope, &transfer, control)?;
            if manifest.items.len() != job.items.len() {
                bail!("final manifest does not bind Turbo descriptor");
            }
            for (saved, final_item) in job.items.iter_mut().zip(manifest.items) {
                if saved.id != final_item.id
                    || saved.name != final_item.name
                    || saved.size != final_item.size
                    || saved.nonce_prefix != final_item.nonce_prefix
                    || saved.chunk_count != final_item.chunk_count
                {
                    bail!("final manifest does not bind Turbo descriptor");
                }
                saved.digest = final_item.digest.value;
            }
            job.envelope = envelope;
            return Ok(());
        }
        control.phase(Phase::Waiting)?;
        tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
    }
}
fn live_join_token(transfer: &Transfer, manifest: &Manifest) -> Result<Option<String>> {
    match transfer.driver.as_str() {
        "http" => Ok(None),
        "webrtc" => {
            let token = manifest
                .join_token
                .clone()
                .context("live transfer manifest has no join capability")?;
            if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
                bail!("live transfer manifest has an invalid join capability");
            }
            Ok(Some(token))
        }
        _ => bail!("unsupported download driver"),
    }
}
fn validate_transfer(transfer: &Transfer, id: &str) -> Result<()> {
    if transfer.protocol_version != 1
        || transfer.id != id
        || transfer.chunk_bytes == 0
        || transfer.chunk_bytes > 24_999_984
        || !matches!(transfer.driver.as_str(), "http" | "webrtc")
    {
        bail!("unsupported or invalid transfer metadata");
    }
    Ok(())
}
fn reject_live_note(transfer: &Transfer) -> Result<()> {
    // File manifests always contain declared item records. Notes do not, and a
    // WebRTC registration would permanently claim a burn-on-read note before
    // this native file receiver can render or consume it.
    if transfer.driver == "webrtc" && transfer.items.is_empty() {
        bail!("native WebRTC note reads are unsupported; use the browser before claiming the note");
    }
    Ok(())
}
fn server_download_limit(capabilities: &DownloadCapabilities, transfer: &Transfer) -> u32 {
    capabilities
        .download_concurrency
        .unwrap_or(1)
        .min(transfer.download_concurrency.unwrap_or(u32::MAX))
        .clamp(1, 8)
}
fn server_ranges(capabilities: &DownloadCapabilities, transfer: &Transfer) -> bool {
    capabilities
        .transfer_capabilities
        .as_ref()
        .is_some_and(|value| value.download_ranges)
        && transfer.transfer_capabilities.download_ranges
}
async fn metadata(
    client: &Client,
    instance: &str,
    id: &str,
    source: &DownloadSource,
    cancel: &CancellationToken,
    control: &Control,
) -> Result<Transfer> {
    for attempt in 0..MAX_ATTEMPTS {
        let endpoint = match source {
            DownloadSource::Public => format!("{instance}/api/v1/transfers/{id}"),
            DownloadSource::Inbox => format!("{instance}/api/native/v1/inbox/{id}/metadata"),
        };
        let response = tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), r = client.get(endpoint).send() => r? };
        if response.status() == StatusCode::ACCEPTED {
            control.phase(Phase::Waiting)?;
            retry_wait(response.headers().get(RETRY_AFTER), attempt, cancel).await?;
            continue;
        }
        if matches!(response.status(), StatusCode::NOT_FOUND | StatusCode::GONE) {
            bail!("transfer is unavailable, expired, or has been revoked");
        }
        if !response.status().is_success() {
            bail!("metadata request failed: {}", response.status());
        }
        let transfer = response_json::<Api<Transfer>>(response, cancel, "metadata response body")
            .await?
            .data;
        return Ok(transfer);
    }
    bail!("transfer is still pending")
}
async fn capabilities(
    client: &Client,
    instance: &str,
    cancel: &CancellationToken,
) -> Result<DownloadCapabilities> {
    let response = tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), r = client.get(format!("{instance}/api/v1/info")).send() => r? };
    // Pre-capability servers are safe to use serially without ranges. A
    // successful but malformed capability document is never silently ignored.
    if matches!(
        response.status(),
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
    ) {
        return Ok(DownloadCapabilities {
            download_concurrency: Some(1),
            transfer_capabilities: Some(RangeCapabilities {
                download_ranges: false,
            }),
        });
    }
    if !response.status().is_success() {
        bail!("instance capability request failed: {}", response.status());
    }
    Ok(response_json::<Api<DownloadCapabilities>>(
        response,
        cancel,
        "instance capability response body",
    )
    .await?
    .data)
}
async fn response_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    cancel: &CancellationToken,
    label: &str,
) -> Result<T> {
    let body = tokio::select! {
        _ = cancel.cancelled() => bail!("transfer cancelled"),
        body = tokio::time::timeout(BODY_IDLE, response.bytes()) => body,
    }
    .with_context(|| format!("{label} timed out or could not be read"))??;
    serde_json::from_slice(&body).with_context(|| format!("invalid {label} JSON"))
}
fn async_client(control: &Control, cookie: Option<&str>) -> Result<Client> {
    let mut headers = HeaderMap::new();
    if let Some(cookie) = cookie {
        let mut value = HeaderValue::from_str(cookie).context("invalid session cookie")?;
        value.set_sensitive(true);
        headers.insert(COOKIE, value);
    }
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .user_agent(control.client_user_agent())
        .default_headers(headers)
        .build()
        .context("create HTTP client")
}

fn chunk_url(
    instance: &str,
    transfer: &str,
    item: &str,
    position: u64,
    source: &DownloadSource,
) -> String {
    match source {
        DownloadSource::Public => {
            format!("{instance}/api/v1/transfers/{transfer}/items/{item}/chunks/{position}")
        }
        DownloadSource::Inbox => {
            format!("{instance}/api/native/v1/inbox/{transfer}/items/{item}/chunks/{position}")
        }
    }
}
async fn retry_wait(
    value: Option<&reqwest::header::HeaderValue>,
    attempt: u32,
    cancel: &CancellationToken,
) -> Result<()> {
    let retry_after_ms = value
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v.saturating_mul(1000));
    let ms = retry_delay_ms(attempt, retry_after_ms, 0);
    tokio::select! { _ = cancel.cancelled() => bail!("transfer cancelled"), _ = tokio::time::sleep(Duration::from_millis(ms)) => Ok(()) }
}
fn cancellation_bridge(control: Control) -> CancellationToken {
    let token = CancellationToken::new();
    let watcher = token.clone();
    std::thread::spawn(move || {
        while !watcher.is_cancelled() {
            if control.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                watcher.cancel();
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    token
}
fn plaintext_len(size: u64, chunk: u64, index: u64) -> u64 {
    size.saturating_sub(index.saturating_mul(chunk)).min(chunk)
}
/// AEAD records encode plaintext followed by a fixed authentication tag. UI
/// progress is plaintext-only, while `wire` continues to count fresh bytes.
fn ciphertext_to_plain(plain: u64, cipher: u64, received: u64) -> u64 {
    received.min(cipher).min(plain)
}
fn cipher_path(base: &Path, item: usize, chunk: u64) -> PathBuf {
    base.join(format!("cipher-{item}-{chunk}"))
}
fn etag_path(base: &Path, item: usize, chunk: u64) -> PathBuf {
    base.join(format!("etag-{item}-{chunk}"))
}
fn plain_path(base: &Path, item: usize) -> PathBuf {
    base.join(format!("plain-{item}"))
}
fn write_plain(base: &Path, item: usize, offset: u64, plain: &[u8]) -> Result<()> {
    let path = plain_path(base, item);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    restrict(&path)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(plain)?;
    file.sync_all()?;
    Ok(())
}
fn authenticate_and_write(request: AuthenticateAndWriteRequest<'_>) -> Result<u64> {
    let AuthenticateAndWriteRequest {
        base,
        item_index,
        index,
        chunk_bytes,
        transfer_id,
        item,
        master,
        ciphertext,
        retain_ciphertext,
    } = request;
    let key = derive_item_key(master, transfer_id, &item.id)?;
    let plain = decrypt_chunk(
        &key,
        &decode(&item.nonce_prefix)?,
        index as u32,
        &ciphertext,
        aad(transfer_id, &item.id, &index.to_string()).as_bytes(),
    )
    .context("could not authenticate downloaded chunk")?;
    let expected = plaintext_len(item.size, chunk_bytes, index);
    if plain.len() as u64 != expected {
        bail!("authenticated chunk has an invalid plaintext length");
    }
    if retain_ciphertext {
        // Live transport has no HTTP prefix file. Retain the authenticated record
        // before the verified bit can be checkpointed, so restart can rebuild
        // plaintext instead of silently restarting the entire incomplete item.
        let target = cipher_path(base, item_index, index);
        let mut temporary = tempfile::NamedTempFile::new_in(base)?;
        temporary.write_all(&ciphertext)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&target)?;
        sync_parent(&target)?;
    }
    write_plain(base, item_index, index * chunk_bytes, &plain)?;
    Ok(expected)
}
fn restore_retained(
    store: &Store,
    item_index: usize,
    transfer_id: &str,
    chunk_bytes: u64,
    item: &SavedItem,
    index: u64,
    key: &[u8],
) -> Result<bool> {
    let cipher = match fs::read(cipher_path(store.path(), item_index, index)) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let item_key = derive_item_key(key, transfer_id, &item.id)?;
    let plain = match decrypt_chunk(
        &item_key,
        &decode(&item.nonce_prefix)?,
        index as u32,
        &cipher,
        aad(transfer_id, &item.id, &index.to_string()).as_bytes(),
    ) {
        Ok(plain) => plain,
        Err(_) => return Ok(false),
    };
    if plain.len() as u64 != plaintext_len(item.size, chunk_bytes, index) {
        return Ok(false);
    }
    write_plain(store.path(), item_index, index * chunk_bytes, &plain)?;
    Ok(true)
}
fn now_ms() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}
fn sha256_file(path: &Path) -> Result<String> {
    let mut input = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            return Ok(hex::encode(hash.finalize()));
        }
        hash.update(&buf[..n]);
    }
}
fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path.parent().context("download target has no parent")?)?.sync_all()?;
    }
    Ok(())
}
fn completed_paths(job: &DownloadJob) -> Result<Vec<String>> {
    job.checked()?;
    if job.items.iter().any(|item| !item.published) {
        bail!("completed download checkpoint has unpublished items");
    }
    job.items
        .iter()
        .map(|item| {
            if !item.target.is_file() || sha256_file(&item.target)? != item.digest {
                bail!("published download target is missing or corrupted");
            }
            Ok(item.target.display().to_string())
        })
        .collect()
}
fn validate_range(
    value: Option<&reqwest::header::HeaderValue>,
    start: u64,
    end: u64,
    total: u64,
) -> Result<()> {
    let value = value
        .context("range response lacks Content-Range")?
        .to_str()?;
    if value != format!("bytes {start}-{end}/{total}") {
        bail!("Content-Range does not exactly match continuation");
    }
    Ok(())
}
fn restrict(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_failure_does_not_advertise_an_empty_checkpoint_for_resume() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let instance = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream
                .write_all(
                    b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let home = tempfile::tempdir().unwrap();
        let control = Control::new(
            crate::control::TransferSettings {
                state_home: home.path().join("jobs"),
                max_concurrency: Some(1),
                memory_budget: 128 * 1024 * 1024,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            std::sync::mpsc::channel().0,
        );
        assert!(
            run(
                &instance,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                &home.path().join("output"),
                &control
            )
            .is_err()
        );
        control.cancel();
        server.join().unwrap();
        assert!(control.checkpoint_id().is_none());
        assert!(
            saved_transfers(&control.transfer_home())
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn live_records_recover_plaintext_after_process_death() {
        let transfer = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let home = tempfile::tempdir().unwrap();
        let store = Store::create(home.path(), &uuid::Uuid::new_v4().to_string()).unwrap();
        let master = [7; 32];
        let prefix = filebeam_encryption::generate_nonce_prefix().unwrap();
        let plain = b"verified live payload";
        let item = SavedItem {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAW".into(),
            name: "file".into(),
            size: plain.len() as u64,
            nonce_prefix: encode(&prefix),
            chunk_count: 1,
            digest: String::new(),
            target: home.path().join("output"),
            verified: vec![true],
            published: false,
        };
        let key = derive_item_key(&master, transfer, &item.id).unwrap();
        let cipher = filebeam_encryption::encrypt_chunk(
            &key,
            &prefix,
            0,
            plain,
            aad(transfer, &item.id, "0").as_bytes(),
        )
        .unwrap();
        authenticate_and_write(AuthenticateAndWriteRequest {
            base: store.path(),
            item_index: 0,
            index: 0,
            chunk_bytes: plain.len() as u64,
            transfer_id: transfer,
            item: &item,
            master: &master,
            ciphertext: cipher,
            retain_ciphertext: true,
        })
        .unwrap();
        fs::remove_file(plain_path(store.path(), 0)).unwrap();
        assert!(
            restore_retained(&store, 0, transfer, plain.len() as u64, &item, 0, &master).unwrap()
        );
        assert_eq!(fs::read(plain_path(store.path(), 0)).unwrap(), plain);
        fs::write(cipher_path(store.path(), 0, 0), b"corrupted").unwrap();
        assert!(
            !restore_retained(&store, 0, transfer, plain.len() as u64, &item, 0, &master).unwrap()
        );
    }
    #[test]
    fn continuation_range_is_exact() {
        assert!(validate_range(Some(&"bytes 5-19/20".parse().unwrap()), 5, 19, 20).is_ok());
        assert!(validate_range(Some(&"bytes 5-19/21".parse().unwrap()), 5, 19, 20).is_err());
    }

    #[test]
    fn turbo_descriptor_binds_server_item_identity_before_chunks_arrive() {
        let key = vec![7; 32];
        let prefix = filebeam_encryption::generate_nonce_prefix().unwrap();
        let descriptor = serde_json::json!({"version":1,"purpose":"turbo-descriptor","chunk_bytes":4,"items":[{
            "id":"item-a","name":"early.bin","type":"application/octet-stream","size":5,
            "nonce_prefix": URL_SAFE_NO_PAD.encode([3;16]),"chunk_count":2
        }]});
        let ciphertext = filebeam_encryption::encrypt_manifest(
            &key,
            &prefix,
            serde_json::to_string(&descriptor).unwrap().as_bytes(),
            b"filebeam:v1:transfer:descriptor:descriptor",
        )
        .unwrap();
        let envelope = serde_json::json!({"v":1,"nonce_prefix":URL_SAFE_NO_PAD.encode(prefix),"ciphertext":URL_SAFE_NO_PAD.encode(ciphertext)}).to_string();
        let transfer = Transfer {
            driver: "http".into(),
            id: "transfer".into(),
            protocol_version: 1,
            chunk_bytes: 4,
            encrypted_manifest: None,
            encrypted_descriptor: Some(envelope.clone()),
            items: vec![MetadataItem {
                id: "item-a".into(),
                position: 0,
                chunk_count: 2,
            }],
            download_concurrency: None,
            transfer_capabilities: TransferCapabilities::default(),
        };
        assert_eq!(
            turbo_descriptor_for(
                &key,
                "transfer",
                &envelope,
                &transfer,
                &Control::test_factory()
            )
            .unwrap()
            .items[0]
                .id,
            "item-a"
        );
        let mut rebound = transfer;
        rebound.items[0].id = "other".into();
        assert!(
            turbo_descriptor_for(
                &key,
                "transfer",
                &envelope,
                &rebound,
                &Control::test_factory()
            )
            .is_err()
        );
    }
    #[test]
    fn checkpoint_requires_complete_bitmap() {
        let job = DownloadJob {
            version: 1,
            id: "x".into(),
            direction: "download".into(),
            state: "running".into(),
            instance: "https://x".into(),
            done: 0,
            total: 1,
            transfer_id: "X".into(),
            chunk_bytes: 1,
            server_concurrency: 1,
            ranges: false,
            driver: "http".into(),
            envelope: "x".into(),
            source: DownloadSource::Public,
            turbo: None,
            output: PathBuf::new(),
            items: vec![SavedItem {
                id: "x".into(),
                name: "x".into(),
                size: 1,
                nonce_prefix: "x".into(),
                chunk_count: 1,
                digest: "x".into(),
                target: PathBuf::new(),
                verified: vec![],
                published: false,
            }],
        };
        assert!(job.checked().is_err());
    }

    #[test]
    fn inbox_checkpoint_cannot_resume_without_host_reauthentication() {
        let root = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let store = Store::create(root.path(), &id).unwrap();
        let output = root.path().join("output");
        std::fs::create_dir(&output).unwrap();
        store
            .save(&DownloadJob {
                version: VERSION,
                id: id.clone(),
                direction: "download".into(),
                state: "paused".into(),
                instance: "https://filebeam.example".into(),
                done: 0,
                total: 1,
                transfer_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                chunk_bytes: 1,
                server_concurrency: 1,
                ranges: false,
                driver: "http".into(),
                envelope: "opaque".into(),
                source: DownloadSource::Inbox,
                turbo: None,
                output: output.clone(),
                items: vec![SavedItem {
                    id: "item".into(),
                    name: "file".into(),
                    size: 1,
                    nonce_prefix: "x".into(),
                    chunk_count: 1,
                    digest: "x".into(),
                    target: output.join("file"),
                    verified: vec![false],
                    published: false,
                }],
            })
            .unwrap();
        drop(store);
        let store = Store::open(root.path(), &id).unwrap();
        assert!(
            resume(store, &Control::test_factory())
                .unwrap_err()
                .to_string()
                .contains("requires reauthentication")
        );
    }

    #[test]
    fn live_note_is_rejected_before_receiver_registration() {
        let transfer = Transfer {
            driver: "webrtc".into(),
            id: "X".into(),
            protocol_version: 1,
            chunk_bytes: 1,
            encrypted_manifest: Some("envelope".into()),
            encrypted_descriptor: None,
            items: Vec::new(),
            download_concurrency: None,
            transfer_capabilities: TransferCapabilities::default(),
        };
        assert!(reject_live_note(&transfer).is_err());
    }

    #[test]
    fn sha256_file_matches_expected_digest() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"filebeam").unwrap();
        assert_eq!(
            sha256_file(file.path()).unwrap(),
            "b242789cafff94acb0d6267c3910a3f5b6748584bc8f2b5b29236296c65c995b"
        );
    }

    #[test]
    fn receive_progress_keeps_payload_separate_from_verified_bytes() {
        let mut progress = ReceiveProgress::new(10, vec![2, 4]);
        assert_eq!(progress.report(0, 0, 5), (15, 7));
        assert_eq!(progress.report(1, 0, 3), (18, 7));
        assert_eq!(progress.restart(0, 0), (13, 2));
        assert_eq!(progress.verified(1, 0, 3), (13, 7));
        assert_eq!(progress.verified(0, 0, 5), (18, 7));
    }

    #[test]
    fn receive_progress_keeps_equal_chunk_indices_separate() {
        let mut progress = ReceiveProgress::new(0, vec![0, 0]);
        progress.report(0, 0, 3);
        assert_eq!(progress.report(1, 0, 5), (8, 5));
        assert_eq!(progress.verified(0, 0, 3), (8, 3));
        assert_eq!(progress.verified(1, 0, 5), (8, 5));
    }

    #[test]
    fn ciphertext_progress_excludes_aead_tag_for_empty_and_tail_chunks() {
        assert_eq!(ciphertext_to_plain(0, TAG_BYTES, TAG_BYTES), 0);
        assert_eq!(ciphertext_to_plain(3, 3 + TAG_BYTES, 2), 2);
        assert_eq!(ciphertext_to_plain(3, 3 + TAG_BYTES, 3), 3);
        assert_eq!(ciphertext_to_plain(3, 3 + TAG_BYTES, 3 + TAG_BYTES), 3);
    }

    #[test]
    fn publish_does_not_clobber_existing_different_data() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("report.txt");
        let source = plain_path(directory.path(), 0);
        fs::write(&source, b"verified").unwrap();
        fs::write(&target, b"other").unwrap();
        let item = SavedItem {
            id: "item".into(),
            name: "report.txt".into(),
            size: 8,
            nonce_prefix: "nonce".into(),
            chunk_count: 1,
            digest: sha256_file(&source).unwrap(),
            target: target.clone(),
            verified: vec![true],
            published: false,
        };
        assert!(publish_item(directory.path(), &item, 0).is_err());
        assert_eq!(fs::read(target).unwrap(), b"other");
    }
}
