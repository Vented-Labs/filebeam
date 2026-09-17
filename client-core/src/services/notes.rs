use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_encryption::{
    decrypt_chunk, decrypt_manifest, derive_item_key, derive_password_key,
    derive_password_protected_key, encrypt_chunk, encrypt_manifest, generate_nonce_prefix,
    generate_transfer_key,
};
use filebeam_transfer_native::{
    control::{Control, Phase, ShareReady},
    runtime::shared_tokio_runtime,
    webrtc::{
        Signaling, connect_receiver_cancellable, connect_sender, request_chunk, serve_channel,
    },
};
use reqwest::{Url, blocking::Client, cookie::Jar, header::HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::{client::url, note_management::NoteManagementStore};

const TAG_BYTES: usize = 16;
const MAX_CONTROL_BODY_BYTES: usize = 1024 * 1024;
const MAX_PROTOCOL_CHUNK_BYTES: u64 = 24_999_984;
const MAX_CHUNKS: u64 = 65_535;

fn live_read_requires_peer_consent(driver: &str, relay_only: bool) -> bool {
    driver == "webrtc" && !relay_only
}

#[derive(Clone)]
pub struct NotesService {
    instance: Url,
    http: Client,
    async_http: reqwest::Client,
    consumed: Arc<Mutex<HashSet<String>>>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct NoteMetadata {
    pub id: String,
    pub status: String,
    pub burn_on_read: bool,
    pub encrypted_manifest: Option<String>,
    pub protocol_version: u8,
    pub chunk_bytes: u64,
    pub driver: String,
    pub items: Vec<NoteItem>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct NoteItem {
    pub id: String,
    pub position: u64,
    pub chunk_count: u64,
}
#[derive(Clone, Debug)]
pub struct NoteCreate {
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub password: Option<String>,
    pub burn_on_read: bool,
    pub retention_hours: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteTransport {
    Http,
    WebRtc,
}
#[derive(Clone, Debug)]
pub struct CreatedNote {
    pub id: String,
    pub link: String,
    pub(crate) delete_token: String,
}
#[derive(Clone, Debug)]
pub struct OpenedNote {
    pub id: String,
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub consumed: bool,
}
/// Metadata that can be displayed before a note is opened. It intentionally
/// excludes the encrypted envelope and never consumes a burn-on-read note.
#[derive(Clone, Debug)]
pub struct NoteInspection {
    pub id: String,
    pub status: String,
    pub burn_on_read: bool,
    pub transport: NoteTransport,
    pub password_required: bool,
}
pub struct NoteReceiveOptions<'a> {
    pub burn_acknowledged: bool,
    pub control: Option<&'a Control>,
}
pub struct PendingBurn {
    transfer_id: String,
    read_token: Zeroizing<String>,
    session_token: Option<String>,
}
pub struct ReceivedNote {
    pub note: OpenedNote,
    /// Present only when verified plaintext was received but removal failed.
    /// The capability remains process-memory-only in this value.
    pub pending_burn: Option<PendingBurn>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnResult {
    Consumed,
    AlreadyConsumed,
}
#[derive(Deserialize)]
struct Api<T> {
    data: T,
}
#[derive(Deserialize)]
struct Info {
    chunk_bytes: u64,
    #[serde(default)]
    transport_limits: Option<TransportLimits>,
    #[serde(default)]
    maximum_note_bytes: Option<u64>,
}
#[derive(Deserialize)]
struct TransportLimits {
    #[serde(default)]
    http: Option<DriverLimits>,
    #[serde(default)]
    webrtc: Option<DriverLimits>,
}
#[derive(Deserialize)]
struct DriverLimits {
    #[serde(default)]
    maximum_note_bytes: Option<u64>,
}
#[derive(Deserialize)]
struct Reservation {
    id: String,
    share_url: String,
    upload_token: String,
    delete_token: String,
    #[serde(default)]
    read_token: Option<String>,
    #[serde(default)]
    join_token: Option<String>,
    chunk_bytes: u64,
    driver: String,
    items: Vec<ReservedItem>,
}
#[derive(Deserialize)]
struct ReservedItem {
    id: String,
    position: u64,
}
#[derive(Deserialize)]
struct Envelope {
    v: u8,
    nonce_prefix: String,
    #[serde(default)]
    salt: Option<String>,
    ciphertext: String,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u8,
    items: Vec<ManifestItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_token: Option<String>,
}
#[derive(Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
struct DigestValue {
    algorithm: String,
    value: String,
}

impl NotesService {
    pub(crate) fn new(instance: Url, http: Client, cookies: Arc<Jar>) -> Self {
        Self {
            instance,
            http,
            async_http: reqwest::Client::builder()
                .cookie_provider(cookies)
                .build()
                .expect("create async note client"),
            consumed: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn read(&self, transfer_id: &str) -> Result<NoteMetadata> {
        let response = self
            .http
            .get(url(
                &self.instance,
                &format!("api/v1/transfers/{transfer_id}"),
            )?)
            .send()
            .context("read note")?;
        if !response.status().is_success() {
            bail!("note read returned {}", response.status());
        }
        let note = decode_control::<Api<NoteMetadata>>(response, "decode note metadata")?.data;
        if note.encrypted_manifest.is_none() {
            bail!("note has no encrypted envelope");
        }
        ensure!(
            note.protocol_version == 1
                && matches!(note.driver.as_str(), "http" | "webrtc")
                && note.items.len() == 1,
            "unsupported note transfer"
        );
        Ok(note)
    }

    /// Reads only server metadata. In particular, this does not
    /// request chunks, signal a peer, or claim a burn-on-read transfer.
    pub fn inspect(&self, link: &str) -> Result<NoteInspection> {
        let parsed = self.parse_link(link)?;
        let note = self.read(&parsed.id)?;
        let envelope: Envelope = serde_json::from_str(
            note.encrypted_manifest
                .as_deref()
                .context("note has no encrypted envelope")?,
        )?;
        ensure!(envelope.v == 1, "unsupported note envelope");
        ensure!(
            decode(&envelope.nonce_prefix)?.len() == 16,
            "invalid note manifest nonce"
        );
        Ok(NoteInspection {
            id: note.id,
            status: note.status,
            burn_on_read: note.burn_on_read,
            transport: match note.driver.as_str() {
                "http" => NoteTransport::Http,
                "webrtc" => NoteTransport::WebRtc,
                _ => bail!("unsupported note transfer"),
            },
            password_required: envelope.salt.is_some(),
        })
    }

    pub fn create(&self, request: NoteCreate) -> Result<CreatedNote> {
        validate_create(&request)?;
        let policy = self.policy()?;
        let chunk_bytes =
            usize::try_from(policy.chunk_bytes).context("note chunk policy is too large")?;
        let limit = policy.note_limit(NoteTransport::Http);
        let bytes = request.text.as_bytes();
        ensure!(
            limit.is_none_or(|limit| bytes.len() as u64 <= limit),
            "note exceeds the HTTP note limit"
        );
        let chunks = chunk_count(bytes.len() as u64, policy.chunk_bytes)?;
        let ciphertext_bytes = bytes
            .len()
            .checked_add(
                usize::try_from(chunks)?
                    .checked_mul(TAG_BYTES)
                    .context("note is too large")?,
            )
            .context("note is too large")?;
        let reservation = self
            .http
            .post(url(&self.instance, "api/v1/transfers")?)
            .json(&serde_json::json!({
                "kind":"note", "driver":"http", "protocol_version":1, "chunk_bytes":chunk_bytes,
                "retention_hours":request.retention_hours, "burn_on_read":request.burn_on_read,
                "items":[{"ciphertext_bytes":ciphertext_bytes,"chunk_count":chunks}]
            }))
            .send()
            .context("reserve note")?;
        if !reservation.status().is_success() {
            bail!("note reservation returned {}", reservation.status());
        }
        let reservation =
            decode_control::<Api<Reservation>>(reservation, "decode note reservation")?.data;
        ensure!(
            reservation.driver == "http"
                && reservation.chunk_bytes == policy.chunk_bytes
                && reservation.items.len() == 1,
            "invalid note reservation"
        );
        let item = &reservation.items[0];
        ensure!(item.position == 0, "invalid note item position");
        let share_key = Zeroizing::new(generate_transfer_key().context("generate note key")?);
        let salt = request
            .password
            .as_ref()
            .map(|_| generate_nonce_prefix().map(|value| value[..16].to_vec()))
            .transpose()?;
        let master_key = password_key(&share_key, request.password.as_deref(), salt.as_deref())?;
        let nonce_prefix = generate_nonce_prefix().context("generate note nonce")?;
        let item_key = Zeroizing::new(derive_item_key(&master_key, &reservation.id, &item.id)?);
        for (index, plaintext) in bytes.chunks(chunk_bytes).enumerate() {
            let ciphertext = encrypt_chunk(
                &item_key,
                &nonce_prefix,
                u32::try_from(index).context("note chunk index is too large")?,
                plaintext,
                aad(&reservation.id, &item.id, index as u64).as_bytes(),
            )?;
            let response = self
                .http
                .put(url(
                    &self.instance,
                    &format!(
                        "api/v1/transfers/{}/items/{}/chunks/{index}",
                        reservation.id, item.id
                    ),
                )?)
                .header("X-Filebeam-Upload-Token", &reservation.upload_token)
                .body(ciphertext)
                .send()
                .context("upload encrypted note chunk")?;
            if !response.status().is_success() {
                bail!("note chunk upload returned {}", response.status());
            }
        }
        // An empty note is represented by a single authenticated empty chunk.
        if bytes.is_empty() {
            let ciphertext = encrypt_chunk(
                &item_key,
                &nonce_prefix,
                0,
                b"",
                aad(&reservation.id, &item.id, 0).as_bytes(),
            )?;
            let response = self
                .http
                .put(url(
                    &self.instance,
                    &format!(
                        "api/v1/transfers/{}/items/{}/chunks/0",
                        reservation.id, item.id
                    ),
                )?)
                .header("X-Filebeam-Upload-Token", &reservation.upload_token)
                .body(ciphertext)
                .send()?;
            if !response.status().is_success() {
                bail!("note chunk upload returned {}", response.status());
            }
        }
        let manifest = Manifest {
            version: 1,
            title: request.title.filter(|title| !title.is_empty()),
            language: Some(request.language),
            read_token: reservation.read_token.clone(),
            items: vec![ManifestItem {
                id: item.id.clone(),
                name: "note.txt".into(),
                mime: "text/plain".into(),
                size: bytes.len() as u64,
                nonce_prefix: encode(&nonce_prefix),
                chunk_count: chunks,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: hex_digest(bytes),
                },
            }],
        };
        let manifest_prefix = generate_nonce_prefix()?;
        let manifest_ciphertext = encrypt_manifest(
            &master_key,
            &manifest_prefix,
            &serde_json::to_vec(&manifest)?,
            aad(&reservation.id, "manifest", "manifest").as_bytes(),
        )?;
        let envelope = serde_json::json!({"v":1,"nonce_prefix":encode(&manifest_prefix),"salt":salt.as_ref().map(|salt| encode(salt)),"kdf":salt.as_ref().map(|_| serde_json::json!({"name":"argon2id","memory_kib":65536,"iterations":3,"parallelism":1})),"ciphertext":encode(&manifest_ciphertext)}).to_string();
        let response = self
            .http
            .post(url(
                &self.instance,
                &format!("api/v1/transfers/{}/complete", reservation.id),
            )?)
            .header("X-Filebeam-Upload-Token", &reservation.upload_token)
            .json(&serde_json::json!({"encrypted_manifest":envelope}))
            .send()
            .context("complete note")?;
        if !response.status().is_success() {
            bail!("note completion returned {}", response.status());
        }
        let share_url = self
            .instance
            .join(reservation.share_url.trim_start_matches('/'))?;
        Ok(CreatedNote {
            id: reservation.id,
            link: format!("{}#k=v1.{}", share_url, encode(&share_key)),
            delete_token: reservation.delete_token,
        })
    }

    pub fn save_management(
        &self,
        note: &CreatedNote,
        management: &NoteManagementStore,
    ) -> Result<()> {
        management.save(&note.id, &self.instance, &note.delete_token, None)
    }

    /// Job-oriented HTTP creation. The note is not checkpointable because its
    /// encryption key and delete capability deliberately remain memory-only.
    pub fn create_with_control(
        &self,
        request: NoteCreate,
        control: &Control,
    ) -> Result<CreatedNote> {
        control.phase(Phase::Preparing)?;
        control.check()?;
        control.phase(Phase::Encrypting)?;
        let created = self.create(request)?;
        control.check()?;
        control.phase(Phase::Finalizing)?;
        Ok(created)
    }

    /// Owns a live note until cancelled. Ciphertext never leaves this process
    /// except through the authenticated WebRTC data channel.
    pub fn create_live(
        &self,
        request: NoteCreate,
        control: &Control,
        end_requested: Arc<AtomicBool>,
        management: Option<NoteManagementStore>,
    ) -> Result<CreatedNote> {
        validate_create(&request)?;
        if !control.webrtc_relay_only() {
            control.request_peer_consent(self.instance.origin().ascii_serialization())?;
        }
        let policy = self.policy()?;
        let chunk_bytes = usize::try_from(policy.chunk_bytes)?;
        let bytes = request.text.as_bytes();
        ensure!(
            policy
                .note_limit(NoteTransport::WebRtc)
                .is_none_or(|limit| bytes.len() as u64 <= limit),
            "note exceeds the WebRTC note limit"
        );
        let chunk_count = chunk_count(bytes.len() as u64, policy.chunk_bytes)?;
        let ciphertext_bytes = bytes
            .len()
            .checked_add(
                usize::try_from(chunk_count)?
                    .checked_mul(TAG_BYTES)
                    .context("note is too large")?,
            )
            .context("note is too large")?;
        let reservation = self
            .http
            .post(url(&self.instance, "api/v1/transfers")?)
            .json(&serde_json::json!({
                "kind":"note", "driver":"webrtc", "protocol_version":1, "chunk_bytes":chunk_bytes,
                "retention_hours":request.retention_hours, "burn_on_read":request.burn_on_read,
                "items":[{"ciphertext_bytes":ciphertext_bytes,"chunk_count":chunk_count}]
            }))
            .send()?;
        if !reservation.status().is_success() {
            bail!("live note reservation returned {}", reservation.status());
        }
        let reservation =
            decode_control::<Api<Reservation>>(reservation, "decode live note reservation")?.data;
        ensure!(
            reservation.driver == "webrtc"
                && reservation.chunk_bytes == policy.chunk_bytes
                && reservation.items.len() == 1,
            "invalid live note reservation"
        );
        let join_token = reservation
            .join_token
            .clone()
            .context("live note join capability is unavailable")?;
        let item = &reservation.items[0];
        let share_key = Zeroizing::new(generate_transfer_key()?);
        let salt = request
            .password
            .as_ref()
            .map(|_| generate_nonce_prefix().map(|v| v[..16].to_vec()))
            .transpose()?;
        let master = password_key(&share_key, request.password.as_deref(), salt.as_deref())?;
        let prefix = generate_nonce_prefix()?;
        let key = Zeroizing::new(derive_item_key(&master, &reservation.id, &item.id)?);
        let mut chunks = HashMap::new();
        for (index, plain) in bytes.chunks(chunk_bytes).enumerate() {
            chunks.insert(
                (item.id.clone(), index as u64),
                encrypt_chunk(
                    &key,
                    &prefix,
                    u32::try_from(index).context("note chunk index is too large")?,
                    plain,
                    aad(&reservation.id, &item.id, index).as_bytes(),
                )?,
            );
        }
        let manifest = Manifest {
            version: 1,
            title: request.title.filter(|title| !title.is_empty()),
            language: Some(request.language),
            read_token: reservation.read_token.clone(),
            items: vec![ManifestItem {
                id: item.id.clone(),
                name: "note.txt".into(),
                mime: "text/plain".into(),
                size: bytes.len() as u64,
                nonce_prefix: encode(&prefix),
                chunk_count,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: hex_digest(bytes),
                },
            }],
        };
        let mp = generate_nonce_prefix()?;
        let mut value = serde_json::to_value(&manifest)?;
        value["join_token"] = serde_json::Value::String(join_token);
        let encrypted_manifest = serde_json::json!({"v":1,"nonce_prefix":encode(&mp),"salt":salt.as_ref().map(|x| encode(x)),"kdf":salt.as_ref().map(|_| serde_json::json!({"name":"argon2id","memory_kib":65536,"iterations":3,"parallelism":1})),"ciphertext":encode(&encrypt_manifest(&master, &mp, &serde_json::to_vec(&value)?, aad(&reservation.id,"manifest","manifest").as_bytes())?)}).to_string();
        let signal = Signaling::new(
            self.async_http.clone(),
            self.instance.as_str(),
            &reservation.id,
        )?;
        shared_tokio_runtime()
            .block_on(signal.publish(&reservation.upload_token, &encrypted_manifest))?;
        let share_url = self
            .instance
            .join(reservation.share_url.trim_start_matches('/'))?;
        let created = CreatedNote {
            id: reservation.id.clone(),
            link: format!("{}#k=v1.{}", share_url, encode(&share_key)),
            delete_token: reservation.delete_token,
        };
        if let Some(management) = &management {
            management.save(
                &created.id,
                &self.instance,
                &created.delete_token,
                Some(&reservation.upload_token),
            )?;
        }
        control.emit(
            filebeam_transfer_native::control::TransferEvent::ShareReady(ShareReady {
                share_url: created.link.clone(),
            }),
        );
        control.phase(Phase::Waiting)?;
        shared_tokio_runtime().block_on(async {
            let mut served = HashSet::new();
            while !control.cancelled.load(Ordering::Relaxed) {
                let (sessions, servers) = signal.sender_sessions(&reservation.upload_token).await?;
                for session in sessions
                    .into_iter()
                    .filter(|session| session.offer.is_some() && served.insert(session.id.clone()))
                {
                    let (peer, channel) = connect_sender(
                        &signal,
                        &reservation.upload_token,
                        &session,
                        &servers,
                        control.webrtc_relay_only(),
                        control.cancelled.clone(),
                    )
                    .await?;
                    let payload = chunks.clone();
                    tokio::spawn(async move {
                        let _peer = peer;
                        let _ = serve_channel(channel, payload).await;
                    });
                }
                tokio::time::sleep(Duration::from_millis(1800)).await;
            }
            if end_requested.load(Ordering::Relaxed) {
                let mut last = None;
                for attempt in 0..3 {
                    match signal.end(&reservation.upload_token).await {
                        Ok(()) => return Ok(()),
                        Err(error) => last = Some(error),
                    }
                    tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
                }
                Err(last.expect("live end retry recorded an error"))
            } else {
                Ok(())
            }
        })?;
        if end_requested.load(Ordering::Relaxed)
            && let Some(management) = management
        {
            management.acknowledge_end(&created.id)?;
        }
        Ok(created)
    }

    /// The caller must supply a control when opening a live note. This keeps the
    /// address-exposure decision and cancellation lifetime with the owning job.
    pub fn open(&self, link: &str, password: Option<&str>) -> Result<OpenedNote> {
        self.open_controlled(link, password, None)
    }

    pub fn open_controlled(
        &self,
        link: &str,
        password: Option<&str>,
        control: Option<&Control>,
    ) -> Result<OpenedNote> {
        let received = self.receive(
            link,
            password,
            NoteReceiveOptions {
                burn_acknowledged: true,
                control,
            },
        )?;
        if received.pending_burn.is_some() {
            bail!("verified note could not be removed; use the managed receive API to retry")
        }
        Ok(received.note)
    }

    /// Opens a note under a transfer control. Burn-on-read notes require an
    /// explicit acknowledgement before a WebRTC peer is constructed. On a
    /// removal failure, verified plaintext is returned with a memory-only
    /// retry handle rather than being discarded.
    pub fn receive(
        &self,
        link: &str,
        password: Option<&str>,
        options: NoteReceiveOptions<'_>,
    ) -> Result<ReceivedNote> {
        if let Some(control) = options.control {
            control.check()?;
        }
        let parsed = self.parse_link(link)?;
        let share_key = Zeroizing::new(parsed.key.context("note link has no decryption key")?);
        let note = self.read(&parsed.id)?;
        let envelope: Envelope = serde_json::from_str(note.encrypted_manifest.as_deref().unwrap())?;
        ensure!(envelope.v == 1, "unsupported note envelope");
        ensure!(
            decode(&envelope.nonce_prefix)?.len() == 16,
            "invalid note manifest nonce"
        );
        let master_key = password_key(
            &share_key,
            password,
            envelope.salt.as_deref().map(decode).transpose()?.as_deref(),
        )?;
        let manifest_bytes = decrypt_manifest(
            &master_key,
            &decode(&envelope.nonce_prefix)?,
            &decode(&envelope.ciphertext)?,
            aad(&note.id, "manifest", "manifest").as_bytes(),
        )
        .context("could not decrypt note manifest")?;
        let join_token = serde_json::from_slice::<serde_json::Value>(&manifest_bytes)
            .ok()
            .and_then(|v| {
                v.get("join_token")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            });
        let manifest: Manifest =
            serde_json::from_slice(&manifest_bytes).context("invalid decrypted note manifest")?;
        ensure!(
            manifest.version == 1 && manifest.items.len() == 1,
            "invalid note manifest"
        );
        let item = &manifest.items[0];
        let server_item = &note.items[0];
        ensure!(
            note.chunk_bytes > 0 && note.chunk_bytes <= MAX_PROTOCOL_CHUNK_BYTES,
            "invalid note chunk size"
        );
        ensure!(
            item.id == server_item.id
                && item.chunk_count == server_item.chunk_count
                && item.chunk_count == chunk_count(item.size, note.chunk_bytes)?
                && item.size
                    <= item
                        .chunk_count
                        .checked_mul(note.chunk_bytes)
                        .context("note is too large")?,
            "note manifest does not match transfer"
        );
        ensure!(
            decode(&item.nonce_prefix)?.len() == 16,
            "invalid note item nonce"
        );
        ensure!(
            item.digest.algorithm == "sha256"
                && item.digest.value.len() == 64
                && item
                    .digest
                    .value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
            "invalid note digest"
        );
        let resource_limit = options
            .control
            .map(Control::memory_budget)
            .unwrap_or(u64::MAX);
        ensure!(
            item.size <= resource_limit,
            "note exceeds the receive resource budget"
        );
        if note.burn_on_read {
            ensure!(
                options.burn_acknowledged,
                "burn-on-read was not acknowledged"
            );
        }
        let key = Zeroizing::new(derive_item_key(&master_key, &note.id, &item.id)?);
        let prefix = decode(&item.nonce_prefix)?;
        let mut text = Vec::new();
        text.try_reserve(
            usize::try_from(item.size).context("note is too large for this platform")?,
        )
        .context("note exceeds the receive resource budget")?;
        let live_session = if note.driver == "webrtc" {
            let token = join_token.context("live note manifest has no join capability")?;
            let control = options
                .control
                .context("opening a live note requires a controlled receive")?;
            let signal = Signaling::new(self.async_http.clone(), self.instance.as_str(), &note.id)?;
            let relay_only = control.webrtc_relay_only();
            if live_read_requires_peer_consent(&note.driver, relay_only) {
                // ICE gathering can reveal peer addresses, so consent precedes session registration.
                control.request_peer_consent(self.instance.origin().ascii_serialization())?;
            }
            Some(shared_tokio_runtime().block_on(async {
                let (peer, session, channel) = connect_receiver_cancellable(
                    &signal,
                    &token,
                    relay_only,
                    control.cancelled.clone(),
                )
                .await?;
                Ok::<_, anyhow::Error>((peer, session, channel, signal))
            })?)
        } else {
            None
        };
        for index in 0..item.chunk_count {
            if let Some(control) = options.control {
                control.check()?;
            }
            let ciphertext = if let Some((_, _session, channel, _)) = &live_session {
                let plaintext = if index + 1 == item.chunk_count {
                    item.size
                        .checked_sub(
                            index
                                .checked_mul(note.chunk_bytes)
                                .context("note is too large")?,
                        )
                        .context("invalid note chunk size")?
                } else {
                    note.chunk_bytes
                };
                let expected = plaintext
                    .checked_add(TAG_BYTES as u64)
                    .context("note chunk is too large")?;
                shared_tokio_runtime().block_on(request_chunk(
                    channel.clone(),
                    (index + 1) as u32,
                    item.id.clone(),
                    index,
                    expected,
                ))?
            } else {
                let response = self
                    .http
                    .get(url(
                        &self.instance,
                        &format!(
                            "api/v1/transfers/{}/items/{}/chunks/{index}",
                            note.id, item.id
                        ),
                    )?)
                    .send()
                    .context("download encrypted note chunk")?;
                if !response.status().is_success() {
                    bail!("note chunk download returned {}", response.status());
                }
                read_chunk(
                    response,
                    expected_chunk_bytes(item.size, note.chunk_bytes, index)?,
                )?
            };
            ensure!(
                ciphertext.len() as u64
                    == expected_chunk_bytes(item.size, note.chunk_bytes, index)?,
                "note chunk length is invalid"
            );
            text.extend(decrypt_chunk(
                &key,
                &prefix,
                index as u32,
                &ciphertext,
                aad(&note.id, &item.id, index).as_bytes(),
            )?);
        }
        ensure!(
            text.len() as u64 == item.size
                && hex_digest(&text) == item.digest.value
                && item.digest.algorithm == "sha256",
            "note integrity check failed"
        );
        let text = String::from_utf8(text).context("note is not valid UTF-8")?;
        if let Some((peer, session, _, signal)) = &live_session {
            shared_tokio_runtime().block_on(signal.report(session, "completed", 100))?;
            shared_tokio_runtime().block_on(peer.close());
        }
        let pending_burn = if note.burn_on_read {
            let token = manifest
                .read_token
                .as_deref()
                .context("burn note has no read token")?;
            let session_token = live_session
                .as_ref()
                .map(|(_, session, _, _)| session.token.clone());
            let result = if let Some(session_token) = session_token.as_deref() {
                self.consume_live(&note.id, token, session_token)
            } else {
                self.consume_once(&note.id, token)
            };
            match result {
                Ok(BurnResult::Consumed | BurnResult::AlreadyConsumed) => None,
                Err(_) => Some(PendingBurn {
                    transfer_id: note.id.clone(),
                    read_token: Zeroizing::new(token.to_owned()),
                    session_token,
                }),
            }
        } else {
            None
        };
        Ok(ReceivedNote {
            note: OpenedNote {
                id: note.id,
                text,
                title: manifest.title,
                language: manifest.language.unwrap_or_else(|| "plain".into()),
                consumed: note.burn_on_read && pending_burn.is_none(),
            },
            pending_burn,
        })
    }

    /// Retries only the authenticated removal request held by a successful
    /// receive. The token is never serialized, logged, or written to disk.
    pub fn retry_burn(&self, pending: &PendingBurn) -> Result<BurnResult> {
        if let Some(session_token) = pending.session_token.as_deref() {
            self.consume_live(&pending.transfer_id, &pending.read_token, session_token)
        } else {
            self.consume_once(&pending.transfer_id, &pending.read_token)
        }
    }

    pub fn consume(&self, transfer_id: &str, read_token: &str) -> Result<BurnResult> {
        self.consume_request(transfer_id, read_token)
    }

    /// Revokes a note using its creator capability. Callers must retain this
    /// capability in platform secure storage; it is never included in job state.
    pub fn revoke(&self, transfer_id: &str, delete_token: &str) -> Result<()> {
        let response = self
            .http
            .delete(url(
                &self.instance,
                &format!("api/v1/transfers/{transfer_id}"),
            )?)
            .header(
                "X-Filebeam-Delete-Token",
                HeaderValue::from_str(delete_token)
                    .context("delete token contains invalid characters")?,
            )
            .send()
            .context("revoke note")?;
        ensure!(
            response.status().as_u16() == 202,
            "note revocation returned {}",
            response.status()
        );
        Ok(())
    }
    fn consume_once(&self, transfer_id: &str, token: &str) -> Result<BurnResult> {
        if self.consumed.lock().unwrap().contains(transfer_id) {
            return Ok(BurnResult::AlreadyConsumed);
        }
        let result = self.consume_request(transfer_id, token)?;
        if result == BurnResult::Consumed {
            self.consumed.lock().unwrap().insert(transfer_id.into());
        }
        Ok(result)
    }
    fn consume_request(&self, transfer_id: &str, read_token: &str) -> Result<BurnResult> {
        let token =
            HeaderValue::from_str(read_token).context("read token contains invalid characters")?;
        let response = self
            .http
            .post(url(
                &self.instance,
                &format!("api/v1/transfers/{transfer_id}/consume"),
            )?)
            .header("X-Filebeam-Read-Token", token)
            .json(&serde_json::json!({}))
            .send()
            .context("consume burn note")?;
        match response.status().as_u16() {
            202 => Ok(BurnResult::Consumed),
            404 => Ok(BurnResult::AlreadyConsumed),
            status => bail!("burn note consumption returned {status}"),
        }
    }
    fn consume_live(
        &self,
        transfer_id: &str,
        read_token: &str,
        session_token: &str,
    ) -> Result<BurnResult> {
        let response = self
            .http
            .post(url(
                &self.instance,
                &format!("api/v1/transfers/{transfer_id}/consume"),
            )?)
            .header("X-Filebeam-Read-Token", HeaderValue::from_str(read_token)?)
            .header(
                "X-Filebeam-Session-Token",
                HeaderValue::from_str(session_token)?,
            )
            .json(&serde_json::json!({}))
            .send()?;
        match response.status().as_u16() {
            202 => Ok(BurnResult::Consumed),
            404 => Ok(BurnResult::AlreadyConsumed),
            status => bail!("burn note consumption returned {status}"),
        }
    }
}
impl NotesService {
    fn policy(&self) -> Result<Info> {
        let response = self
            .http
            .get(url(&self.instance, "api/v1/info")?)
            .timeout(Duration::from_secs(15))
            .send()
            .context("read note policy")?;
        if !response.status().is_success() {
            bail!("note policy returned {}", response.status());
        }
        let policy = decode_control::<Api<Info>>(response, "decode note policy")?.data;
        ensure!(
            policy.chunk_bytes > 0 && policy.chunk_bytes <= MAX_PROTOCOL_CHUNK_BYTES,
            "invalid note chunk policy"
        );
        Ok(policy)
    }
    fn parse_link(&self, link: &str) -> Result<filebeam_transfer_native::protocol::Link> {
        let parsed = filebeam_transfer_native::protocol::parse_link_for_instance(
            link,
            self.instance.as_str(),
        )?;
        ensure!(
            parsed.instance == self.instance.origin().ascii_serialization(),
            "note link belongs to a different instance"
        );
        Ok(parsed)
    }
}
impl Info {
    fn note_limit(&self, transport: NoteTransport) -> Option<u64> {
        self.transport_limits
            .as_ref()
            .and_then(|limits| match transport {
                NoteTransport::Http => limits.http.as_ref(),
                NoteTransport::WebRtc => limits.webrtc.as_ref(),
            })
            .and_then(|limit| limit.maximum_note_bytes)
            .or(self.maximum_note_bytes)
    }
}
fn chunk_count(size: u64, chunk_bytes: u64) -> Result<u64> {
    ensure!(
        chunk_bytes > 0 && chunk_bytes <= MAX_PROTOCOL_CHUNK_BYTES,
        "invalid note chunk policy"
    );
    let chunks = size.div_ceil(chunk_bytes).max(1);
    ensure!(
        chunks <= MAX_CHUNKS,
        "note exceeds the protocol chunk limit"
    );
    Ok(chunks)
}
fn decode_control<T: serde::de::DeserializeOwned>(
    mut response: reqwest::blocking::Response,
    context: &str,
) -> Result<T> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CONTROL_BODY_BYTES as u64)
    {
        bail!("control response exceeds its limit");
    }
    let mut body = Vec::new();
    response
        .by_ref()
        .take((MAX_CONTROL_BODY_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .with_context(|| context.to_owned())?;
    ensure!(
        body.len() <= MAX_CONTROL_BODY_BYTES,
        "control response exceeds its limit"
    );
    serde_json::from_slice(&body).with_context(|| context.to_owned())
}
fn expected_chunk_bytes(size: u64, chunk_bytes: u64, index: u64) -> Result<u64> {
    let count = chunk_count(size, chunk_bytes)?;
    ensure!(index < count, "note chunk index is invalid");
    let plaintext = if index + 1 == count {
        size.checked_sub(
            index
                .checked_mul(chunk_bytes)
                .context("note is too large")?,
        )
        .context("note chunk is invalid")?
    } else {
        chunk_bytes
    };
    plaintext
        .checked_add(TAG_BYTES as u64)
        .context("note chunk is too large")
}
fn read_chunk(mut response: reqwest::blocking::Response, expected: u64) -> Result<Vec<u8>> {
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length == expected),
        "note chunk length is invalid"
    );
    let mut body = Vec::new();
    response
        .by_ref()
        .take(expected.checked_add(1).context("note chunk is too large")?)
        .read_to_end(&mut body)
        .context("read encrypted note chunk")?;
    ensure!(
        body.len() as u64 == expected,
        "note chunk length is invalid"
    );
    Ok(body)
}
fn validate_create(request: &NoteCreate) -> Result<()> {
    ensure!(!request.text.is_empty(), "note cannot be empty");
    ensure!(
        request
            .title
            .as_ref()
            .is_none_or(|title| title.chars().count() <= 160),
        "note title is too long"
    );
    ensure!(
        matches!(
            request.language.as_str(),
            "plain"
                | "php"
                | "dotenv"
                | "javascript"
                | "typescript"
                | "json"
                | "markdown"
                | "css"
                | "html"
        ),
        "unsupported note language"
    );
    ensure!(
        request
            .password
            .as_ref()
            .is_none_or(|password| !password.is_empty()),
        "password cannot be empty"
    );
    Ok(())
}
fn aad(transfer: &str, item: &str, position: impl std::fmt::Display) -> String {
    format!("filebeam:v1:{transfer}:{item}:{position}")
}
fn encode(value: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(value)
}
fn decode(value: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(value)
        .context("invalid base64url note envelope")
}
fn hex_digest(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn password_key(
    share_key: &[u8],
    password: Option<&str>,
    salt: Option<&[u8]>,
) -> Result<Zeroizing<Vec<u8>>> {
    match (password, salt) {
        (None, None) => Ok(Zeroizing::new(share_key.to_vec())),
        (Some(password), Some(salt)) => {
            // Argon2 uses 64 MiB. Serialize native service KDFs so independent
            // note opens cannot multiply that allocation beyond the process budget.
            static KDF: OnceLock<Mutex<()>> = OnceLock::new();
            let _permit = KDF.get_or_init(|| Mutex::new(())).lock().unwrap();
            Ok(Zeroizing::new(derive_password_protected_key(
                share_key,
                &derive_password_key(password.as_bytes(), salt, 65_536, 3, 1)?,
            )?))
        }
        (None, Some(_)) => bail!("this note requires its password"),
        (Some(_), None) => bail!("this note does not use a password"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ServiceClient;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn fixture(origin: &str) -> (String, String, Vec<u8>) {
        let transfer = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let item = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
        let share = [7_u8; 32];
        let prefix = [9_u8; 16];
        let key = derive_item_key(&share, transfer, item).unwrap();
        let chunk = encrypt_chunk(
            &key,
            &prefix,
            0,
            b"verified text",
            aad(transfer, item, 0).as_bytes(),
        )
        .unwrap();
        let manifest = Manifest {
            version: 1,
            title: Some("Test".into()),
            language: Some("plain".into()),
            read_token: Some("a".repeat(64)),
            items: vec![ManifestItem {
                id: item.into(),
                name: "note.txt".into(),
                mime: "text/plain".into(),
                size: 13,
                nonce_prefix: encode(&prefix),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: hex_digest(b"verified text"),
                },
            }],
        };
        let mp = [3_u8; 16];
        let envelope = serde_json::json!({
            "v": 1, "nonce_prefix": encode(&mp), "ciphertext": encode(&encrypt_manifest(
                &share, &mp, &serde_json::to_vec(&manifest).unwrap(), aad(transfer, "manifest", "manifest").as_bytes()
            ).unwrap())
        }).to_string();
        let metadata = serde_json::json!({"data": {
            "id": transfer, "status": "available", "burn_on_read": true,
            "encrypted_manifest": envelope, "protocol_version": 1, "chunk_bytes": 1024,
            "driver": "http", "items": [{"id": item, "position": 0, "chunk_count": 1}]
        }})
        .to_string();
        (
            format!("{origin}/{transfer}#k=v1.{}", encode(&share)),
            metadata,
            chunk,
        )
    }

    #[test]
    fn browser_format_note_envelope_round_trips_with_password() {
        let transfer = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let item = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
        let share = [7_u8; 32];
        let salt = [8_u8; 16];
        let master = password_key(&share, Some("correct password"), Some(&salt)).unwrap();
        let prefix = [9_u8; 16];
        let item_key = derive_item_key(&master, transfer, item).unwrap();
        let ciphertext = encrypt_chunk(
            &item_key,
            &prefix,
            0,
            b"browser compatible",
            aad(transfer, item, 0).as_bytes(),
        )
        .unwrap();
        assert_eq!(
            decrypt_chunk(
                &item_key,
                &prefix,
                0,
                &ciphertext,
                aad(transfer, item, 0).as_bytes()
            )
            .unwrap(),
            b"browser compatible"
        );
        let envelope_prefix = [3_u8; 16];
        let manifest = encrypt_manifest(
            &master,
            &envelope_prefix,
            br#"{"version":1,"items":[]}"#,
            aad(transfer, "manifest", "manifest").as_bytes(),
        )
        .unwrap();
        assert_eq!(
            decrypt_manifest(
                &master,
                &envelope_prefix,
                &manifest,
                aad(transfer, "manifest", "manifest").as_bytes()
            )
            .unwrap(),
            br#"{"version":1,"items":[]}"#
        );
        assert!(password_key(&share, Some("wrong password"), Some(&salt)).is_ok());
        let wrong = password_key(&share, Some("wrong password"), Some(&salt)).unwrap();
        assert!(
            decrypt_manifest(
                &wrong,
                &envelope_prefix,
                &manifest,
                aad(transfer, "manifest", "manifest").as_bytes()
            )
            .is_err()
        );
    }

    #[test]
    fn password_envelope_requires_a_password_after_restart() {
        assert!(password_key(&[1_u8; 32], None, Some(&[2_u8; 16])).is_err());
        assert!(password_key(&[1_u8; 32], Some("password"), None).is_err());
    }

    #[test]
    fn relay_only_live_read_has_no_direct_peer_consent_or_fallback() {
        assert!(!live_read_requires_peer_consent("webrtc", true));
        assert!(live_read_requires_peer_consent("webrtc", false));
        assert!(!live_read_requires_peer_consent("http", false));
    }

    #[test]
    fn note_chunk_planner_rejects_overflow_and_protocol_excess() {
        assert_eq!(chunk_count(0, 1024).unwrap(), 1);
        assert_eq!(chunk_count(1025, 1024).unwrap(), 2);
        assert!(chunk_count(1, 0).is_err());
        assert!(
            chunk_count(
                MAX_PROTOCOL_CHUNK_BYTES * (MAX_CHUNKS + 1),
                MAX_PROTOCOL_CHUNK_BYTES
            )
            .is_err()
        );
        assert!(expected_chunk_bytes(1, 1024, 1).is_err());
    }

    #[test]
    fn http_burn_is_verified_and_removal_failure_keeps_plaintext_for_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (_, metadata, chunk) = fixture(&origin);
        thread::spawn(move || {
            for (status, body) in [
                (200, metadata.into_bytes()),
                (200, chunk),
                (500, Vec::new()),
                (202, Vec::new()),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        let (link, _, _) = fixture(&origin);
        let client = ServiceClient::new(&origin).unwrap();
        let mut received = client
            .notes()
            .receive(
                &link,
                None,
                NoteReceiveOptions {
                    burn_acknowledged: true,
                    control: None,
                },
            )
            .unwrap();
        assert_eq!(received.note.text, "verified text");
        assert!(!received.note.consumed);
        assert!(received.pending_burn.is_some());
        assert_eq!(
            client
                .notes()
                .retry_burn(received.pending_burn.as_ref().unwrap())
                .unwrap(),
            BurnResult::Consumed
        );
        received.pending_burn = None;
        assert_eq!(received.note.text, "verified text");
    }

    #[test]
    fn cancelled_receive_does_not_issue_a_request() {
        let client = ServiceClient::new("http://127.0.0.1:9").unwrap();
        let control = Control::test_factory();
        control.cancel();
        assert!(
            client
                .notes()
                .receive(
                    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                    None,
                    NoteReceiveOptions {
                        burn_acknowledged: true,
                        control: Some(&control),
                    }
                )
                .is_err()
        );
    }
}
