//! Restart-safe asynchronous HTTP upload implementation.
//!
//! Ciphertext is checkpointed one chunk at a time before it is handed to active
//! network workers. This keeps restart safety without making a full local spool
//! a prerequisite for the first PUT.

use std::{
    collections::{HashMap, HashSet},
    fs::{self},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use crate::checkpoint::Store;
use crate::http::{ResponseLimits, read_limited, request_headers};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use filebeam_encryption::{
    derive_item_key, derive_password_key, derive_password_protected_key, encrypt_chunk,
    encrypt_manifest, generate_nonce_prefix, generate_transfer_key,
};
use filebeam_transfer::{
    AdaptiveConcurrency, StageAction, StageSession, StageStatus, TransferMemoryBudget,
    UploadTransport, concurrency_limit_with_memory, retry_delay_ms, retryable_status,
};
use futures_util::stream;
use reqwest::{
    Body, Client, StatusCode,
    header::{
        AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderName, HeaderValue, RETRY_AFTER,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc2822};
use tokio::{io::AsyncReadExt, sync::watch, task::JoinSet};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    control::{Control, PeerFailed, Phase, TransferEvent},
    source::{self, SourceSpec, UploadSource},
    uploads::{self, DirectoryMode},
    webrtc::{self, Signaling},
};

#[path = "rtc_upload.rs"]
mod rtc_upload;
use rtc_upload::{CiphertextArtifact, serve_artifacts};

const TAG_BYTES: u64 = 16;
const JSON_LIMIT: usize = 2 * 1024 * 1024;
const RESPONSE_DEADLINE: Duration = Duration::from_secs(60);
const CONTROL_BODY_IDLE: Duration = Duration::from_secs(15);
const UPLOAD_BODY_IDLE: Duration = Duration::from_secs(120);
const UPLOAD_ACK_HEADERS: Duration = Duration::from_secs(120);
const MEMORY_OVERHEAD_BYTES: u64 = 8 * 1024 * 1024;
const BODY_BUFFER_BYTES: usize = 64 * 1024;
const WEBRTC_POLL: Duration = Duration::from_secs(2);
const WEBRTC_MAX_PEERS: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct UploadJob {
    pub version: u8,
    pub id: String,
    pub direction: String,
    pub state: String,
    pub instance: String,
    pub done: u64,
    pub total: u64,
    transfer_id: Option<String>,
    upload_token: Option<String>,
    share_url: Option<String>,
    delete_token: Option<String>,
    share_key: Vec<u8>,
    password_salt: Option<String>,
    #[serde(skip, default)]
    master_key: Vec<u8>,
    chunk_bytes: u64,
    server_concurrency: u32,
    upload_status: bool,
    #[serde(default)]
    turbo: bool,
    #[serde(default)]
    descriptor_published: bool,
    #[serde(default)]
    encrypted_descriptor: Option<String>,
    transport: Option<UploadTransport>,
    #[serde(default = "http_driver")]
    driver: String,
    #[serde(default)]
    join_token: Option<String>,
    exact_manifest: Option<String>,
    receipt: Option<String>,
    recipient: Option<Recipient>,
    items: Vec<Item>,
    chunks: Vec<Chunk>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Item {
    id: String,
    position: u64,
    name: String,
    source: Source,
    nonce_prefix: String,
    digest: String,
    bytes: u64,
    chunk_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Source {
    spec: SourceSpec,
    bytes: u64,
    modified_ns: u128,
    digest: String,
    #[serde(default)]
    chunk_digests: Vec<String>,
}

enum Inputs<'a> {
    Paths(&'a [PathBuf]),
    Sources(&'a [UploadSource]),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Chunk {
    item: usize,
    item_id: String,
    position: u64,
    plaintext_bytes: u64,
    ciphertext_bytes: u64,
    checksum: String,
    artifact: PathBuf,
    complete: bool,
    stage: Option<StageSession>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Recipient {
    username: String,
    user_id: u64,
    account_key_bundle_id: u64,
    public_key: String,
    encrypted_key: Option<String>,
}

#[derive(Serialize)]
struct TurboDescriptor<'a> {
    version: u8,
    purpose: &'a str,
    chunk_bytes: u64,
    items: Vec<TurboDescriptorItem<'a>>,
}

#[derive(Serialize)]
struct TurboDescriptorItem<'a> {
    id: &'a str,
    name: &'a str,
    #[serde(rename = "type")]
    mime: &'a str,
    size: u64,
    nonce_prefix: &'a str,
    chunk_count: u64,
}

struct PreparedCiphertext {
    item: usize,
    position: u64,
    plaintext_bytes: u64,
    ciphertext: Vec<u8>,
}

struct PreparedInput {
    source: Source,
    item: usize,
    item_id: String,
    position: u64,
    chunk_bytes: u64,
    prefix: Vec<u8>,
    key: Vec<u8>,
    transfer: String,
    control: Control,
}

struct UploadContext {
    client: Client,
    instance: String,
    transfer: String,
    token: String,
    policy: Option<UploadTransport>,
    adaptive: Arc<Mutex<AdaptiveConcurrency>>,
    sending: Arc<Mutex<SendProgress>>,
    control: Control,
    store: Arc<Store>,
}

/// Tracks bytes handed to active HTTP request bodies. `done` can include this
/// observed progress, while the base moves only after the parent checkpoint is
/// durable. Entries are removed on acknowledgement, bounding this map by the
/// upload scheduler's active window.
struct SendProgress {
    base_done: u64,
    active: HashMap<(usize, u64), u64>,
}

impl SendProgress {
    fn new(base_done: u64) -> Self {
        Self {
            base_done,
            active: HashMap::new(),
        }
    }

    fn restart(&mut self, chunk: &Chunk) -> u64 {
        self.active.insert(chunk_key(chunk), 0);
        self.snapshot()
    }

    fn report(
        &mut self,
        key: (usize, u64),
        plaintext_bytes: u64,
        ciphertext_bytes: u64,
        sent: u64,
    ) -> u64 {
        // AEAD appends its tag, so only the ciphertext prefix before it maps to
        // user-visible plaintext progress.
        self.active
            .insert(key, sent.min(ciphertext_bytes).min(plaintext_bytes));
        self.snapshot()
    }

    fn acknowledged(&mut self, chunk: &Chunk) -> u64 {
        self.active.remove(&chunk_key(chunk));
        self.base_done = self.base_done.saturating_add(chunk.plaintext_bytes);
        self.snapshot()
    }

    fn snapshot(&self) -> u64 {
        self.base_done
            .saturating_add(self.active.values().copied().sum::<u64>())
    }
}

fn chunk_key(chunk: &Chunk) -> (usize, u64) {
    (chunk.item, chunk.position)
}

struct DirectRequest<'a> {
    context: &'a UploadContext,
    chunk: &'a Chunk,
    policy: Option<&'a UploadTransport>,
}

struct StageRequest<'a> {
    context: &'a UploadContext,
    chunk: Chunk,
    body: Vec<u8>,
    policy: &'a UploadTransport,
}

#[derive(Deserialize)]
struct Api<T> {
    data: T,
}
#[derive(Deserialize)]
struct Info {
    anonymous_uploads_enabled: bool,
    enabled_drivers: Vec<String>,
    chunk_bytes: u64,
    file_retention_hours: u64,
    #[serde(default)]
    file_retention_options: Vec<u64>,
    maximum_transfer_bytes: Option<u64>,
    maximum_file_count: Option<usize>,
    #[serde(default)]
    transport_limits: HashMap<String, super::DriverLimits>,
    #[serde(default)]
    upload_concurrency: Option<u32>,
    #[serde(default)]
    transfer_capabilities: Capabilities,
}
#[derive(Default, Deserialize)]
struct Capabilities {
    #[serde(default)]
    upload_status: bool,
}
#[derive(Serialize)]
struct Create<'a> {
    kind: &'a str,
    driver: &'a str,
    protocol_version: u8,
    chunk_bytes: u64,
    retention_hours: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipient_username: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_key_bundle_id: Option<u64>,
    items: Vec<CreateItem>,
}
#[derive(Serialize)]
struct CreateItem {
    ciphertext_bytes: u64,
    chunk_count: u64,
}
#[derive(Deserialize)]
struct Created {
    id: String,
    share_url: String,
    chunk_bytes: u64,
    #[serde(default)]
    upload_transport: Option<UploadTransport>,
    items: Vec<CreatedItem>,
    upload_token: String,
    delete_token: String,
    #[serde(default)]
    driver: String,
    #[serde(default)]
    join_token: Option<String>,
}
#[derive(Deserialize)]
struct CreatedItem {
    id: String,
    position: u64,
}
#[derive(Deserialize)]
struct StatusPage {
    status: String,
    protocol_version: u8,
    chunk_bytes: u64,
    #[serde(default)]
    upload_transport: Option<UploadTransport>,
    chunks: Vec<PublishedChunk>,
    next_cursor: Option<String>,
}
#[derive(Deserialize)]
struct PublishedChunk {
    item_id: String,
    position: u64,
    ciphertext_bytes: u64,
    checksum: String,
}

pub(super) fn run(
    instance: &str,
    paths: &[PathBuf],
    mode: DirectoryMode,
    options: super::UploadOptions,
    control: &Control,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let store = control.create_checkpoint_store(&id)?;
    crate::runtime::shared_tokio_runtime().block_on(run_new(
        instance,
        Inputs::Paths(paths),
        mode,
        control,
        Arc::new(store),
        "http",
        options,
    ))
}

/// Creates a live, peer-served transfer. The caller owns transport selection;
/// this entry point intentionally shares the HTTP checkpoint format so an
/// interrupted sender can resume from its authenticated ciphertext artifacts.
pub(super) fn run_webrtc(
    instance: &str,
    paths: &[PathBuf],
    mode: DirectoryMode,
    options: super::UploadOptions,
    control: &Control,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let store = control.create_checkpoint_store(&id)?;
    crate::runtime::shared_tokio_runtime().block_on(run_new(
        instance,
        Inputs::Paths(paths),
        mode,
        control,
        Arc::new(store),
        "webrtc",
        options,
    ))
}

pub(super) fn run_sources(
    instance: &str,
    sources: &[UploadSource],
    mode: DirectoryMode,
    options: super::UploadOptions,
    control: &Control,
) -> Result<String> {
    if sources.is_empty() {
        bail!("Select at least one source");
    }
    let id = Uuid::new_v4().to_string();
    let store = control.create_checkpoint_store(&id)?;
    crate::runtime::shared_tokio_runtime().block_on(run_new(
        instance,
        Inputs::Sources(sources),
        mode,
        control,
        Arc::new(store),
        match options.transport {
            super::Transport::Http => "http",
            super::Transport::WebRtc => "webrtc",
        },
        options,
    ))
}

pub(super) fn resume(store: Store, control: &Control) -> Result<Vec<String>> {
    crate::runtime::shared_tokio_runtime().block_on(async move {
        let job = store
            .load::<UploadJob>()?
            .context("saved upload checkpoint is empty")?;
        let url = continue_job(job, Arc::new(store), control).await?;
        Ok(vec![url])
    })
}

