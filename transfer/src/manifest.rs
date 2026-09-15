use crate::{AEAD_TAG_BYTES, MAX_CHUNKS, MAX_CIPHERTEXT_BYTES};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub join_token: Option<String>,
    pub version: u8,
    pub items: Vec<ManifestItem>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ManifestItem {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
    pub size: u64,
    pub nonce_prefix: String,
    pub chunk_count: u64,
    pub digest: DigestValue,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DigestValue {
    pub algorithm: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ManifestServerItem {
    pub id: String,
    pub position: u64,
    pub chunk_count: u64,
}

pub fn validate_manifest(
    manifest: &Manifest,
    driver: &str,
    chunk_bytes: u64,
    server_items: &[ManifestServerItem],
) -> Result<(), String> {
    if driver == "webrtc" && !manifest.join_token.as_ref().is_some_and(valid_capability) {
        return Err("live manifest has an invalid join token".into());
    }
    if manifest.version != 1
        || manifest.items.is_empty()
        || manifest.items.len() != server_items.len()
    {
        return Err("manifest does not match transfer".into());
    }
    if chunk_bytes == 0 || chunk_bytes > MAX_CIPHERTEXT_BYTES - AEAD_TAG_BYTES {
        return Err("transfer has an invalid chunk size".into());
    }
    let mut server_ids = HashSet::new();
    let mut positions = HashSet::new();
    for server in server_items {
        if server.id.is_empty()
            || !server_ids.insert(&server.id)
            || !positions.insert(server.position)
            || server.position as usize >= server_items.len()
            || server.chunk_count == 0
            || server.chunk_count > MAX_CHUNKS
        {
            return Err("transfer has invalid item metadata".into());
        }
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for item in &manifest.items {
        let Some(server) = server_items.iter().find(|server| server.id == item.id) else {
            return Err("unknown item".into());
        };
        let count = item
            .size
            .checked_add(chunk_bytes.saturating_sub(1))
            .ok_or("manifest chunk count overflow")?
            / chunk_bytes;
        if item.id.is_empty()
            || item.name.is_empty()
            || !ids.insert(&item.id)
            || !names.insert(portable_filename(&item.name))
            || item.digest.algorithm != "sha256"
            || !valid_digest(&item.digest.value)
            || item.chunk_count == 0
            || item.chunk_count > MAX_CHUNKS
            || item.chunk_count != count.max(1)
            || server.chunk_count != item.chunk_count
            || !valid_nonce_prefix(&item.nonce_prefix)
        {
            return Err("manifest item is invalid".into());
        }
    }
    Ok(())
}

fn valid_capability(value: &String) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_nonce_prefix(value: &str) -> bool {
    value.len() == 22
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        && base64url_value(*value.as_bytes().last().unwrap()).is_some_and(|last| last & 0b1111 == 0)
}

fn portable_filename(value: &str) -> String {
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

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}
