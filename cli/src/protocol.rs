use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_encryption::{
    Sha256Hasher, decrypt_chunk, decrypt_manifest, derive_item_key, derive_password_key,
    derive_password_protected_key, encrypt_chunk, encrypt_manifest, generate_nonce_prefix,
    generate_transfer_key,
};
use reqwest::{
    Url,
    blocking::{Body, Client, Response},
    header::CONTENT_TYPE,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

use crate::{
    app::{Control, Phase, SecretKind, TransferReader},
    uploads::{self, DirectoryMode},
};

const TAG_BYTES: u64 = 16;
#[derive(Deserialize)]
struct Api<T> {
    data: T,
}
#[derive(Clone, Deserialize)]
pub struct Info {
    pub name: String,
    chunk_bytes: u64,
    pub file_retention_hours: u64,
    pub anonymous_uploads_enabled: bool,
    #[serde(default)]
    pub enabled_drivers: Vec<String>,
    pub maximum_transfer_bytes: Option<u64>,
    pub maximum_file_count: Option<usize>,
}

#[cfg(test)]
impl Info {
    pub fn fixture() -> Self {
        Self {
            name: "Filebeam".into(),
            chunk_bytes: 24_999_984,
            file_retention_hours: 24,
            anonymous_uploads_enabled: true,
            enabled_drivers: vec!["http".into()],
            maximum_transfer_bytes: Some(2 * 1024 * 1024 * 1024),
            maximum_file_count: Some(20),
        }
    }
}

pub fn instance_info(instance: &str) -> Result<Info> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .user_agent(concat!("beam/", env!("BEAM_VERSION")))
        .build()?;
    get_json(&client, &format!("{instance}/api/v1/info"))
}
#[derive(Serialize)]
struct Create<'a> {
    kind: &'a str,
    driver: &'a str,
    protocol_version: u8,
    chunk_bytes: u64,
    retention_hours: u64,
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
    items: Vec<CreatedItem>,
    upload_token: String,
}
#[derive(Deserialize)]
struct CreatedItem {
    id: String,
    position: u64,
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
    control.phase(Phase::Connecting)?;
    let client = http_client()?;
    let info: Info = get_json(&client, &format!("{instance}/api/v1/info"))?;
    if !info.anonymous_uploads_enabled {
        bail!("this instance does not allow anonymous uploads");
    }
    if !info.enabled_drivers.iter().any(|driver| driver == "http") {
        bail!("this instance does not have HTTP uploads enabled");
    }
    if info.chunk_bytes == 0 || info.chunk_bytes > 24_999_984 {
        bail!("server supplied an invalid chunk size");
    }
    let mut files = Vec::new();
    let mut total = 0_u64;
    let prepared = uploads::prepare(paths, mode, info.maximum_file_count, control)?;
    for file in &prepared.files {
        control.check()?;
        let path = &file.path;
        let metadata = fs::metadata(path).with_context(|| format!("read {}", path.display()))?;
        if !metadata.is_file() {
            bail!("{} is not a regular file", path.display());
        }
        let chunks = metadata.len().div_ceil(info.chunk_bytes).max(1);
        total = total
            .checked_add(metadata.len())
            .context("selected files are too large")?;
        files.push((file, metadata.len(), chunks));
    }
    if info
        .maximum_file_count
        .is_some_and(|maximum| files.len() > maximum)
    {
        bail!(
            "this instance allows at most {} files",
            info.maximum_file_count.unwrap()
        );
    }
    let ciphertext_total = files.iter().try_fold(0_u64, |sum, (_, size, chunks)| {
        sum.checked_add(size.checked_add(chunks * TAG_BYTES)?)
    });
    let ciphertext_total = ciphertext_total.context("selected files are too large")?;
    if info
        .maximum_transfer_bytes
        .is_some_and(|maximum| ciphertext_total > maximum)
    {
        bail!("the encrypted transfer exceeds this instance's size limit");
    }
    control.totals(total, files.len());
    let created: Created = post_json(
        &client,
        &format!("{instance}/api/v1/transfers"),
        &Create {
            kind: "files",
            driver: "http",
            protocol_version: 1,
            chunk_bytes: info.chunk_bytes,
            retention_hours: info.file_retention_hours,
            items: files
                .iter()
                .map(|(_, size, chunks)| CreateItem {
                    ciphertext_bytes: size + chunks * TAG_BYTES,
                    chunk_count: *chunks,
                })
                .collect(),
        },
    )?;
    if created.chunk_bytes != info.chunk_bytes
        || created.items.len() != files.len()
        || created
            .items
            .iter()
            .enumerate()
            .any(|(position, item)| item.position != position as u64)
    {
        bail!("server transfer reservation is invalid");
    }
    let master = Zeroizing::new(generate_transfer_key().context("generate transfer key")?);
    let mut manifest_items = Vec::new();
    let mut done = 0;
    for (position, ((source, size, chunks), server)) in files.iter().zip(&created.items).enumerate()
    {
        let path = &source.path;
        control.item(source.name.clone(), position + 1, *size);
        let key = Zeroizing::new(derive_item_key(&master, &created.id, &server.id)?);
        let prefix = generate_nonce_prefix()?;
        let mut file = File::open(path)?;
        let mut hash = Sha256Hasher::new();
        let mut buffer = vec![0; info.chunk_bytes as usize];
        for index in 0..*chunks {
            control.phase(Phase::Encrypting)?;
            let remaining = size.saturating_sub(index * info.chunk_bytes);
            let read = remaining.min(info.chunk_bytes) as usize;
            file.read_exact(&mut buffer[..read]).with_context(|| {
                format!("{} changed while it was being uploaded", path.display())
            })?;
            hash.update(&buffer[..read]);
            let ciphertext = encrypt_chunk(
                &key,
                &prefix,
                index as u32,
                &buffer[..read],
                aad(&created.id, &server.id, &index.to_string()).as_bytes(),
            )?;
            control.phase(Phase::Sending)?;
            let length = ciphertext.len() as u64;
            let body = TransferReader::new(
                Cursor::new(ciphertext),
                control,
                read as u64,
                done,
                index * info.chunk_bytes,
            );
            let response = client
                .put(format!(
                    "{instance}/api/v1/transfers/{}/items/{}/chunks/{index}",
                    created.id, server.id
                ))
                .header(CONTENT_TYPE, "application/octet-stream")
                .header("X-Filebeam-Upload-Token", &created.upload_token)
                .body(Body::sized(body, length))
                .send()?;
            control.check()?;
            if response.status().as_u16() != 201 {
                bail!("chunk upload failed: {}", response.status());
            }
            done += read as u64;
            control.commit(done);
        }
        if file.read(&mut [0_u8; 1])? != 0 {
            bail!("{} changed while it was being uploaded", path.display());
        }
        manifest_items.push(ManifestItem {
            id: server.id.clone(),
            name: source.name.clone(),
            mime: "application/octet-stream".into(),
            size: *size,
            nonce_prefix: encode(&prefix),
            chunk_count: *chunks,
            digest: DigestValue {
                algorithm: "sha256".into(),
                value: hex::encode(hash.finalize()),
            },
        });
    }
    control.phase(Phase::Finalizing)?;
    let manifest = serde_json::to_vec(&Manifest {
        version: 1,
        items: manifest_items,
    })?;
    let prefix = generate_nonce_prefix()?;
    let ciphertext = encrypt_manifest(
        &master,
        &prefix,
        &manifest,
        aad(&created.id, "manifest", "manifest").as_bytes(),
    )?;
    let envelope = serde_json::to_string(&Envelope {
        v: 1,
        nonce_prefix: encode(&prefix),
        salt: None,
        ciphertext: encode(&ciphertext),
    })?;
    let response = client
        .post(format!(
            "{instance}/api/v1/transfers/{}/complete",
            created.id
        ))
        .header("X-Filebeam-Upload-Token", &created.upload_token)
        .json(&serde_json::json!({"encrypted_manifest": envelope}))
        .send()?;
    if !response.status().is_success() {
        bail!("could not complete transfer: {}", response.status());
    }
    let base = Url::parse(instance)?.join(
        created
            .share_url
            .split('#')
            .next()
            .unwrap_or(&created.share_url),
    )?;
    Ok(format!(
        "{}#k=v1.{}",
        base.as_str().trim_end_matches('/'),
        encode(&master)
    ))
}