async fn run_new(
    instance: &str,
    inputs: Inputs<'_>,
    mode: DirectoryMode,
    control: &Control,
    store: Arc<Store>,
    driver: &str,
    options: super::UploadOptions,
) -> Result<String> {
    control.phase(Phase::Connecting)?;
    if options.recipient.is_some() && (driver != "http" || options.password) {
        bail!("inbox delivery requires HTTP without password protection");
    }
    let client = client(control, &options.authentication)?;
    let info: Info = json(
        control,
        request(control, client.get(format!("{instance}/api/v1/info"))).await?,
    )
    .await?;
    if (!info.anonymous_uploads_enabled
        && matches!(
            &options.authentication,
            super::UploadAuthentication::Anonymous
        ))
        || !info.enabled_drivers.iter().any(|enabled| enabled == driver)
    {
        bail!("this instance does not allow {driver} uploads with this authentication");
    }
    if info.chunk_bytes == 0 || info.chunk_bytes > 24_999_984 {
        bail!("server supplied an invalid chunk size");
    }
    let retention_hours = select_retention(
        info.file_retention_hours,
        &info.file_retention_options,
        options.retention_hours,
    )?;
    let limits = filebeam_transfer::capabilities::select_driver_limits(
        driver,
        &info.transport_limits,
        info.maximum_transfer_bytes,
        info.maximum_file_count,
    );
    let mut prepared = match inputs {
        Inputs::Paths(paths) => uploads::prepare(paths, mode, limits.maximum_file_count, control)?,
        Inputs::Sources(sources) => {
            uploads::Prepared::from_sources(sources, mode, limits.maximum_file_count, control)?
        }
    };
    prepared.retain_archive(store.path())?;
    // Validate declared sizes before reading potentially huge sources.
    let metadata = prepared
        .files
        .iter()
        .map(|file| source_metadata(&file.spec, control).map(|source| (file.name.clone(), source)))
        .collect::<Result<Vec<_>>>()?;
    let total = metadata.iter().try_fold(0u64, |n, (_, source)| {
        n.checked_add(source.bytes)
            .ok_or_else(|| anyhow::anyhow!("selected files are too large"))
    })?;
    let declared = metadata.iter().try_fold(0u64, |n, (_, source)| {
        n.checked_add(ciphertext_bytes(source.bytes, info.chunk_bytes)?)
            .ok_or_else(|| anyhow::anyhow!("selected files are too large"))
    })?;
    if limits
        .maximum_transfer_bytes
        .is_some_and(|limit| declared > limit)
    {
        bail!("the encrypted transfer exceeds this instance's size limit");
    }
    let sources = prepared
        .files
        .iter()
        .map(|file| {
            fingerprint(&file.spec, info.chunk_bytes, &control.cancelled, control)
                .map(|source| (file.name.clone(), source))
        })
        .collect::<Result<Vec<_>>>()?;
    control.totals(total, sources.len());
    let mut job = UploadJob {
        version: 1,
        id: store
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        direction: "upload".into(),
        state: "reserving".into(),
        instance: instance.trim_end_matches('/').into(),
        done: 0,
        total,
        transfer_id: None,
        upload_token: None,
        share_url: None,
        delete_token: None,
        share_key: generate_transfer_key()?.to_vec(),
        password_salt: if options.password {
            Some(encode(&generate_nonce_prefix()?))
        } else {
            None
        },
        master_key: Vec::new(),
        chunk_bytes: info.chunk_bytes,
        server_concurrency: info.upload_concurrency.unwrap_or(8).clamp(1, 8),
        upload_status: info.transfer_capabilities.upload_status,
        turbo: options.turbo,
        descriptor_published: false,
        encrypted_descriptor: None,
        transport: None,
        driver: driver.into(),
        join_token: None,
        exact_manifest: None,
        receipt: None,
        recipient: options.recipient.as_ref().map(|recipient| Recipient {
            username: recipient.username.clone(),
            user_id: recipient.user_id,
            account_key_bundle_id: recipient.account_key_bundle_id,
            public_key: recipient.public_key.clone(),
            encrypted_key: None,
        }),
        items: Vec::new(),
        chunks: Vec::new(),
    };
    store.save(&job)?; // The job exists before the non-idempotent reservation request.
    let create = Create {
        kind: "files",
        driver,
        protocol_version: 1,
        chunk_bytes: info.chunk_bytes,
        retention_hours,
        recipient_username: job
            .recipient
            .as_ref()
            .map(|recipient| recipient.username.as_str()),
        account_key_bundle_id: job
            .recipient
            .as_ref()
            .map(|recipient| recipient.account_key_bundle_id),
        items: sources
            .iter()
            .map(|(_, s)| {
                Ok(CreateItem {
                    ciphertext_bytes: ciphertext_bytes(s.bytes, info.chunk_bytes)?,
                    chunk_count: chunk_count(s.bytes, info.chunk_bytes)?,
                })
            })
            .collect::<Result<_>>()?,
    };
    let created: Created = json(
        control,
        request(
            control,
            client
                .post(format!("{}/api/v1/transfers", job.instance))
                .json(&create),
        )
        .await?,
    )
    .await?;
    if created.driver != driver
        || created.chunk_bytes != job.chunk_bytes
        || created.items.len() != sources.len()
        || created
            .items
            .iter()
            .enumerate()
            .any(|(i, item)| item.position != i as u64)
    {
        bail!("server transfer reservation is invalid");
    }
    if let Some(transport) = &created.upload_transport {
        transport.validate().map_err(anyhow::Error::msg)?;
    }
    job.transfer_id = Some(created.id.clone());
    job.upload_token = Some(created.upload_token);
    job.delete_token = Some(created.delete_token);
    job.share_url = Some(created.share_url);
    job.transport = created.upload_transport;
    job.join_token = created.join_token;
    if let Some(recipient) = &mut job.recipient {
        let public_key =
            decode(&recipient.public_key).context("recipient public key is invalid")?;
        let aad = format!(
            "filebeam:recipient:v1:{}:{}:{}",
            created.id, recipient.user_id, recipient.account_key_bundle_id
        );
        recipient.encrypted_key = Some(encode(&filebeam_encryption::seal_key_for_recipient(
            &public_key,
            &job.share_key,
            aad.as_bytes(),
        )?));
    }
    if job.driver == "webrtc" && job.join_token.as_deref().unwrap_or_default().is_empty() {
        bail!("server did not provide a live transfer join token");
    }
    for ((name, source), server) in sources.into_iter().zip(created.items) {
        let bytes = source.bytes;
        let digest = source.digest.clone();
        job.items.push(Item {
            id: server.id,
            position: server.position,
            name,
            source,
            nonce_prefix: encode(&generate_nonce_prefix()?),
            // The reservation follows a complete preflight fingerprint. The
            // producer verifies it again before it adopts or emits any chunk.
            digest,
            bytes,
            chunk_count: chunk_count(bytes, job.chunk_bytes)?,
        });
    }
    store.save(&job)?; // Reservation credentials and nonce material are durable before encryption.
    job.state = "preparing".into();
    store.save(&job)?;
    control.set_checkpoint_id(job.id.clone());
    // Keep the prepared ZIP owner alive while the producer reads it.
    let _prepared = prepared;
    continue_job(job, store, control).await
}

