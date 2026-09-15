use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
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
    webrtc::{Signaling, connect_receiver, connect_sender, request_chunk, serve_channel},
};
use reqwest::{Url, blocking::Client, cookie::Jar, header::HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::client::url;

const TAG_BYTES: usize = 16;

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
    pub delete_token: String,
}
#[derive(Clone, Debug)]
pub struct OpenedNote {
    pub id: String,
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub consumed: bool,
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
        let note = response
            .json::<Api<NoteMetadata>>()
            .context("decode note metadata")?
            .data;
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

    pub fn create(&self, request: NoteCreate) -> Result<CreatedNote> {
        ensure!(!request.text.is_empty(), "note cannot be empty");
        ensure!(
            request
                .title
                .as_ref()
                .is_none_or(|title| title.len() <= 160),
            "note title is too long"
        );
        ensure!(
            request
                .password
                .as_ref()
                .is_none_or(|password| !password.is_empty()),
            "password cannot be empty"
        );
        let info = self
            .http
            .get(url(&self.instance, "api/v1/info")?)
            .send()
            .context("read note policy")?;
        if !info.status().is_success() {
            bail!("note policy returned {}", info.status());
        }
        let chunk_bytes = info.json::<Api<Info>>()?.data.chunk_bytes as usize;
        ensure!(
            chunk_bytes > 0 && chunk_bytes <= 24_999_984,
            "invalid note chunk policy"
        );
        let bytes = request.text.as_bytes();
        let chunks = bytes.len().div_ceil(chunk_bytes).max(1);
        ensure!(chunks <= 65_535, "note exceeds the protocol chunk limit");
        let ciphertext_bytes = bytes
            .len()
            .checked_add(chunks * TAG_BYTES)
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
        let reservation = reservation.json::<Api<Reservation>>()?.data;
        ensure!(
            reservation.driver == "http"
                && reservation.chunk_bytes as usize == chunk_bytes
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
                index as u32,
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
                chunk_count: chunks as u64,
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

    /// Owns a live note until cancelled. Ciphertext never leaves this process
    /// except through the authenticated WebRTC data channel.
    pub fn create_live(&self, request: NoteCreate, control: &Control) -> Result<CreatedNote> {
        ensure!(!request.text.is_empty(), "note cannot be empty");
        if !control.webrtc_relay_only() {
            control.request_peer_consent(self.instance.origin().ascii_serialization())?;
        }
        let info = self
            .http
            .get(url(&self.instance, "api/v1/info")?)
            .send()?
            .json::<Api<Info>>()?
            .data;
        let chunk_bytes = usize::try_from(info.chunk_bytes)?;
        ensure!(
            chunk_bytes > 0 && chunk_bytes <= 24_999_984,
            "invalid note chunk policy"
        );
        let bytes = request.text.as_bytes();
        let chunk_count = bytes.len().div_ceil(chunk_bytes).max(1);
        let reservation = self.http.post(url(&self.instance, "api/v1/transfers")?).json(&serde_json::json!({
            "kind":"note", "driver":"webrtc", "protocol_version":1, "chunk_bytes":chunk_bytes,
            "retention_hours":request.retention_hours, "burn_on_read":request.burn_on_read,
            "items":[{"ciphertext_bytes":bytes.len() + chunk_count * TAG_BYTES,"chunk_count":chunk_count}]
        })).send()?;
        if !reservation.status().is_success() {
            bail!("live note reservation returned {}", reservation.status());
        }
        let reservation = reservation.json::<Api<Reservation>>()?.data;
        ensure!(
            reservation.driver == "webrtc" && reservation.items.len() == 1,
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
                    index as u32,
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
                chunk_count: chunk_count as u64,
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
        control.emit(
            filebeam_transfer_native::control::TransferEvent::ShareReady(ShareReady {
                share_url: created.link.clone(),
            }),
        );
        control.phase(Phase::Waiting)?;
        shared_tokio_runtime().block_on(async {
            let mut served = HashSet::new();
            while !control.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
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
            signal.end(&reservation.upload_token).await
        })?;
        Ok(created)
    }

    pub fn open(&self, link: &str, password: Option<&str>) -> Result<OpenedNote> {
        let parsed = filebeam_transfer_native::protocol::parse_link_for_instance(
            link,
            self.instance.as_str(),
        )?;
        ensure!(
            parsed.instance == self.instance.origin().ascii_serialization(),
            "note link belongs to a different instance"
        );
        let share_key = Zeroizing::new(parsed.key.context("note link has no decryption key")?);
        let note = self.read(&parsed.id)?;
        let envelope: Envelope = serde_json::from_str(note.encrypted_manifest.as_deref().unwrap())?;
        ensure!(envelope.v == 1, "unsupported note envelope");
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
            item.id == server_item.id
                && item.chunk_count == server_item.chunk_count
                && item.chunk_count > 0
                && item.size <= item.chunk_count * note.chunk_bytes,
            "note manifest does not match transfer"
        );
        let key = Zeroizing::new(derive_item_key(&master_key, &note.id, &item.id)?);
        let prefix = decode(&item.nonce_prefix)?;
        let mut text = Vec::with_capacity(item.size as usize);
        let live_session = if note.driver == "webrtc" {
            let token = join_token.context("live note manifest has no join capability")?;
            let signal = Signaling::new(self.async_http.clone(), self.instance.as_str(), &note.id)?;
            Some(shared_tokio_runtime().block_on(async {
                let (peer, session, channel) = connect_receiver(&signal, &token, false).await?;
                Ok::<_, anyhow::Error>((peer, session, channel, signal))
            })?)
        } else {
            None
        };
        for index in 0..item.chunk_count {
            let ciphertext = if let Some((_, _session, channel, _)) = &live_session {
                let expected = if index + 1 == item.chunk_count {
                    item.size - index * note.chunk_bytes + TAG_BYTES as u64
                } else {
                    note.chunk_bytes + TAG_BYTES as u64
                };
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
                response.bytes()?.to_vec()
            };
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
        let consumed = if note.burn_on_read {
            let token = manifest
                .read_token
                .as_deref()
                .context("burn note has no read token")?;
            if let Some((_, session, _, _)) = &live_session {
                self.consume_live(&note.id, token, &session.token)? == BurnResult::Consumed
            } else {
                self.consume_once(&note.id, token)? == BurnResult::Consumed
            }
        } else {
            false
        };
        Ok(OpenedNote {
            id: note.id,
            text,
            title: manifest.title,
            language: manifest.language.unwrap_or_else(|| "plain".into()),
            consumed,
        })
    }

    pub fn consume(&self, transfer_id: &str, read_token: &str) -> Result<BurnResult> {
        self.consume_request(transfer_id, read_token)
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
}