pub fn download(
    instance: &str,
    raw: &str,
    output: &Path,
    control: &Control,
) -> Result<Vec<PathBuf>> {
    let parsed = parse_link_for_instance(raw, instance)?;
    let instance = parsed.instance.as_str();
    control.phase(Phase::Connecting)?;
    let client = http_client()?;
    let transfer = retry_metadata(&client, instance, &parsed.id, control)?;
    if transfer.protocol_version != 1
        || transfer.id != parsed.id
        || transfer.chunk_bytes == 0
        || transfer.chunk_bytes > 24_999_984
    {
        bail!("unsupported or invalid transfer metadata");
    }
    let envelope: Envelope = serde_json::from_str(
        &transfer
            .encrypted_manifest
            .clone()
            .context("transfer has no encrypted manifest")?,
    )
    .context("invalid encrypted manifest envelope")?;
    if envelope.v != 1 {
        bail!("unsupported encrypted manifest version");
    }
    let mut key = Zeroizing::new(match parsed.key {
        Some(key) => key,
        None => decode_share_key(&control.secret(SecretKind::ShareKey)?)?,
    });
    if let Some(salt) = envelope.salt.as_deref() {
        let password = control.secret(SecretKind::Password)?;
        let password_key = Zeroizing::new(derive_password_key(
            password.as_bytes(),
            &decode(salt)?,
            65_536,
            3,
            1,
        )?);
        key = Zeroizing::new(derive_password_protected_key(&key, &password_key)?);
    }
    let plain = decrypt_manifest(
        &key,
        &decode(&envelope.nonce_prefix)?,
        &decode(&envelope.ciphertext)?,
        aad(&parsed.id, "manifest", "manifest").as_bytes(),
    )
    .context("could not decrypt manifest")?;
    let manifest: Manifest =
        serde_json::from_slice(&plain).context("invalid decrypted manifest")?;
    validate_manifest(&manifest, &transfer)?;
    control.check()?;
    fs::create_dir_all(output)?;
    let total = manifest
        .items
        .iter()
        .try_fold(0_u64, |sum, item| sum.checked_add(item.size))
        .context("transfer is too large")?;
    control.totals(total, manifest.items.len());
    let mut done = 0;
    let mut saved = Vec::new();
    for (position, item) in manifest.items.into_iter().enumerate() {
        control.item(item.name.clone(), position + 1, item.size);
        let target = collision_free(output, &safe_filename(&item.name));
        let mut temporary = NamedTempFile::new_in(output).context("create temporary download")?;
        let item_key = Zeroizing::new(derive_item_key(&key, &parsed.id, &item.id)?);
        let prefix = decode(&item.nonce_prefix)?;
        let mut hasher = Sha256::new();
        let mut item_done = 0;
        for index in 0..item.chunk_count {
            control.phase(Phase::Receiving)?;
            let size = item
                .size
                .saturating_sub(item_done)
                .min(transfer.chunk_bytes);
            let ciphertext = get_chunk(
                &client,
                &format!(
                    "{instance}/api/v1/transfers/{}/items/{}/chunks/{index}",
                    parsed.id, item.id
                ),
                size,
                (done, item_done),
                control,
            )?;
            control.phase(Phase::Verifying)?;
            let plaintext = decrypt_chunk(
                &item_key,
                &prefix,
                index as u32,
                &ciphertext,
                aad(&parsed.id, &item.id, &index.to_string()).as_bytes(),
            )
            .context("could not decrypt chunk")?;
            item_done += plaintext.len() as u64;
            done += plaintext.len() as u64;
            hasher.update(&plaintext);
            temporary
                .write_all(&plaintext)
                .context("write decrypted download")?;
            control.commit(done);
        }
        if item_done != item.size {
            bail!("download size mismatch for {}", item.name);
        }
        if !valid_digest(&item.digest.value) || hex::encode(hasher.finalize()) != item.digest.value
        {
            bail!("SHA-256 verification failed for {}", item.name);
        }
        temporary.as_file_mut().sync_all()?;
        control.check()?;
        temporary
            .persist_noclobber(&target)
            .map_err(|error| error.error)?;
        saved.push(target);
    }
    Ok(saved)
}