async fn continue_job(mut job: UploadJob, store: Arc<Store>, control: &Control) -> Result<String> {
    validate_job(&job)?;
    if job.state == "complete" {
        let url = job
            .receipt
            .context("completed upload receipt is unavailable");
        if let Ok(url) = &url {
            control.emit(TransferEvent::ShareReady(crate::control::ShareReady {
                share_url: url.clone(),
            }));
        }
        return url;
    }
    if job.state == "ended" {
        return receipt(&job);
    }
    unlock_job(&mut job, control)?;
    // Existing fully-spooled checkpoints remain resumable without their ZIP
    // tempfile. Interleaved checkpoints must prove source identity before any
    // missing record is prepared or an orphan is adopted.
    let fully_prepared = preparation_complete(&job)?;
    if !fully_prepared {
        validate_sources(&job, control)?;
    }
    let transfer = job
        .transfer_id
        .clone()
        .context("upload was not reserved yet")?;
    let token = job
        .upload_token
        .clone()
        .context("upload token is unavailable")?;
    let client = client(control, &super::UploadAuthentication::Anonymous)?;
    if job.turbo && !job.descriptor_published {
        if job.encrypted_descriptor.is_none() {
            job.encrypted_descriptor = Some(turbo_descriptor(&job, &transfer, control)?);
            // The exact idempotency value must survive a lost successful PUT.
            store.save(&job)?;
        }
        publish_turbo_descriptor(&mut job, &client, &transfer, &token, control).await?;
        store.save(&job)?;
        control.emit(TransferEvent::ShareReady(crate::control::ShareReady {
            share_url: receipt(&job)?,
        }));
    }
    if job.driver == "http" && job.upload_status {
        reconcile(&mut job, &client, &transfer, &token, control).await?;
        store.save(&job)?;
        for chunk in job.chunks.iter().filter(|chunk| chunk.complete) {
            store.remove_named(&stage_name(chunk))?;
        }
    }
    control.totals(job.total, job.items.len());
    control.advance(job.done, 0, 0);
    control.commit(job.done);
    control.phase(Phase::Sending)?;
    if job.driver == "webrtc" {
        prepare_live_ciphertext(&mut job, &store, control).await?;
        if job.state == "preparing" {
            validate_sources(&job, control)?;
            for item in &mut job.items {
                item.digest = item.source.digest.clone();
                item.bytes = item.source.bytes;
                item.chunk_count = chunk_count(item.source.bytes, job.chunk_bytes)?;
            }
            job.state = "sending".into();
            store.save(&job)?;
        }
        if job.state == "sending" {
            if job.exact_manifest.is_none() {
                job.exact_manifest = Some(encrypted_manifest(&job, control)?);
                store.save(&job)?;
            }
            let manifest = job
                .exact_manifest
                .as_deref()
                .context("live upload manifest is unavailable")?;
            Signaling::new(client.clone(), &job.instance, &transfer)?
                .publish(&token, manifest)
                .await?;
            job.state = "serving".into();
            store.save(&job)?;
        }
        if !control.webrtc_relay_only() {
            control.request_peer_consent(job.instance.clone())?;
        }
        control.emit(crate::control::TransferEvent::ShareReady(
            crate::control::ShareReady {
                share_url: receipt(&job)?,
            },
        ));
        return serve_live(&job, client, control).await;
    }
    let configured = control
        .max_concurrency()
        .unwrap_or(4)
        .min(job.server_concurrency);
    // A producer holds plaintext and ciphertext simultaneously. Reserve that
    // pair plus scheduler overhead before assigning the same two-copy budget
    // to active network/staging slots.
    let producer_bytes = job.chunk_bytes.saturating_mul(2).saturating_add(TAG_BYTES);
    let memory = TransferMemoryBudget::new(
        control.memory_budget(),
        MEMORY_OVERHEAD_BYTES.saturating_add(producer_bytes),
        1,
        1,
    );
    let maximum = concurrency_limit_with_memory(configured, job.chunk_bytes, memory, 8);
    let producer_can_overlap = memory
        .slot_bytes(job.chunk_bytes)
        .is_some_and(|slot| memory.fixed_overhead_bytes.saturating_add(slot) <= memory.total_bytes);
    let adaptive = Arc::new(Mutex::new(AdaptiveConcurrency::new(maximum)));
    let sending = Arc::new(Mutex::new(SendProgress::new(job.done)));
    let mut active = JoinSet::new();
    let mut active_indices = HashSet::new();
    let mut producer = None;
    loop {
        control.check()?;
        let limit = adaptive.lock().unwrap().limit() as usize;
        while producer.is_none() && active.len() < limit {
            let index = job.chunks.iter().enumerate().find_map(|(index, chunk)| {
                (!chunk.complete && !active_indices.contains(&index)).then_some(index)
            });
            let Some(index) = index else { break };
            active_indices.insert(index);
            let chunk = job.chunks[index].clone();
            let context = UploadContext {
                client: client.clone(),
                instance: job.instance.clone(),
                transfer: transfer.clone(),
                token: token.clone(),
                policy: job.transport.clone(),
                adaptive: adaptive.clone(),
                sending: sending.clone(),
                control: (*control).clone(),
                store: store.clone(),
            };
            active.spawn(async move { (index, upload_chunk(context, chunk).await) });
        }
        // One producer result may wait behind the active window. That is the
        // only ready ciphertext kept in memory, and lets encryption overlap a
        // saturated network window instead of rebuilding a full spool.
        let ready = job
            .chunks
            .iter()
            .enumerate()
            .any(|(index, chunk)| !chunk.complete && !active_indices.contains(&index));
        if producer.is_none()
            && (producer_can_overlap || active.is_empty())
            && !ready
            && let Some((item, position)) = next_missing_chunk(&job)?
        {
            let source = job.items[item].source.clone();
            let item_id = job.items[item].id.clone();
            let prefix = decode(&job.items[item].nonce_prefix)?;
            let key = derive_item_key(&job.master_key, &transfer, &item_id)?;
            if active.is_empty() {
                control.phase(Phase::Encrypting)?;
            }
            let producer_transfer = transfer.clone();
            let producer_control = (*control).clone();
            let producer_chunk_bytes = job.chunk_bytes;
            producer = Some(tokio::task::spawn_blocking(move || {
                prepare_one(PreparedInput {
                    source,
                    item,
                    item_id,
                    position,
                    chunk_bytes: producer_chunk_bytes,
                    prefix,
                    key,
                    transfer: producer_transfer,
                    control: producer_control,
                })
            }));
        }
        if active.is_empty() && producer.is_none() {
            break;
        }
        tokio::select! {
            _ = cancelled(control) => bail!("Transfer cancelled"),
            result = turbo_heartbeat(&client, &job.instance, &transfer, &token), if job.turbo => {
                result?;
            }
            result = active.join_next(), if !active.is_empty() => {
                let (index, result) = result.context("upload worker ended unexpectedly")??;
                active_indices.remove(&index);
                match result {
                Ok(chunk) => {
                    job.chunks[index] = chunk;
                    job.chunks[index].complete = true;
                    job.done = job
                        .chunks
                        .iter()
                        .filter(|c| c.complete)
                        .map(|c| c.plaintext_bytes)
                        .sum();
                    store.save(&job)?;
                    let display_done = sending.lock().unwrap().acknowledged(&job.chunks[index]);
                    control.advance(display_done, 0, 0);
                    control.commit(job.done);
                    // The parent completion checkpoint is durable before the
                    // sidecar is removed, so SIGKILL cannot lose a stage ID.
                    store.remove_named(&stage_name(&job.chunks[index]))?;
                    // Completion is durable before this immutable retry input is removed.
                    let _ = fs::remove_file(&job.chunks[index].artifact);
                }
                Err(error) => return Err(error),
                }
            }
            // `select!` builds disabled futures too; defer the Option access
            // into the async body so an active upload cannot panic here.
            result = async { producer.as_mut().expect("producer guard").await }, if producer.is_some() => {
                let prepared = result.context("ciphertext producer ended unexpectedly")??;
                let name = format!("chunk-{}-{}", prepared.item, prepared.position);
                let path = store.path().join(&name);
                let checksum = hex::encode(Sha256::digest(&prepared.ciphertext));
                if path.exists() {
                    // Artifact-before-checkpoint crashes are safe to adopt only
                    // after source preflight and exact deterministic ciphertext verification.
                    let existing = fs::read(&path)?;
                    if existing != prepared.ciphertext { bail!("orphaned ciphertext conflicts with the verified source"); }
                } else {
                    store.persist_immutable(&name, &prepared.ciphertext)?;
                }
                job.chunks.push(Chunk { item: prepared.item, item_id: job.items[prepared.item].id.clone(), position: prepared.position, plaintext_bytes: prepared.plaintext_bytes, ciphertext_bytes: prepared.ciphertext.len() as u64, checksum, artifact: path, complete: false, stage: None });
                store.save(&job)?;
                producer = None;
            }
        }
    }
    if job.state == "preparing" {
        if !preparation_complete(&job)? {
            bail!("ciphertext producer stopped before all chunks were durable");
        }
        validate_sources(&job, control)?;
        for item in &mut job.items {
            // The manifest digest must be the preflight full-file digest, not a
            // digest assembled from independently scheduled chunk reads.
            item.digest = item.source.digest.clone();
            item.bytes = item.source.bytes;
            item.chunk_count = chunk_count(item.source.bytes, job.chunk_bytes)?;
        }
        job.state = "sending".into();
        store.save(&job)?;
    }
    if job.state != "finalizing" {
        job.state = "finalizing".into();
        job.exact_manifest = Some(encrypted_manifest(&job, control)?);
        store.save(&job)?; // Exact envelope is persisted before its idempotent request.
    }
    control.phase(Phase::Finalizing)?;
    complete_transfer(
        &client,
        &job.instance,
        &transfer,
        &token,
        job.exact_manifest
            .as_deref()
            .context("final upload manifest is unavailable")?,
        job.recipient
            .as_ref()
            .and_then(|recipient| recipient.encrypted_key.as_deref()),
        control,
    )
    .await?;
    let url = receipt(&job)?;
    job.state = "complete".into();
    job.receipt = Some(url.clone());
    job.master_key.zeroize();
    job.upload_token = None;
    job.exact_manifest = None;
    store.save(&job)?;
    let archive = store.path().join("source.zip");
    if archive.exists() {
        fs::remove_file(archive).context("remove completed upload archive")?;
    }
    control.emit(TransferEvent::ShareReady(crate::control::ShareReady {
        share_url: url.clone(),
    }));
    Ok(url)
}

pub(super) fn revoke(store: Store, control: &Control) -> Result<()> {
    crate::runtime::shared_tokio_runtime().block_on(async move {
        let job = store
            .load::<UploadJob>()?
            .context("saved upload checkpoint is empty")?;
        validate_job(&job)?;
        let transfer = job.transfer_id.context("upload was not reserved yet")?;
        let token = job.delete_token.context("delete token is unavailable")?;
        let response = request(
            control,
            client(control, &super::UploadAuthentication::Anonymous)?
                .delete(format!("{}/api/v1/transfers/{transfer}", job.instance))
                .header("X-Filebeam-Delete-Token", token),
        )
        .await?;
        if response.status() != StatusCode::ACCEPTED {
            bail!("could not revoke transfer: {}", response.status());
        }
        let path = store.path().to_path_buf();
        let discarded =
            control
                .transfer_home()
                .join(format!(".{}.revoked-{}", job.id, Uuid::new_v4()));
        fs::rename(&path, &discarded).with_context(|| format!("discard {}", path.display()))?;
        drop(store);
        fs::remove_dir_all(&discarded).with_context(|| format!("discard {}", discarded.display()))
    })
}

pub(super) fn end_live(store: Store, control: &Control) -> Result<()> {
    crate::runtime::shared_tokio_runtime().block_on(async move {
        let mut job = store
            .load::<UploadJob>()?
            .context("saved upload checkpoint is empty")?;
        validate_job(&job)?;
        if job.driver != "webrtc" {
            bail!("saved transfer is not a live share");
        }
        let transfer = job
            .transfer_id
            .clone()
            .context("upload was not reserved yet")?;
        let token = job
            .upload_token
            .clone()
            .context("upload token is unavailable")?;
        Signaling::new(
            client(control, &super::UploadAuthentication::Anonymous)?,
            &job.instance,
            &transfer,
        )?
        .end(&token)
        .await?;
        job.state = "ended".into();
        store.save(&job)
    })
}

fn unlock_job(job: &mut UploadJob, control: &Control) -> Result<()> {
    if job.master_key.len() == 32 {
        return Ok(());
    }
    if job.share_key.len() != 32 {
        bail!("saved upload has an invalid share key");
    }
    if let Some(salt) = &job.password_salt {
        let password = control.secret(crate::control::SecretKind::Password)?;
        if password.chars().count() < 8 {
            bail!("passwords must contain at least 8 characters");
        }
        let _kdf_memory = control.reserve_memory(super::PASSWORD_KDF_BYTES)?;
        let mut password_key =
            derive_password_key(password.as_bytes(), &decode(salt)?, 65_536, 3, 1)?;
        job.master_key = derive_password_protected_key(&job.share_key, &password_key)?;
        password_key.zeroize();
    } else {
        job.master_key = job.share_key.clone();
    }
    Ok(())
}

async fn prepare_live_ciphertext(
    job: &mut UploadJob,
    store: &Store,
    control: &Control,
) -> Result<()> {
    while let Some((item, position)) = next_missing_chunk(job)? {
        control.check()?;
        control.phase(Phase::Encrypting)?;
        let transfer = job
            .transfer_id
            .as_deref()
            .context("upload was not reserved")?;
        let input = PreparedInput {
            source: job.items[item].source.clone(),
            item,
            item_id: job.items[item].id.clone(),
            position,
            chunk_bytes: job.chunk_bytes,
            prefix: decode(&job.items[item].nonce_prefix)?,
            key: derive_item_key(&job.master_key, transfer, &job.items[item].id)?,
            transfer: transfer.into(),
            control: Control::clone(control),
        };
        let prepared = tokio::task::spawn_blocking(move || prepare_one(input))
            .await
            .context("ciphertext producer ended unexpectedly")??;
        let name = format!("chunk-{}-{}", prepared.item, prepared.position);
        let artifact = store.path().join(&name);
        let checksum = hex::encode(Sha256::digest(&prepared.ciphertext));
        if artifact.exists() {
            let existing = fs::read(&artifact)?;
            if existing != prepared.ciphertext {
                bail!("orphaned ciphertext conflicts with the verified source");
            }
        } else {
            store.persist_immutable(&name, &prepared.ciphertext)?;
        }
        job.chunks.push(Chunk {
            item: prepared.item,
            item_id: job.items[prepared.item].id.clone(),
            position: prepared.position,
            plaintext_bytes: prepared.plaintext_bytes,
            ciphertext_bytes: prepared.ciphertext.len() as u64,
            checksum,
            artifact,
            complete: false,
            stage: None,
        });
        // The immutable file is durable before its parent record, so a restart
        // can safely re-verify and adopt an interrupted preparation step.
        store.save(job)?;
        control.advance(job.done, 0, 0);
    }
    if !preparation_complete(job)? {
        bail!("live ciphertext preparation stopped before all chunks were durable");
    }
    Ok(())
}

async fn serve_live(job: &UploadJob, client: Client, control: &Control) -> Result<String> {
    let transfer = job
        .transfer_id
        .as_deref()
        .context("upload was not reserved")?;
    let token = job
        .upload_token
        .as_deref()
        .context("upload token is unavailable")?;
    let signaling = Signaling::new(client, &job.instance, transfer)?;
    let chunks = job
        .chunks
        .iter()
        .map(|chunk| {
            Ok((
                (chunk.item_id.clone(), chunk.position),
                CiphertextArtifact {
                    path: chunk.artifact.clone(),
                    bytes: chunk.ciphertext_bytes,
                    checksum: chunk.checksum.clone(),
                },
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    let mut peers: JoinSet<(String, Result<()>)> = JoinSet::new();
    let mut serving = HashSet::new();
    let mut stopping = HashMap::new();
    let mut answered = HashSet::new();
    loop {
        control.phase(Phase::Waiting)?;
        let sessions = tokio::select! {
            result = tokio::time::timeout(webrtc::REQUEST_TIMEOUT, signaling.sender_sessions(token)) => {
                result.context("timed out polling live receiver sessions")?
            },
            _ = cancelled(control) => {
                // Keep the published transfer live: its checkpointed artifacts
                // and credentials let the sender resume after an interrupt.
                stop_live_peers(&mut peers, &mut stopping).await;
                bail!("Transfer cancelled");
            }
        }?;
        let (sessions, ice_servers) = sessions;
        answered.retain(|id| {
            sessions.iter().any(|session| {
                session.id == *id
                    && !matches!(
                        session.status.as_str(),
                        "completed" | "cancelled" | "failed"
                    )
            })
        });
        for session in &sessions {
            if matches!(
                session.status.as_str(),
                "completed" | "cancelled" | "failed"
            ) && let Some(stop) = stopping.get(&session.id)
            {
                stop.send_replace(());
            }
        }
        while let Some(result) = peers.try_join_next() {
            let (id, served) = match result {
                Ok(result) => result,
                Err(_) => {
                    control.emit(TransferEvent::PeerFailed(PeerFailed {
                        message: peer_failure_message(),
                    }));
                    continue;
                }
            };
            serving.remove(&id);
            stopping.remove(&id);
            if served.is_err() {
                control.emit(TransferEvent::PeerFailed(PeerFailed {
                    message: peer_failure_message(),
                }));
            }
        }
        for session in sessions {
            if peers.len() >= WEBRTC_MAX_PEERS
                || serving.contains(&session.id)
                || answered.contains(&session.id)
                || session.offer.is_none()
                || matches!(
                    session.status.as_str(),
                    "completed" | "cancelled" | "failed"
                )
            {
                continue;
            }
            let id = session.id.clone();
            let signaling = signaling.clone();
            let token = token.to_owned();
            let chunks = chunks.clone();
            let ice_servers = ice_servers.clone();
            let relay_only = control.webrtc_relay_only();
            let task_control = control.clone();
            let (stop, mut stopped) = watch::channel(());
            serving.insert(id.clone());
            stopping.insert(id.clone(), stop);
            // An SDP answer is immutable at the signaling endpoint. A failed
            // connection must be retried by a fresh receiver session, not by
            // publishing another answer for this offer.
            answered.insert(id.clone());
            peers.spawn(async move {
                let outcome = async {
                    let (peer, channel) = webrtc::connect_sender(
                        &signaling,
                        &token,
                        &session,
                        &ice_servers,
                        relay_only,
                        task_control.cancelled.clone(),
                    )
                    .await?;
                    let served = tokio::select! {
                        served = serve_artifacts(channel, chunks) => served,
                        _ = cancelled(&task_control) => bail!("Transfer cancelled"),
                        _ = stopped.changed() => Ok(()),
                    };
                    peer.close().await;
                    served
                }
                .await;
                (id, outcome)
            });
        }
        tokio::select! {
            _ = tokio::time::sleep(WEBRTC_POLL) => {},
            _ = cancelled(control) => {
                stop_live_peers(&mut peers, &mut stopping).await;
                bail!("Transfer cancelled");
            }
        }
    }
}

async fn stop_live_peers(
    peers: &mut JoinSet<(String, Result<()>)>,
    stopping: &mut HashMap<String, watch::Sender<()>>,
) {
    for stop in stopping.values() {
        stop.send_replace(());
    }
    while peers.join_next().await.is_some() {}
    stopping.clear();
}

fn peer_failure_message() -> String {
    // Peer protocol errors can contain receiver-controlled frames and must not
    // be surfaced through the advisory event stream.
    "A live receiver disconnected or failed.".into()
}

fn next_missing_chunk(job: &UploadJob) -> Result<Option<(usize, u64)>> {
    for (item, entry) in job.items.iter().enumerate() {
        for position in 0..chunk_count(entry.source.bytes, job.chunk_bytes)? {
            if !job
                .chunks
                .iter()
                .any(|chunk| chunk.item == item && chunk.position == position)
            {
                return Ok(Some((item, position)));
            }
        }
    }
    Ok(None)
}

fn prepare_one(input: PreparedInput) -> Result<PreparedCiphertext> {
    let offset = input
        .position
        .checked_mul(input.chunk_bytes)
        .context("chunk offset overflow")?;
    let mut file = source::open(&input.source.spec, input.control.source_resolver())?;
    let chunk_bytes = ((input.source.bytes.saturating_sub(offset)).min(input.chunk_bytes)) as usize;
    let (base, length) = input.source.spec.bounds();
    if offset > length {
        bail!("source chunk starts beyond its declared length");
    }
    file.seek(SeekFrom::Start(
        base.checked_add(offset).context("source offset overflow")?,
    ))?;
    let mut plain = vec![0; chunk_bytes];
    file.read_exact(&mut plain).with_context(|| {
        format!(
            "{} changed while it was being prepared",
            source_label(&input.source.spec)
        )
    })?;
    let expected = input
        .source
        .chunk_digests
        .get(input.position as usize)
        .context("saved upload lacks per-chunk source fingerprints; start a new upload")?;
    if hex::encode(Sha256::digest(&plain)) != *expected
        || source_metadata(&input.source.spec, &input.control)?.modified_ns
            != input.source.modified_ns
    {
        bail!(
            "{} changed after source preflight; start a new upload to avoid nonce reuse",
            source_label(&input.source.spec)
        );
    }
    let ciphertext = encrypt_chunk(
        &input.key,
        &input.prefix,
        input.position as u32,
        &plain,
        aad(&input.transfer, &input.item_id, input.position).as_bytes(),
    )?;
    Ok(PreparedCiphertext {
        item: input.item,
        position: input.position,
        plaintext_bytes: plain.len() as u64,
        ciphertext,
    })
}

async fn reconcile(
    job: &mut UploadJob,
    client: &Client,
    transfer: &str,
    token: &str,
    control: &Control,
) -> Result<()> {
    let mut cursor: Option<String> = None;
    let expected = job
        .chunks
        .iter()
        .map(|chunk| ((chunk.item_id.clone(), chunk.position), chunk))
        .collect::<HashMap<_, _>>();
    let mut published = HashMap::new();
    let mut cursors = HashSet::new();
    loop {
        let mut request_builder = client
            .get(format!(
                "{}/api/v1/transfers/{transfer}/upload-status",
                job.instance
            ))
            .header("X-Filebeam-Upload-Token", token)
            .query(&[("limit", "500")]);
        if let Some(after) = &cursor {
            if !cursors.insert(after.clone()) {
                bail!("server upload status pagination repeated a cursor");
            }
            request_builder = request_builder.query(&[("after", after)]);
        }
        let page: StatusPage = json(control, request(control, request_builder).await?).await?;
        if page.status != "pending"
            || page.protocol_version != 1
            || page.chunk_bytes != job.chunk_bytes
        {
            bail!("saved upload no longer matches the pending server transfer");
        }
        if let Some(t) = page.upload_transport {
            job.transport = Some(t);
        }
        for c in page.chunks {
            let key = (c.item_id, c.position);
            let local = expected
                .get(&key)
                .context("server published an unknown upload chunk")?;
            if c.ciphertext_bytes != local.ciphertext_bytes || c.checksum != local.checksum {
                bail!("server published ciphertext conflicts with local immutable ciphertext");
            }
            if published
                .insert(key, (c.ciphertext_bytes, c.checksum))
                .is_some()
            {
                bail!("server upload status repeated a chunk");
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    for chunk in &mut job.chunks {
        if published.contains_key(&(chunk.item_id.clone(), chunk.position)) {
            chunk.complete = true;
            chunk.stage = None;
        }
    }
    job.done = job
        .chunks
        .iter()
        .filter(|c| c.complete)
        .map(|c| c.plaintext_bytes)
        .sum();
    Ok(())
}

async fn upload_chunk(context: UploadContext, chunk: Chunk) -> Result<Chunk> {
    if let Some(policy) = &context.policy
        && chunk.stage.is_some()
        && policy.should_stage(
            chunk.ciphertext_bytes,
            context.adaptive.lock().unwrap().rate(),
        )
    {
        let body = read_artifact(&chunk)?;
        return stage(StageRequest {
            context: &context,
            chunk,
            body,
            policy,
        })
        .await;
    }
    let (response, abandoned) = direct_request(DirectRequest {
        context: &context,
        chunk: &chunk,
        policy: context.policy.as_ref(),
    })
    .await;
    match response {
        Ok(response) if response.status() == StatusCode::CREATED => {
            return Ok(chunk);
        }
        Ok(response)
            if response.status() == StatusCode::PAYLOAD_TOO_LARGE
                || chunk.stage.is_some()
                || retryable_status(response.status().as_u16(), false) => {}
        Err(_) if abandoned.load(Ordering::Relaxed) => {}
        Err(_) => {}
        Ok(response) => bail!("chunk upload failed: {}", response.status()),
    }
    if let Some(policy) = &context.policy {
        context.control.phase(Phase::Reconnecting)?;
        let display_done = context.sending.lock().unwrap().restart(&chunk);
        context.control.advance(display_done, 0, 0);
        let body = read_artifact(&chunk)?;
        return stage(StageRequest {
            context: &context,
            chunk,
            body,
            policy,
        })
        .await;
    }
    retry_direct(&context, chunk).await
}

async fn retry_direct(context: &UploadContext, chunk: Chunk) -> Result<Chunk> {
    for attempt in 0..5 {
        context.control.phase(Phase::Retrying)?;
        let (response, _) = direct_request(DirectRequest {
            context,
            chunk: &chunk,
            policy: None,
        })
        .await;
        let mut retry_after = None;
        match response {
            Ok(response) if response.status() == StatusCode::CREATED => return Ok(chunk),
            Ok(response) if retryable_status(response.status().as_u16(), false) => {
                retry_after = retry_after_ms(&response);
                context.adaptive.lock().unwrap().congested(now())
            }
            Err(_) => context.adaptive.lock().unwrap().congested(now()),
            Ok(response) => bail!("chunk upload failed: {}", response.status()),
        };
        context.control.phase(Phase::Reconnecting)?;
        cancellable_sleep(
            &context.control,
            retry_delay_ms(attempt, retry_after, (attempt * 37) as u64),
        )
        .await?;
    }
    bail!("chunk upload retry limit exceeded")
}

/// Streams immutable ciphertext from disk.  Reqwest consumes this stream while
/// writing the socket, which makes the samples represent wire progress rather
/// than time spent waiting for the server acknowledgement.
async fn direct_request(
    request: DirectRequest<'_>,
) -> (Result<reqwest::Response>, Arc<AtomicBool>) {
    let context = request.context;
    let chunk = request.chunk;
    let policy = request.policy;
    let abandoned = Arc::new(AtomicBool::new(false));
    let display_done = context.sending.lock().unwrap().restart(chunk);
    context.control.advance(display_done, 0, 0);
    let key = Uuid::new_v4().to_string();
    let file = match tokio::fs::File::open(&chunk.artifact).await {
        Ok(file) => file,
        Err(error) => return (Err(error).context("open immutable ciphertext"), abandoned),
    };
    let total = chunk.ciphertext_bytes;
    let started = Instant::now();
    let stream_abandoned = abandoned.clone();
    let stream_key = key.clone();
    let stream_control = context.control.clone();
    let stream_policy = policy.cloned();
    let stream_adaptive = context.adaptive.clone();
    let stream_sending = context.sending.clone();
    let stream_chunk_key = chunk_key(chunk);
    let stream_plaintext_bytes = chunk.plaintext_bytes;
    let stream_ciphertext_bytes = chunk.ciphertext_bytes;
    let (_body_state, mut body_updates) = watch::channel((false, Instant::now()));
    // Keep one sender outside the stream so a completed stream does not turn
    // the completion notification into a permanently-ready closed channel.
    let stream_body_state = _body_state.clone();
    let body_stream = stream::unfold((file, 0u64), move |(mut file, loaded)| {
        let abandoned = stream_abandoned.clone();
        let adaptive = stream_adaptive.clone();
        let key = stream_key.clone();
        let control = stream_control.clone();
        let policy = stream_policy.clone();
        let body_state = stream_body_state.clone();
        let sending = stream_sending.clone();
        async move {
            if control.cancelled.load(Ordering::Relaxed) {
                return Some((
                    Err(std::io::Error::other("Transfer cancelled")),
                    (file, loaded),
                ));
            }
            if policy.as_ref().is_some_and(|p| {
                p.should_abandon_direct(total, loaded, started.elapsed().as_millis() as u64)
            }) {
                abandoned.store(true, Ordering::Relaxed);
                return Some((
                    Err(std::io::Error::other(
                        "direct upload exceeds transport budget",
                    )),
                    (file, loaded),
                ));
            }
            let remaining = total.saturating_sub(loaded) as usize;
            if remaining == 0 {
                body_state.send_replace((true, Instant::now()));
                return None;
            }
            let mut buffer = vec![0; remaining.min(BODY_BUFFER_BYTES)];
            match file.read(&mut buffer).await {
                Ok(0) => Some((
                    Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "immutable ciphertext was truncated",
                    )),
                    (file, loaded),
                )),
                Ok(read) => {
                    buffer.truncate(read);
                    let loaded = loaded + read as u64;
                    body_state.send_replace((loaded == total, Instant::now()));
                    adaptive.lock().unwrap().sample(&key, loaded, now());
                    let display_done = sending.lock().unwrap().report(
                        stream_chunk_key,
                        stream_plaintext_bytes,
                        stream_ciphertext_bytes,
                        loaded,
                    );
                    control.advance(display_done, 0, read as u64);
                    Some((
                        Ok::<Bytes, std::io::Error>(Bytes::from(buffer)),
                        (file, loaded),
                    ))
                }
                Err(error) => Some((Err(error), (file, loaded))),
            }
        }
    });
    let send = context
        .client
        .put(format!(
            "{}/api/v1/transfers/{}/items/{}/chunks/{}",
            context.instance, context.transfer, chunk.item_id, chunk.position
        ))
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(CONTENT_LENGTH, total)
        .header("X-Filebeam-Upload-Token", &context.token)
        .body(Body::wrap_stream(body_stream))
        .send();
    tokio::pin!(send);
    let mut body_finished = false;
    let mut idle_deadline = tokio::time::Instant::now() + UPLOAD_BODY_IDLE;
    let transmission_deadline = tokio::time::Instant::now()
        + Duration::from_millis(policy.map_or(u64::MAX, |policy| policy.request_budget_ms));
    let mut acknowledgement_deadline = None;
    let result = loop {
        let idle = tokio::time::sleep_until(idle_deadline);
        let transmission = tokio::time::sleep_until(transmission_deadline);
        let acknowledgement = tokio::time::sleep_until(
            acknowledgement_deadline
                .unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(86400)),
        );
        tokio::pin!(idle, transmission, acknowledgement);
        tokio::select! {
            response = &mut send => break response.map_err(anyhow::Error::from),
            _ = cancelled(&context.control) => break Err(anyhow::anyhow!("Transfer cancelled")),
            changed = body_updates.changed() => {
                body_finished = body_updates.borrow().0;
                if changed.is_err() && !body_finished {
                    break Err(anyhow::anyhow!("upload body stream stopped unexpectedly"));
                }
                if body_finished {
                    acknowledgement_deadline = Some(tokio::time::Instant::now() + UPLOAD_ACK_HEADERS);
                } else {
                    idle_deadline = tokio::time::Instant::now() + UPLOAD_BODY_IDLE;
                }
            }
            _ = &mut idle, if !body_finished => break Err(anyhow::anyhow!("upload body became idle")),
            _ = &mut transmission, if policy.is_some() && !body_finished => break Err(anyhow::anyhow!("upload transmission exceeded transport budget")),
            _ = &mut acknowledgement, if acknowledgement_deadline.is_some() => break Err(anyhow::anyhow!("upload acknowledgement headers timed out")),
        }
    };
    context.adaptive.lock().unwrap().forget(&key);
    if let Ok(ref response) = result
        && response.status() == StatusCode::CREATED
    {
        context.adaptive.lock().unwrap().observe(
            total,
            started.elapsed().as_millis() as u64,
            now(),
        );
    }
    (result, abandoned)
}

fn read_artifact(chunk: &Chunk) -> Result<Vec<u8>> {
    let body = fs::read(&chunk.artifact)
        .with_context(|| format!("read immutable ciphertext {}", chunk.artifact.display()))?;
    if body.len() as u64 != chunk.ciphertext_bytes
        || hex::encode(Sha256::digest(&body)) != chunk.checksum
    {
        bail!("immutable ciphertext was changed or is corrupt");
    }
    Ok(body)
}

fn retry_after_ms(response: &reqwest::Response) -> Option<u64> {
    let value = response.headers().get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds.saturating_mul(1_000));
    }
    let date = OffsetDateTime::parse(value, &Rfc2822).ok()?;
    let now = OffsetDateTime::now_utc();
    Some((date - now).whole_milliseconds().max(0) as u64)
}

async fn cancellable_sleep(control: &Control, delay_ms: u64) -> Result<()> {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => Ok(()),
        _ = cancelled(control) => bail!("Transfer cancelled"),
    }
}

/// A stage has one writer, while direct chunks continue in the shared scheduler.
/// Each part response is reconciled through the transfer crate so a duplicate
/// acknowledgement or a lost staging file cannot silently advance local state.
async fn stage(request: StageRequest<'_>) -> Result<Chunk> {
    let context = request.context;
    let mut chunk = request.chunk;
    let body = request.body;
    let policy = request.policy;
    policy.validate().map_err(anyhow::Error::msg)?;
    let name = stage_name(&chunk);
    let saved = context
        .store
        .load_named::<StageSession>(&name)?
        .or(chunk.stage.take())
        .unwrap_or(
            StageSession::new(
                Uuid::new_v4().to_string(),
                chunk.ciphertext_bytes,
                chunk.checksum.clone(),
            )
            .map_err(anyhow::Error::msg)?,
        );
    validate_stage(&saved, &chunk)?;
    // The server's status is the offset authority after a crash. Keep only the
    // durable identity before probing it, rather than trusting a stale ACK.
    let mut session = StageSession::new(saved.id, chunk.ciphertext_bytes, chunk.checksum.clone())
        .map_err(anyhow::Error::msg)?;
    // This is before `begin`, including direct-to-stage fallback.
    context.store.save_named(&name, &session)?;
    let base = format!(
        "{}/api/v1/transfers/{}/items/{}/chunks/{}/uploads/{}",
        context.instance, context.transfer, chunk.item_id, chunk.position, session.id
    );
    let started: StageStatus = stage_json(
        &context.control,
        context
            .client
            .put(&base)
            .header("X-Filebeam-Upload-Token", &context.token)
            .json(&serde_json::json!({"ciphertext_bytes":chunk.ciphertext_bytes,"checksum":chunk.checksum})),
    )
    .await?;
    let mut action = session.reconcile(&started).map_err(anyhow::Error::msg)?;
    let mut part_bytes = policy
        .initial_part_bytes(context.adaptive.lock().unwrap().rate())
        .max(policy.part_min_bytes);
    loop {
        match action {
            StageAction::Complete => {
                chunk.stage = Some(session);
                return Ok(chunk);
            }
            StageAction::Finalizing => {
                let status = complete_stage(
                    &context.client,
                    &base,
                    &context.token,
                    &mut session,
                    &context.control,
                )
                .await?;
                action = session.reconcile(&status).map_err(anyhow::Error::msg)?;
                context.store.save_named(&name, &session)?;
            }
            StageAction::Continue { offset } => {
                if offset == chunk.ciphertext_bytes {
                    action = StageAction::Finalizing;
                    continue;
                }
                let size = (chunk.ciphertext_bytes - offset).min(part_bytes) as usize;
                policy
                    .validate_part_count(chunk.ciphertext_bytes, part_bytes)
                    .map_err(anyhow::Error::msg)?;
                let part = &body[offset as usize..offset as usize + size];
                let checksum = hex::encode(Sha256::digest(part));
                let start = Instant::now();
                let response = tokio::select! {
                    response = tokio::time::timeout(Duration::from_millis(policy.request_budget_ms), context.client
                        .put(format!("{base}/parts/{offset}"))
                        .header("X-Filebeam-Upload-Token", &context.token)
                        .header("X-Filebeam-Part-Checksum", checksum)
                        .body(part.to_vec())
                        .send()) => response,
                    _ = cancelled(&context.control) => bail!("Transfer cancelled"),
                };
                match response {
                    Ok(Ok(response)) if response.status().is_success() => {
                        let status: StageStatus =
                            stage_json_response(&context.control, response).await?;
                        action = session
                            .acknowledge_part(offset, size as u64, &status)
                            .map_err(anyhow::Error::msg)?;
                        context.store.save_named(&name, &session)?;
                        let elapsed = start.elapsed().as_millis() as u64;
                        context
                            .adaptive
                            .lock()
                            .unwrap()
                            .observe(size as u64, elapsed, now());
                        part_bytes = policy.grow_part(part_bytes, size as u64, elapsed);
                    }
                    Ok(Ok(response)) if retryable_status(response.status().as_u16(), true) => {
                        session.record_retry().map_err(anyhow::Error::msg)?;
                        context.adaptive.lock().unwrap().congested(now());
                        part_bytes = policy.shrink_part(part_bytes);
                        context.control.phase(Phase::Retrying)?;
                        cancellable_sleep(
                            &context.control,
                            retry_delay_ms(session.retries, retry_after_ms(&response), 0),
                        )
                        .await?;
                    }
                    Ok(Ok(response)) if response.status() == StatusCode::GONE => {
                        session
                            .reset(Uuid::new_v4().to_string())
                            .map_err(anyhow::Error::msg)?;
                        // The replacement UUID must survive before its begin request.
                        context.store.save_named(&name, &session)?;
                        return Box::pin(stage(StageRequest {
                            context,
                            chunk: Chunk {
                                stage: Some(session),
                                ..chunk
                            },
                            body,
                            policy,
                        }))
                        .await;
                    }
                    Ok(Ok(response)) => bail!("staged part upload failed: {}", response.status()),
                    Ok(Err(_)) | Err(_) => {
                        session.record_retry().map_err(anyhow::Error::msg)?;
                        context.adaptive.lock().unwrap().congested(now());
                        context.control.phase(Phase::Retrying)?;
                        cancellable_sleep(
                            &context.control,
                            retry_delay_ms(session.retries, None, 0),
                        )
                        .await?;
                    }
                }
            }
        }
    }
}

async fn complete_stage(
    client: &Client,
    base: &str,
    token: &str,
    session: &mut StageSession,
    control: &Control,
) -> Result<StageStatus> {
    for attempt in 0..=8 {
        let response = tokio::select! {
            response = tokio::time::timeout(RESPONSE_DEADLINE, client
                .post(format!("{base}/complete"))
                .header("X-Filebeam-Upload-Token", token)
                .send()) => response,
            _ = cancelled(control) => bail!("Transfer cancelled"),
        };
        match response {
            Ok(Ok(response)) if response.status().is_success() => {
                return stage_json_response(control, response).await;
            }
            Ok(Ok(response)) if retryable_status(response.status().as_u16(), true) => {
                let delay = retry_after_ms(&response);
                session.record_retry().map_err(anyhow::Error::msg)?;
                control.phase(Phase::Retrying)?;
                cancellable_sleep(
                    control,
                    retry_delay_ms(session.retries, delay, attempt as u64 * 29),
                )
                .await?;
                continue;
            }
            Ok(Ok(response)) => bail!("staged completion failed: {}", response.status()),
            Ok(Err(_)) | Err(_) => {}
        }
        if attempt == 8 {
            break;
        }
        session.record_retry().map_err(anyhow::Error::msg)?;
        control.phase(Phase::Retrying)?;
        cancellable_sleep(
            control,
            retry_delay_ms(session.retries, None, attempt as u64 * 29),
        )
        .await?;
    }
    bail!("staged completion retry limit exceeded")
}

/// Retry the idempotent completion request with the exact checkpointed envelope.
/// A lost success response is indistinguishable from a failed request to the client.
async fn complete_transfer(
    client: &Client,
    instance: &str,
    transfer: &str,
    token: &str,
    manifest: &str,
    encrypted_key: Option<&str>,
    control: &Control,
) -> Result<()> {
    for attempt in 0..5 {
        let response = request(
            control,
            client
                .post(format!("{instance}/api/v1/transfers/{transfer}/complete"))
                .header("X-Filebeam-Upload-Token", token)
                .json(&serde_json::json!({"encrypted_manifest": manifest, "encrypted_key": encrypted_key})),
        )
        .await;
        match response {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) if retryable_status(response.status().as_u16(), false) => {
                control.phase(Phase::Retrying)?;
                cancellable_sleep(
                    control,
                    retry_delay_ms(attempt, retry_after_ms(&response), attempt as u64 * 41),
                )
                .await?;
            }
            Ok(response) => bail!("could not complete transfer: {}", response.status()),
            Err(error) if attempt < 4 => {
                control.phase(Phase::Reconnecting)?;
                cancellable_sleep(control, retry_delay_ms(attempt, None, attempt as u64 * 41))
                    .await?;
                let _ = error;
            }
            Err(error) => return Err(error).context("could not complete transfer"),
        }
    }
    bail!("could not complete transfer")
}