fn get_chunk(
    client: &Client,
    url: &str,
    size: u64,
    bases: (u64, u64),
    control: &Control,
) -> Result<Vec<u8>> {
    for _ in 0..5 {
        control.check()?;
        let response = client.get(url).send()?;
        if response.status().as_u16() == 202 {
            control.phase(Phase::Waiting)?;
            wait_response(&response, control)?;
            continue;
        }
        if !response.status().is_success() {
            bail!("chunk download failed: {} for {url}", response.status());
        }
        control.phase(Phase::Receiving)?;
        let mut ciphertext = Vec::with_capacity((size + TAG_BYTES) as usize);
        TransferReader::new(response, control, size, bases.0, bases.1)
            .take(size + TAG_BYTES + 1)
            .read_to_end(&mut ciphertext)?;
        if ciphertext.len() as u64 != size + TAG_BYTES {
            bail!("encrypted chunk size mismatch");
        }
        return Ok(ciphertext);
    }
    bail!("chunk is still pending; try again shortly")
}
fn wait_response(response: &Response, control: &Control) -> Result<()> {
    let seconds = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1)
        .min(5);
    for _ in 0..seconds * 10 {
        control.check()?;
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
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
    // A full share URL is authoritative. The configured instance is only needed for IDs
    // (and the legacy scheme-less link form), never to redirect an explicit URL.
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
    value.len() == 26 && matches!(value.as_bytes()[0], b'0'..=b'7') && value.bytes().skip(1).all(|c| matches!(c, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
}
fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .enumerate()
            .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) && c == b'-' || c.is_ascii_hexdigit())
}
pub fn safe_filename(value: &str) -> String {
    let name: String = value
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        "download".into()
    } else {
        name
    }
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
    if manifest.version != 1 || manifest.items.len() != transfer.items.len() {
        bail!("manifest does not match transfer");
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut positions = HashSet::new();
    for item in &manifest.items {
        let server = transfer
            .items
            .iter()
            .find(|server| server.id == item.id)
            .context("unknown item")?;
        if !ids.insert(&item.id)
            || !names.insert(&item.name)
            || !positions.insert(server.position)
            || !valid_digest(&item.digest.value)
        {
            bail!("manifest has duplicate or invalid item metadata");
        }
        let count = item.size.div_ceil(transfer.chunk_bytes).max(1);
        if item.chunk_count != count
            || server.chunk_count != count
            || item.digest.algorithm != "sha256"
            || decode(&item.nonce_prefix)?.len() != 16
        {
            bail!("manifest item is invalid");
        }
    }
    Ok(())
}
fn retry_metadata(
    client: &Client,
    instance: &str,
    id: &str,
    control: &Control,
) -> Result<Transfer> {
    for _ in 0..5 {
        control.check()?;
        let response = client
            .get(format!("{instance}/api/v1/transfers/{id}"))
            .send()?;
        if response.status().as_u16() == 202 {
            control.phase(Phase::Waiting)?;
            wait_response(&response, control)?;
            continue;
        }
        return decode_json(response);
    }
    bail!("transfer is still pending; try again shortly")
}
fn get_json<T: for<'de> Deserialize<'de>>(client: &Client, url: &str) -> Result<T> {
    decode_json(client.get(url).send()?)
}
fn post_json<T: for<'de> Deserialize<'de>, B: Serialize>(
    client: &Client,
    url: &str,
    body: &B,
) -> Result<T> {
    decode_json(client.post(url).json(body).send()?)
}
fn decode_json<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T> {
    if !response.status().is_success() {
        bail!(
            "server returned {} for {}",
            response.status(),
            response.url()
        );
    }
    Ok(response.json::<Api<T>>()?.data)
}
fn http_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("beam/", env!("BEAM_VERSION")))
        .build()
        .context("create HTTP client")
}
fn aad(transfer: &str, item: &str, index: &str) -> String {
    format!("filebeam:v1:{transfer}:{item}:{index}")
}
fn encode(value: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(value)
}
fn decode(value: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(value)
        .context("invalid base64url data")
}
fn decode_share_key(value: &str) -> Result<Vec<u8>> {
    let key = value.trim().strip_prefix("v1.").unwrap_or(value.trim());
    let decoded = decode(key)?;
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
        assert_eq!(
            parse_link_for_instance(
                &format!("localhost:8000/01arz3ndektsv4rrffq69g5fav#k=v1.{}", key()),
                "http://localhost:8000"
            )
            .unwrap()
            .id,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV"
        );
        assert_eq!(
            parse_link_for_instance(
                &format!(
                    "http://127.0.0.1:18080/01arz3ndektsv4rrffq69g5fav#k=v1.{}",
                    key()
                ),
                "http://127.0.0.1:18080"
            )
            .unwrap()
            .id,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV"
        );
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
        assert!(
            parse_link("01ARZ3NDEKTSV4RRFFQ69G5FAV")
                .unwrap()
                .key
                .is_none()
        );
    }
    #[test]
    fn full_links_select_their_origin_and_ids_use_the_configured_instance() {
        let raw = "http://localhost:8000/01M23GEFKC2WNXBASCS675XJRD#k=v1.KGGOo1frIIN3t1kAgQ2STIKjGXOFPKiPBATHBKDpzxY";
        let local = parse_link_for_instance(raw, "https://filebeam.io").unwrap();
        assert_eq!(local.instance, "http://localhost:8000");
        assert_eq!(local.id, "01M23GEFKC2WNXBASCS675XJRD");
        assert_eq!(local.key.unwrap().len(), 32);
        assert_eq!(
            parse_link_for_instance(raw, "invalid configuration")
                .unwrap()
                .instance,
            "http://localhost:8000"
        );
        assert_eq!(
            parse_link_for_instance(
                "https://other.test:8443/01M23GEFKC2WNXBASCS675XJRD",
                "http://localhost:8000"
            )
            .unwrap()
            .instance,
            "https://other.test:8443"
        );
        assert_eq!(
            parse_link_for_instance("01M23GEFKC2WNXBASCS675XJRD", "http://localhost:8000/")
                .unwrap()
                .instance,
            "http://localhost:8000"
        );
        assert_eq!(
            parse_link("01M23GEFKC2WNXBASCS675XJRD").unwrap().instance,
            "https://filebeam.io"
        );
    }
    #[test]
    fn filenames_cannot_traverse() {
        assert_eq!(safe_filename("../../x"), ".._.._x");
        assert_eq!(safe_filename(".."), "download");
    }
    #[test]
    fn manifest_crypto_round_trip() {
        let key = vec![4; 32];
        let prefix = vec![5; 16];
        let aad = aad("01ARZ3NDEKTSV4RRFFQ69G5FAV", "manifest", "manifest");
        let c = encrypt_manifest(&key, &prefix, b"{}", aad.as_bytes()).unwrap();
        assert_eq!(
            decrypt_manifest(&key, &prefix, &c, aad.as_bytes()).unwrap(),
            b"{}"
        );
    }
}