async fn stage_json(control: &Control, builder: reqwest::RequestBuilder) -> Result<StageStatus> {
    stage_json_response(control, request(control, builder).await?).await
}
async fn stage_json_response(
    control: &Control,
    response: reqwest::Response,
) -> Result<StageStatus> {
    if !response.status().is_success() {
        bail!("staging request failed: {}", response.status());
    }
    let bytes = read_control_body(control, response).await?;
    Ok(serde_json::from_slice::<Api<StageStatus>>(&bytes)?.data)
}

async fn request(control: &Control, request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    request_headers(request, RESPONSE_DEADLINE, cancelled(control)).await
}
async fn cancelled(control: &Control) {
    while !control.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn read_control_body(control: &Control, response: reqwest::Response) -> Result<Vec<u8>> {
    read_limited(
        response,
        ResponseLimits {
            headers_timeout: RESPONSE_DEADLINE,
            body_idle_timeout: CONTROL_BODY_IDLE,
            maximum_bytes: JSON_LIMIT,
        },
        cancelled(control),
    )
    .await
}
async fn json<T: for<'de> Deserialize<'de>>(
    control: &Control,
    response: reqwest::Response,
) -> Result<T> {
    if !response.status().is_success() {
        bail!("server returned {}", response.status());
    }
    let bytes = read_control_body(control, response).await?;
    Ok(serde_json::from_slice::<Api<T>>(&bytes)?.data)
}
fn client(control: &Control, authentication: &super::UploadAuthentication) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    match authentication {
        super::UploadAuthentication::Anonymous => {}
        super::UploadAuthentication::Bearer(token) => {
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("invalid bearer authentication")?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        super::UploadAuthentication::SessionCookie(cookie) => {
            let mut value = HeaderValue::from_str(cookie).context("invalid session cookie")?;
            value.set_sensitive(true);
            headers.insert(COOKIE, value);
            headers.insert(
                HeaderName::from_static("sec-fetch-site"),
                HeaderValue::from_static("same-origin"),
            );
        }
    }
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent(control.client_user_agent())
        .default_headers(headers)
        .build()
        .context("create HTTP client")
}
fn source_metadata(spec: &SourceSpec, control: &Control) -> Result<Source> {
    spec.checked_end()?;
    let metadata = match spec {
        SourceSpec::Path {
            path,
            offset,
            length,
        } => {
            let metadata =
                fs::metadata(path).with_context(|| format!("read {}", path.display()))?;
            if !metadata.is_file()
                || metadata.len()
                    < offset
                        .checked_add(*length)
                        .context("source offset overflow")?
            {
                bail!("{} is not a bounded regular file", path.display());
            }
            metadata
        }
        SourceSpec::Provider { .. } => source::open(spec, control.source_resolver())?
            .metadata()
            .context("read provider descriptor metadata")?,
    };
    Ok(Source {
        spec: spec.clone(),
        bytes: spec.bounds().1,
        modified_ns: metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        digest: String::new(),
        chunk_digests: Vec::new(),
    })
}
fn fingerprint(
    spec: &SourceSpec,
    chunk_bytes: u64,
    cancelled: &AtomicBool,
    control: &Control,
) -> Result<Source> {
    let mut source = source_metadata(spec, control)?;
    let mut file = source::open(spec, control.source_resolver())?;
    file.seek(SeekFrom::Start(spec.bounds().0))?;
    let mut hash = Sha256::new();
    let mut chunk_hash = Sha256::new();
    let mut remaining = chunk_bytes;
    let mut chunks = Vec::with_capacity(usize::try_from(chunk_count(source.bytes, chunk_bytes)?)?);
    let mut buffer = [0; 64 * 1024];
    let mut source_remaining = source.bytes;
    while source_remaining > 0 {
        if cancelled.load(Ordering::Relaxed) {
            bail!("Transfer cancelled");
        }
        let buffer_length = buffer.len() as u64;
        let n = file.read(&mut buffer[..usize::try_from(source_remaining.min(buffer_length))?])?;
        if n == 0 {
            bail!(
                "{} was truncated while it was fingerprinted",
                source_label(spec)
            );
        }
        source_remaining -= n as u64;
        hash.update(&buffer[..n]);
        let mut offset = 0;
        while offset < n {
            let take = usize::try_from(remaining.min((n - offset) as u64))?;
            chunk_hash.update(&buffer[offset..offset + take]);
            offset += take;
            remaining -= take as u64;
            if remaining == 0 {
                chunks.push(hex::encode(chunk_hash.finalize_reset()));
                remaining = chunk_bytes;
            }
        }
    }
    if source.bytes == 0 || remaining != chunk_bytes {
        chunks.push(hex::encode(chunk_hash.finalize()));
    }
    if chunks.len() as u64 != chunk_count(source.bytes, chunk_bytes)? {
        bail!("{} changed while it was fingerprinted", source_label(spec));
    }
    source.digest = hex::encode(hash.finalize());
    source.chunk_digests = chunks;
    Ok(source)
}
fn source_label(spec: &SourceSpec) -> String {
    match spec {
        SourceSpec::Path { path, .. } => path.display().to_string(),
        SourceSpec::Provider { identity, .. } => identity.clone(),
    }
}
fn validate_sources(job: &UploadJob, control: &Control) -> Result<()> {
    for (item_index, item) in job.items.iter().enumerate() {
        if item.source.chunk_digests.is_empty() {
            bail!("saved partial upload lacks per-chunk source fingerprints; start a new upload");
        }
        let actual = match fingerprint(
            &item.source.spec,
            job.chunk_bytes,
            &control.cancelled,
            control,
        ) {
            // `uploads::prepare` owns ZIP archives in a temporary directory.
            // Once every authenticated ciphertext record is durable, that
            // archive may disappear between a crash and resume.
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
                    && item_prepared(job, item_index)? =>
            {
                continue;
            }
            result => result?,
        };
        if actual.bytes != item.source.bytes
            || actual.modified_ns != item.source.modified_ns
            || actual.digest != item.source.digest
            || actual.chunk_digests != item.source.chunk_digests
        {
            bail!(
                "{} changed after ciphertext preparation; start a new upload to avoid nonce reuse",
                source_label(&item.source.spec)
            );
        }
    }
    Ok(())
}
fn item_prepared(job: &UploadJob, item: usize) -> Result<bool> {
    Ok(
        (0..chunk_count(job.items[item].source.bytes, job.chunk_bytes)?).all(|position| {
            job.chunks
                .iter()
                .any(|chunk| chunk.item == item && chunk.position == position)
        }),
    )
}
fn stage_name(chunk: &Chunk) -> String {
    format!("stage-{}-{}.json", chunk.item, chunk.position)
}
fn validate_stage(session: &StageSession, chunk: &Chunk) -> Result<()> {
    if session.id.is_empty()
        || session.ciphertext_bytes != chunk.ciphertext_bytes
        || session.checksum != chunk.checksum
        || session.offset > chunk.ciphertext_bytes
    {
        bail!("saved stage sidecar does not match immutable ciphertext");
    }
    Ok(())
}
fn preparation_complete(job: &UploadJob) -> Result<bool> {
    if job.items.iter().any(|item| {
        item.bytes != item.source.bytes
            || item.chunk_count
                != chunk_count(item.source.bytes, job.chunk_bytes).unwrap_or(u64::MAX)
            || item.digest.is_empty()
    }) {
        return Ok(false);
    }
    let expected = job
        .items
        .iter()
        .enumerate()
        .try_fold(0usize, |count, (_, entry)| {
            Ok::<_, anyhow::Error>(
                count + usize::try_from(chunk_count(entry.source.bytes, job.chunk_bytes)?)?,
            )
        })?;
    if job.chunks.len() != expected {
        return Ok(false);
    }
    for chunk in &job.chunks {
        // A completed chunk is checkpointed before its immutable retry input is
        // removed. It is still a durable prepared record.
        if chunk.complete {
            continue;
        }
        let bytes = match fs::read(&chunk.artifact) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false),
        };
        if bytes.len() as u64 != chunk.ciphertext_bytes
            || hex::encode(Sha256::digest(&bytes)) != chunk.checksum
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn validate_job(job: &UploadJob) -> Result<()> {
    if job.version != 1
        || job.direction != "upload"
        || !matches!(
            job.state.as_str(),
            "preparing" | "sending" | "finalizing" | "serving" | "ended" | "complete"
        )
        || job.chunk_bytes == 0
        || job.chunk_bytes > 24_999_984
        || job.total != job.items.iter().map(|item| item.source.bytes).sum::<u64>()
        || (job.state == "complete" && job.receipt.is_none())
        || (matches!(job.state.as_str(), "finalizing" | "serving") && job.exact_manifest.is_none())
        || (job.state == "finalizing" && job.chunks.iter().any(|chunk| !chunk.complete))
        || (!matches!(job.driver.as_str(), "http" | "webrtc"))
        || (job.driver == "webrtc" && job.join_token.as_deref().unwrap_or_default().is_empty())
        || (job.turbo && (job.driver != "http" || job.recipient.is_some()))
        || (job.descriptor_published && job.encrypted_descriptor.is_none())
    {
        bail!("invalid saved upload checkpoint");
    }
    if job.share_key.len() != 32
        || job
            .password_salt
            .as_ref()
            .is_some_and(|salt| decode(salt).map_or(true, |value| value.len() != 16))
        || job.state != "complete"
            && (job.transfer_id.as_deref().unwrap_or_default().is_empty()
                || job.upload_token.as_deref().unwrap_or_default().is_empty())
    {
        bail!("saved upload checkpoint is missing credentials or encryption material");
    }
    let mut positions = HashSet::new();
    let mut item_ids = HashSet::new();
    for (position, item) in job.items.iter().enumerate() {
        if item.id.is_empty()
            || item.position != position as u64
            || !item_ids.insert(&item.id)
            || decode(&item.nonce_prefix).map_or(true, |prefix| prefix.len() != 16)
            || item.digest.len() != 64
            || !item.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            || item.source.digest.len() != 64
            || !item
                .source
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || item.source.chunk_digests.len() as u64
                != chunk_count(item.source.bytes, job.chunk_bytes)?
            || item.source.chunk_digests.iter().any(|digest| {
                digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            bail!("invalid saved upload item checkpoint");
        }
    }
    for chunk in &job.chunks {
        let expected_plaintext = job
            .items
            .get(chunk.item)
            .map(|item| {
                item.source
                    .bytes
                    .saturating_sub(chunk.position.saturating_mul(job.chunk_bytes))
                    .min(job.chunk_bytes)
            })
            .unwrap_or_default();
        if chunk.item >= job.items.len()
            || chunk.item_id != job.items[chunk.item].id
            || chunk.position >= chunk_count(job.items[chunk.item].source.bytes, job.chunk_bytes)?
            || chunk.plaintext_bytes != expected_plaintext
            || chunk.ciphertext_bytes
                != chunk
                    .plaintext_bytes
                    .checked_add(TAG_BYTES)
                    .context("ciphertext size overflow")?
            || chunk.checksum.len() != 64
            || !positions.insert((chunk.item, chunk.position))
        {
            bail!("invalid saved upload chunk checkpoint");
        }
    }
    Ok(())
}
fn receipt(job: &UploadJob) -> Result<String> {
    let share = job
        .share_url
        .as_deref()
        .context("transfer share URL is unavailable")?
        .split('#')
        .next()
        .unwrap_or_default();
    if !share.starts_with('/') || share.starts_with("//") || job.share_key.len() != 32 {
        bail!("server returned an invalid transfer share URL");
    }
    Ok(format!(
        "{}{}#k=v1.{}",
        job.instance.trim_end_matches('/'),
        share,
        encode(&job.share_key)
    ))
}
fn select_retention(default: u64, options: &[u64], requested: Option<u64>) -> Result<u64> {
    let retention = requested.unwrap_or(default);
    if retention == 0
        || (!options.is_empty() && !options.contains(&retention))
        || (options.is_empty() && retention != default)
    {
        bail!("the selected retention is not available on this instance");
    }
    Ok(retention)
}
fn encrypted_manifest(job: &UploadJob, control: &Control) -> Result<String> {
    let transfer = job
        .transfer_id
        .as_deref()
        .context("upload was not reserved")?;
    let mut manifest = serde_json::json!({"version":1,"items":job.items.iter().map(|i| serde_json::json!({"id":i.id,"name":i.name,"type":"application/octet-stream","size":i.bytes,"nonce_prefix":i.nonce_prefix,"chunk_count":i.chunk_count,"digest":{"algorithm":"sha256","value":i.digest}})).collect::<Vec<_>>()});
    if job.driver == "webrtc" {
        manifest["join_token"] = serde_json::Value::String(
            job.join_token
                .clone()
                .context("live transfer join token is unavailable")?,
        );
    }
    let plain = serde_json::to_vec(&manifest)?;
    let _crypto_memory = control.reserve_memory(super::manifest_crypto_bytes(plain.len())?)?;
    let prefix = generate_nonce_prefix()?;
    let ciphertext = encrypt_manifest(
        &job.master_key,
        &prefix,
        &plain,
        format!("filebeam:v1:{transfer}:manifest:manifest").as_bytes(),
    )?;
    let mut envelope = serde_json::json!({
        "v": 1,
        "nonce_prefix": encode(&prefix),
        "ciphertext": encode(&ciphertext),
    });
    if let Some(salt) = &job.password_salt {
        envelope["salt"] = serde_json::Value::String(salt.clone());
        envelope["kdf"] = serde_json::json!({
            "name": "argon2id",
            "memory_kib": 65_536,
            "iterations": 3,
            "parallelism": 1,
        });
    }
    Ok(serde_json::to_string(&envelope)?)
}

fn turbo_descriptor(job: &UploadJob, transfer: &str, control: &Control) -> Result<String> {
    if job.driver != "http" || job.recipient.is_some() {
        bail!("Turbo requires an anonymous HTTP file transfer");
    }
    let descriptor = TurboDescriptor {
        version: 1,
        purpose: "turbo-descriptor",
        chunk_bytes: job.chunk_bytes,
        items: job
            .items
            .iter()
            .map(|item| TurboDescriptorItem {
                id: &item.id,
                name: &item.name,
                mime: "application/octet-stream",
                size: item.bytes,
                nonce_prefix: &item.nonce_prefix,
                chunk_count: item.chunk_count,
            })
            .collect(),
    };
    let plain = serde_json::to_vec(&descriptor)?;
    let _crypto_memory = control.reserve_memory(super::manifest_crypto_bytes(plain.len())?)?;
    let prefix = generate_nonce_prefix()?;
    let ciphertext = encrypt_manifest(
        &job.master_key,
        &prefix,
        &plain,
        format!("filebeam:v1:{transfer}:descriptor:descriptor").as_bytes(),
    )?;
    let mut envelope = serde_json::json!({
        "v": 1,
        "nonce_prefix": encode(&prefix),
        "ciphertext": encode(&ciphertext),
    });
    if let Some(salt) = &job.password_salt {
        envelope["salt"] = serde_json::Value::String(salt.clone());
        envelope["kdf"] = serde_json::json!({
            "name": "argon2id",
            "memory_kib": 65_536,
            "iterations": 3,
            "parallelism": 1,
        });
    }
    Ok(serde_json::to_string(&envelope)?)
}

async fn publish_turbo_descriptor(
    job: &mut UploadJob,
    client: &Client,
    transfer: &str,
    token: &str,
    control: &Control,
) -> Result<()> {
    let envelope = job
        .encrypted_descriptor
        .as_deref()
        .context("Turbo descriptor was not checkpointed")?;
    for attempt in 0..5 {
        let response = request(
            control,
            client
                .put(format!(
                    "{}/api/v1/transfers/{transfer}/descriptor",
                    job.instance
                ))
                .header("X-Filebeam-Upload-Token", token)
                .json(&serde_json::json!({"encrypted_descriptor": envelope})),
        )
        .await;
        match response {
            Ok(response) if response.status().is_success() => {
                job.descriptor_published = true;
                return Ok(());
            }
            Ok(response) if retryable_status(response.status().as_u16(), false) => {
                control.phase(Phase::Retrying)?;
                cancellable_sleep(
                    control,
                    retry_delay_ms(attempt, retry_after_ms(&response), attempt as u64 * 17),
                )
                .await?;
            }
            Ok(response) => bail!("could not publish Turbo descriptor: {}", response.status()),
            Err(error) if attempt == 4 => {
                return Err(error).context("could not publish Turbo descriptor");
            }
            Err(_) => {
                cancellable_sleep(control, retry_delay_ms(attempt, None, attempt as u64 * 17))
                    .await?
            }
        }
    }
    bail!("could not publish Turbo descriptor")
}

async fn turbo_heartbeat(
    client: &Client,
    instance: &str,
    transfer: &str,
    token: &str,
) -> Result<()> {
    tokio::time::sleep(Duration::from_secs(10)).await;
    let response = client
        .patch(format!("{instance}/api/v1/transfers/{transfer}/progress"))
        .header("X-Filebeam-Upload-Token", token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .context("send Turbo heartbeat")?;
    if response.status().as_u16() != 204 {
        bail!("Turbo heartbeat returned {}", response.status());
    }
    Ok(())
}
fn aad(transfer: &str, item: &str, position: u64) -> String {
    format!("filebeam:v1:{transfer}:{item}:{position}")
}
fn http_driver() -> String {
    "http".into()
}
fn encode(value: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(value)
}
fn decode(value: &str) -> Result<Vec<u8>> {
    Ok(URL_SAFE_NO_PAD.decode(value)?)
}
fn chunk_count(bytes: u64, chunk: u64) -> Result<u64> {
    if chunk == 0 || chunk > 24_999_984 {
        bail!("invalid chunk size");
    }
    Ok(bytes.div_ceil(chunk).max(1))
}
fn ciphertext_bytes(bytes: u64, chunk: u64) -> Result<u64> {
    bytes
        .checked_add(
            chunk_count(bytes, chunk)?
                .checked_mul(TAG_BYTES)
                .context("ciphertext size overflow")?,
        )
        .context("ciphertext size overflow")
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_progress_does_not_count_a_tail_tag_or_retry_twice() {
        let chunk = Chunk {
            item: 0,
            item_id: "item".into(),
            position: 3,
            plaintext_bytes: 10,
            ciphertext_bytes: 26,
            checksum: String::new(),
            artifact: PathBuf::new(),
            complete: false,
            stage: None,
        };
        let mut progress = SendProgress::new(11);

        assert_eq!(progress.restart(&chunk), 11);
        assert_eq!(progress.report(chunk_key(&chunk), 10, 26, 9), 20);
        assert_eq!(progress.report(chunk_key(&chunk), 10, 26, 26), 21);

        assert_eq!(progress.restart(&chunk), 11);
        assert_eq!(progress.report(chunk_key(&chunk), 10, 26, 10), 21);
        assert_eq!(progress.report(chunk_key(&chunk), 10, 26, 26), 21);
        assert_eq!(progress.acknowledged(&chunk), 21);
        assert!(progress.active.is_empty());
    }

    #[test]
    fn ciphertext_accounting_handles_empty_and_full_chunks() {
        assert_eq!(chunk_count(0, 10).unwrap(), 1);
        assert_eq!(ciphertext_bytes(0, 10).unwrap(), TAG_BYTES);
        assert_eq!(chunk_count(20, 10).unwrap(), 2);
        assert_eq!(ciphertext_bytes(20, 10).unwrap(), 20 + TAG_BYTES * 2);
    }

    #[test]
    fn fingerprint_rejects_same_length_content_change() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source");
        fs::write(&path, b"first").unwrap();
        let cancelled = AtomicBool::new(false);
        let control = Control::test_factory();
        let spec = SourceSpec::Path {
            path: path.clone(),
            offset: 0,
            length: 5,
        };
        let original = fingerprint(&spec, 10, &cancelled, &control).unwrap();
        fs::write(&path, b"other").unwrap();
        let changed = fingerprint(&spec, 10, &cancelled, &control).unwrap();
        assert_eq!(original.bytes, changed.bytes);
        assert_ne!(original.digest, changed.digest);
    }

    #[test]
    fn fingerprint_tracks_non_power_of_two_chunk_boundaries_and_prepare_rejects_revert() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source");
        let original = vec![9; 25 * 1024 * 1024 + 3];
        fs::write(&path, &original).unwrap();
        let cancelled = AtomicBool::new(false);
        let control = Control::test_factory();
        let spec = SourceSpec::Path {
            path: path.clone(),
            offset: 0,
            length: original.len() as u64,
        };
        let source = fingerprint(&spec, 24_999_984, &cancelled, &control).unwrap();
        assert_eq!(source.chunk_digests.len(), 2);
        fs::write(&path, vec![7; original.len()]).unwrap();
        fs::write(&path, &original).unwrap();
        assert!(
            prepare_one(PreparedInput {
                source,
                item: 0,
                item_id: "item".into(),
                position: 0,
                chunk_bytes: 24_999_984,
                prefix: vec![0; 16],
                key: vec![0; 32],
                transfer: "transfer".into(),
                control,
            })
            .is_err()
        );
    }

    #[test]
    fn checkpoint_header_has_the_generic_upload_summary_fields() {
        let job = UploadJob {
            version: 1,
            id: "job".into(),
            direction: "upload".into(),
            state: "preparing".into(),
            instance: "http://example.test".into(),
            done: 3,
            total: 5,
            transfer_id: None,
            upload_token: None,
            share_url: None,
            delete_token: None,
            share_key: vec![0; 32],
            password_salt: None,
            master_key: vec![0; 32],
            chunk_bytes: 10,
            server_concurrency: 1,
            upload_status: false,
            turbo: false,
            descriptor_published: false,
            encrypted_descriptor: None,
            transport: None,
            driver: "http".into(),
            join_token: None,
            exact_manifest: None,
            receipt: None,
            recipient: None,
            items: Vec::new(),
            chunks: Vec::new(),
        };
        let json = serde_json::to_value(job).unwrap();
        for field in [
            "version",
            "id",
            "direction",
            "state",
            "instance",
            "done",
            "total",
        ] {
            assert!(json.get(field).is_some(), "missing {field}");
        }
    }

    fn job(state: &str) -> UploadJob {
        UploadJob {
            version: 1,
            id: "job".into(),
            direction: "upload".into(),
            state: state.into(),
            instance: "https://example.test".into(),
            done: 0,
            total: 0,
            transfer_id: Some("01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
            upload_token: Some("token".into()),
            share_url: Some("/transfers/01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
            delete_token: Some("delete".into()),
            share_key: vec![7; 32],
            password_salt: None,
            master_key: vec![7; 32],
            chunk_bytes: 10,
            server_concurrency: 1,
            upload_status: false,
            turbo: false,
            descriptor_published: false,
            encrypted_descriptor: None,
            transport: None,
            driver: "http".into(),
            join_token: None,
            exact_manifest: None,
            receipt: None,
            recipient: None,
            items: Vec::new(),
            chunks: Vec::new(),
        }
    }

    #[test]
    fn finalizing_checkpoint_requires_the_exact_manifest_and_receipt_is_relative() {
        let mut pending = job("finalizing");
        assert!(validate_job(&pending).is_err());
        pending.exact_manifest = Some("immutable-envelope".into());
        validate_job(&pending).unwrap();
        let url = receipt(&pending).unwrap();
        assert_eq!(
            url,
            "https://example.test/transfers/01ARZ3NDEKTSV4RRFFQ69G5FAV#k=v1.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"
        );
        pending.share_url = Some("https://attacker.test/link".into());
        assert!(receipt(&pending).is_err());
    }

    #[test]
    fn fully_spooled_checkpoint_does_not_need_the_original_source_to_resume() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("chunk-0-0");
        let ciphertext = vec![1; 16];
        fs::write(&artifact, &ciphertext).unwrap();
        let mut saved = job("preparing");
        saved.items.push(Item {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAW".into(),
            position: 0,
            name: "archive.zip".into(),
            source: Source {
                spec: SourceSpec::Path {
                    path: directory.path().join("removed-archive.zip"),
                    offset: 0,
                    length: 0,
                },
                bytes: 0,
                modified_ns: 0,
                digest: "source".into(),
                chunk_digests: vec![hex::encode(Sha256::digest([]))],
            },
            nonce_prefix: "nonce".into(),
            digest: "plaintext".into(),
            bytes: 0,
            chunk_count: 1,
        });
        saved.chunks.push(Chunk {
            item: 0,
            item_id: saved.items[0].id.clone(),
            position: 0,
            plaintext_bytes: 0,
            ciphertext_bytes: 16,
            checksum: hex::encode(Sha256::digest(&ciphertext)),
            artifact,
            complete: false,
            stage: None,
        });
        assert!(preparation_complete(&saved).unwrap());
    }

    #[test]
    fn peer_failure_event_message_is_sanitized() {
        assert_eq!(
            peer_failure_message(),
            "A live receiver disconnected or failed."
        );
    }

    #[test]
    fn retention_requires_a_discovered_choice() {
        assert_eq!(select_retention(24, &[6, 24], None).unwrap(), 24);
        assert_eq!(select_retention(24, &[6, 24], Some(6)).unwrap(), 6);
        assert!(select_retention(24, &[6, 24], Some(72)).is_err());
    }

    #[test]
    fn password_manifest_carries_browser_compatible_salt_without_password() {
        let mut saved = job("sending");
        saved.password_salt = Some(encode(&[3; 16]));
        let password_key = derive_password_key(b"eight-char", &[3; 16], 65_536, 3, 1).unwrap();
        saved.master_key = derive_password_protected_key(&saved.share_key, &password_key).unwrap();
        let envelope: serde_json::Value =
            serde_json::from_str(&encrypted_manifest(&saved, &Control::test_factory()).unwrap())
                .unwrap();
        assert_eq!(envelope["salt"], encode(&[3; 16]));
        assert_eq!(envelope["kdf"]["name"], "argon2id");
        assert_eq!(envelope["kdf"]["memory_kib"], 65_536);
        assert!(browser_password_metadata_is_valid(&envelope));
        assert!(!envelope.to_string().contains("eight-char"));
    }

    #[test]
    fn unprotected_manifest_and_descriptor_omit_password_metadata() {
        let saved = job("sending");
        let control = Control::test_factory();
        for encoded in [
            encrypted_manifest(&saved, &control).unwrap(),
            turbo_descriptor(&saved, saved.transfer_id.as_deref().unwrap(), &control).unwrap(),
        ] {
            let envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            assert!(envelope.get("salt").is_none());
            assert!(envelope.get("kdf").is_none());
            assert!(browser_password_metadata_is_valid(&envelope));
        }
    }

    fn browser_password_metadata_is_valid(envelope: &serde_json::Value) -> bool {
        let salt = envelope.get("salt");
        match salt {
            None | Some(serde_json::Value::Null) => envelope.get("kdf").is_none(),
            Some(serde_json::Value::String(value)) if !value.is_empty() => {
                let Some(kdf) = envelope.get("kdf") else {
                    return false;
                };
                kdf["name"] == "argon2id"
                    && kdf["memory_kib"] == 65_536
                    && kdf["iterations"] == 3
                    && kdf["parallelism"] == 1
            }
            _ => false,
        }
    }

    #[test]
    fn password_restart_keeps_delete_capability_but_not_working_key() {
        let mut saved = job("sending");
        saved.password_salt = Some(encode(&[3; 16]));
        saved.delete_token = Some("durable-delete-token".into());
        let password_key = derive_password_key(b"eight-char", &[3; 16], 65_536, 3, 1).unwrap();
        saved.master_key = derive_password_protected_key(&saved.share_key, &password_key).unwrap();

        let resumed: UploadJob =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert!(resumed.master_key.is_empty());
        assert_eq!(
            resumed.delete_token.as_deref(),
            Some("durable-delete-token")
        );
        assert_eq!(resumed.password_salt, Some(encode(&[3; 16])));
    }

    #[test]
    fn ended_live_checkpoint_preserves_recovery_and_is_not_a_pause() {
        let mut saved = job("ended");
        saved.driver = "webrtc".into();
        saved.join_token = Some("join".into());
        saved.exact_manifest = Some("published-manifest".into());
        validate_job(&saved).unwrap();
        assert_eq!(receipt(&saved).unwrap(), receipt(&job("sending")).unwrap());
    }

    #[test]
    fn recipient_envelope_uses_browser_account_aad() {
        let pair = filebeam_encryption::generate_account_keypair().unwrap();
        let transfer_key = [9; 32];
        let aad = b"filebeam:recipient:v1:01ARZ3NDEKTSV4RRFFQ69G5FAV:12:34";
        let envelope =
            filebeam_encryption::seal_key_for_recipient(&pair[32..], &transfer_key, aad).unwrap();
        assert_eq!(envelope.len(), 80);
        assert_eq!(
            filebeam_encryption::open_recipient_envelope(&pair[..32], &envelope, aad).unwrap(),
            transfer_key
        );
    }
}
