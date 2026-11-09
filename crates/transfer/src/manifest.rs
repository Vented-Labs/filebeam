use crate::{AEAD_TAG_BYTES, MAX_CHUNKS, MAX_CIPHERTEXT_BYTES};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub attached_note: Option<AttachedNote>,
    #[serde(default)]
    pub join_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<ManifestTree>,
    pub version: u8,
    pub items: Vec<ManifestItem>,
}

pub const MAX_ATTACHED_NOTE_BYTES: usize = 64 * 1024;
pub const MAX_TREE_ENTRIES: usize = 100_000;
pub const MAX_TREE_DEPTH: usize = 64;
pub const MAX_TREE_PATH_BYTES: usize = 4_096;
pub const MAX_TREE_COMPONENT_BYTES: usize = 255;

/// Private tree metadata retained inside the existing encrypted manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestTree {
    pub roots: Vec<String>,
    #[serde(default)]
    pub entries: Vec<ManifestTreeEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestTreeEntry {
    pub path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachedNote {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub language: String,
}

impl std::fmt::Debug for AttachedNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AttachedNote([redacted])")
    }
}

impl AttachedNote {
    pub fn validate(&self) -> Result<(), String> {
        if self.text.is_empty() || self.text.len() > MAX_ATTACHED_NOTE_BYTES {
            return Err("attached note text must contain 1 byte to 64 KiB of UTF-8".into());
        }
        if self
            .title
            .as_ref()
            .is_some_and(|title| title.chars().count() > 160)
        {
            return Err("attached note title must contain at most 160 characters".into());
        }
        if !matches!(
            self.language.as_str(),
            "plain"
                | "php"
                | "dotenv"
                | "javascript"
                | "typescript"
                | "json"
                | "markdown"
                | "css"
                | "html"
        ) {
            return Err("unsupported attached note language".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DigestValue {
    pub algorithm: String,
    pub value: String,
}

/// Authenticated item metadata supplied by the metadata source. Public
/// transfers obtain this from the instance; device transfers obtain it from a
/// verified peer. It intentionally contains no public capability.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManifestMetadataItem {
    pub id: String,
    pub position: u64,
    pub chunk_count: u64,
}

/// Compatibility name for existing public-transfer callers.
pub type ManifestServerItem = ManifestMetadataItem;

/// The admission decision is distinct from authenticated metadata binding.
/// A verified device session may not be represented with a fabricated public
/// join capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestAdmission<'a> {
    Public { driver: &'a str },
    VerifiedDevice { tree_capability: bool },
}

pub fn validate_manifest(
    manifest: &Manifest,
    driver: &str,
    chunk_bytes: u64,
    server_items: &[ManifestMetadataItem],
) -> Result<(), String> {
    validate_manifest_content(manifest, chunk_bytes)?;
    validate_manifest_binding(manifest, server_items)?;
    validate_manifest_admission(manifest, ManifestAdmission::Public { driver })
}

/// Validate fields that are intrinsic to the decrypted manifest. This is used
/// by every metadata source before any output or chunk handling begins.
pub fn validate_manifest_content(manifest: &Manifest, chunk_bytes: u64) -> Result<(), String> {
    if let Some(note) = &manifest.attached_note {
        note.validate()?;
    }
    if manifest.version != 1 || (manifest.items.is_empty() && manifest.tree.is_none()) {
        return Err("manifest does not match transfer".into());
    }
    if chunk_bytes == 0 || chunk_bytes > MAX_CIPHERTEXT_BYTES - AEAD_TAG_BYTES {
        return Err("transfer has an invalid chunk size".into());
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for item in &manifest.items {
        let count = item
            .size
            .checked_add(chunk_bytes.saturating_sub(1))
            .ok_or("manifest chunk count overflow")?
            / chunk_bytes;
        if item.id.is_empty()
            || item.name.is_empty()
            || !ids.insert(&item.id)
            || (manifest.tree.is_none() && !names.insert(portable_filename(&item.name)))
            || item.digest.algorithm != "sha256"
            || !valid_digest(&item.digest.value)
            || item.chunk_count == 0
            || item.chunk_count > MAX_CHUNKS
            || item.chunk_count != count.max(1)
            || !valid_nonce_prefix(&item.nonce_prefix)
        {
            return Err("manifest item is invalid".into());
        }
    }
    if let Some(tree) = &manifest.tree {
        validate_tree(tree, &ids)?;
    }
    Ok(())
}

/// Bind a valid manifest to its authenticated descriptor. A lookup is built
/// once so peer-supplied large manifests do not repeatedly scan metadata.
pub fn validate_manifest_binding(
    manifest: &Manifest,
    metadata_items: &[ManifestMetadataItem],
) -> Result<(), String> {
    if manifest.items.len() != metadata_items.len() {
        return Err("manifest does not match transfer".into());
    }
    let mut items = HashMap::with_capacity(metadata_items.len());
    let mut positions = HashSet::with_capacity(metadata_items.len());
    for item in metadata_items {
        if item.id.is_empty()
            || items.insert(item.id.as_str(), item).is_some()
            || !positions.insert(item.position)
            || usize::try_from(item.position).ok() >= Some(metadata_items.len())
            || item.chunk_count == 0
            || item.chunk_count > MAX_CHUNKS
        {
            return Err("transfer has invalid item metadata".into());
        }
    }
    for item in &manifest.items {
        let Some(metadata) = items.get(item.id.as_str()) else {
            return Err("unknown item".into());
        };
        if metadata.chunk_count != item.chunk_count {
            return Err("manifest item is invalid".into());
        }
    }
    Ok(())
}

/// Apply the authority-specific admission check after content and metadata
/// binding. This prevents a peer path from weakening public-link admission.
pub fn validate_manifest_admission(
    manifest: &Manifest,
    admission: ManifestAdmission<'_>,
) -> Result<(), String> {
    match admission {
        ManifestAdmission::Public { driver }
            if driver == "webrtc"
                && !manifest.join_token.as_deref().is_some_and(valid_capability) =>
        {
            Err("live manifest has an invalid join token".into())
        }
        ManifestAdmission::VerifiedDevice { .. } if manifest.join_token.is_some() => {
            Err("device manifest must not contain a public join token".into())
        }
        ManifestAdmission::VerifiedDevice {
            tree_capability: false,
        } if manifest.tree.is_some() => Err("device does not support manifest trees".into()),
        ManifestAdmission::Public { .. } if manifest.tree.is_some() => {
            Err("public manifests must not contain a tree".into())
        }
        _ => Ok(()),
    }
}

/// Validate metadata received only after a device-authentication admission
/// path has produced a verified authority.
pub fn validate_verified_device_manifest(
    manifest: &Manifest,
    chunk_bytes: u64,
    metadata_items: &[ManifestMetadataItem],
) -> Result<(), String> {
    validate_verified_device_manifest_with_tree_capability(
        manifest,
        chunk_bytes,
        metadata_items,
        false,
    )
}

/// Enable trees only after both verified devices negotiate the tree capability.
pub fn validate_verified_device_manifest_with_tree_capability(
    manifest: &Manifest,
    chunk_bytes: u64,
    metadata_items: &[ManifestMetadataItem],
    tree_capability: bool,
) -> Result<(), String> {
    validate_manifest_content(manifest, chunk_bytes)?;
    validate_manifest_binding(manifest, metadata_items)?;
    validate_manifest_admission(
        manifest,
        ManifestAdmission::VerifiedDevice { tree_capability },
    )
}

fn validate_tree(tree: &ManifestTree, item_ids: &HashSet<&String>) -> Result<(), String> {
    if tree.roots.is_empty() || tree.roots.len() + tree.entries.len() > MAX_TREE_ENTRIES {
        return Err("manifest tree has invalid entries".into());
    }
    let mut roots = HashSet::with_capacity(tree.roots.len());
    for root in &tree.roots {
        validate_tree_component(root)?;
        if !roots.insert(root.as_str()) {
            return Err("manifest tree has duplicate roots".into());
        }
    }
    let mut paths = HashMap::with_capacity(tree.entries.len());
    let mut referenced = HashSet::with_capacity(item_ids.len());
    for entry in &tree.entries {
        validate_tree_path(&entry.path, &roots)?;
        let path = entry.path.join("/");
        if paths.insert(path, entry.item_id.is_some()).is_some() {
            return Err("manifest tree has duplicate paths".into());
        }
        if let Some(item_id) = &entry.item_id
            && (!item_ids.contains(item_id) || !referenced.insert(item_id.as_str()))
        {
            return Err("manifest tree has an invalid item reference".into());
        }
    }
    if referenced.len() != item_ids.len() {
        return Err("manifest tree does not represent every item".into());
    }
    for path in paths.keys() {
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if paths.get(parent) == Some(&true) {
                return Err("manifest tree has a file/directory ancestor conflict".into());
            }
            ancestor = parent;
        }
    }
    Ok(())
}

fn validate_tree_path(path: &[String], roots: &HashSet<&str>) -> Result<(), String> {
    if path.is_empty() || path.len() > MAX_TREE_DEPTH || !roots.contains(path[0].as_str()) {
        return Err("manifest tree has an invalid path".into());
    }
    let mut bytes = 0usize;
    for component in path {
        validate_tree_component(component)?;
        bytes = bytes
            .checked_add(component.len() + 1)
            .ok_or("manifest tree path is too long")?;
    }
    if bytes.saturating_sub(1) > MAX_TREE_PATH_BYTES {
        return Err("manifest tree path is too long".into());
    }
    Ok(())
}

fn validate_tree_component(component: &str) -> Result<(), String> {
    let lower = component.to_ascii_lowercase();
    if component.is_empty()
        || component.len() > MAX_TREE_COMPONENT_BYTES
        || matches!(component, "." | "..")
        || component.contains(['/', '\\', ':', '\0'])
        || component.chars().any(char::is_control)
        || lower.contains("%2e")
        || lower.contains("%2f")
        || lower.contains("%5c")
    {
        return Err("manifest tree has an invalid path component".into());
    }
    Ok(())
}

fn valid_capability(value: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_limits_count_utf8_bytes_and_unicode_title_characters() {
        let mut note = AttachedNote {
            text: "🦀".repeat(16_384),
            title: Some("🦀".repeat(160)),
            language: "markdown".into(),
        };
        note.validate().unwrap();
        note.text.push('x');
        assert!(note.validate().is_err());
        note.text = " \n\t".into();
        note.title.as_mut().unwrap().push('x');
        assert!(note.validate().is_err());
        note.title = None;
        note.validate().unwrap();
        assert!(!format!("{note:?}").contains(&note.text));
        note.language = "executable".into();
        assert!(note.validate().is_err());
    }

    #[test]
    fn old_manifests_remain_readable_and_malformed_attachments_fail() {
        let mut value = serde_json::json!({"version":1,"items":[]});
        assert!(
            serde_json::from_value::<Manifest>(value.clone())
                .unwrap()
                .attached_note
                .is_none()
        );
        value["attached_note"] = serde_json::json!({"text":42,"language":"plain"});
        assert!(serde_json::from_value::<Manifest>(value).is_err());
    }

    #[test]
    fn verified_device_admission_reuses_content_and_binding_without_public_token() {
        let manifest = Manifest {
            attached_note: Some(AttachedNote {
                text: "note".into(),
                title: None,
                language: "plain".into(),
            }),
            join_token: None,
            tree: None,
            version: 1,
            items: vec![ManifestItem {
                id: "item".into(),
                name: "report.pdf".into(),
                mime: "application/pdf".into(),
                size: 3,
                nonce_prefix: "AAAAAAAAAAAAAAAAAAAAAA".into(),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: "0".repeat(64),
                },
            }],
        };
        let metadata = [ManifestMetadataItem {
            id: "item".into(),
            position: 0,
            chunk_count: 1,
        }];
        assert!(validate_verified_device_manifest(&manifest, 16, &metadata).is_ok());
        assert!(validate_manifest(&manifest, "webrtc", 16, &metadata).is_err());

        let mut public_capability = manifest.clone();
        public_capability.join_token = Some("a".repeat(64));
        assert!(validate_manifest(&public_capability, "webrtc", 16, &metadata).is_ok());
        assert!(validate_verified_device_manifest(&public_capability, 16, &metadata).is_err());
    }

    #[test]
    fn binding_rejects_duplicate_positions_without_linear_lookup() {
        let manifest = Manifest {
            attached_note: None,
            join_token: None,
            tree: None,
            version: 1,
            items: Vec::new(),
        };
        let metadata = [
            ManifestMetadataItem {
                id: "one".into(),
                position: 0,
                chunk_count: 1,
            },
            ManifestMetadataItem {
                id: "two".into(),
                position: 0,
                chunk_count: 1,
            },
        ];
        assert!(validate_manifest_binding(&manifest, &metadata).is_err());
    }

    #[test]
    fn tree_is_private_capability_gated_and_preserves_empty_roots() {
        let manifest: Manifest =
            serde_json::from_str(include_str!("../tests/fixtures/tree.json")).unwrap();
        let metadata = [
            ManifestMetadataItem {
                id: "one".into(),
                position: 0,
                chunk_count: 1,
            },
            ManifestMetadataItem {
                id: "two".into(),
                position: 1,
                chunk_count: 1,
            },
        ];
        assert!(validate_manifest(&manifest, "http", 16, &metadata).is_err());
        assert!(validate_verified_device_manifest(&manifest, 16, &metadata).is_err());
        assert!(
            validate_verified_device_manifest_with_tree_capability(&manifest, 16, &metadata, true)
                .is_ok()
        );

        let empty = Manifest {
            attached_note: None,
            join_token: None,
            tree: Some(ManifestTree {
                roots: vec!["empty".into()],
                entries: vec![],
            }),
            version: 1,
            items: vec![],
        };
        assert!(
            validate_verified_device_manifest_with_tree_capability(&empty, 16, &[], true).is_ok()
        );
        assert!(validate_manifest(&empty, "http", 16, &[]).is_err());
    }

    #[test]
    fn tree_rejects_unsafe_paths_collisions_and_missing_items() {
        let base = Manifest {
            attached_note: None,
            join_token: None,
            tree: Some(ManifestTree {
                roots: vec!["root".into()],
                entries: vec![ManifestTreeEntry {
                    path: vec!["root".into(), "file".into()],
                    item_id: Some("one".into()),
                }],
            }),
            version: 1,
            items: vec![ManifestItem {
                id: "one".into(),
                name: "file".into(),
                mime: "application/octet-stream".into(),
                size: 0,
                nonce_prefix: "AAAAAAAAAAAAAAAAAAAAAA".into(),
                chunk_count: 1,
                digest: DigestValue {
                    algorithm: "sha256".into(),
                    value: "0".repeat(64),
                },
            }],
        };
        assert!(validate_manifest_content(&base, 16).is_ok());
        let mut traversal = base.clone();
        traversal.tree.as_mut().unwrap().entries[0].path[1] = "%2e%2e".into();
        assert!(validate_manifest_content(&traversal, 16).is_err());
        let mut collision = base.clone();
        collision
            .tree
            .as_mut()
            .unwrap()
            .entries
            .push(ManifestTreeEntry {
                path: vec!["root".into(), "file".into(), "child".into()],
                item_id: None,
            });
        assert!(validate_manifest_content(&collision, 16).is_err());
        let mut missing = base;
        missing.tree.as_mut().unwrap().entries.clear();
        assert!(validate_manifest_content(&missing, 16).is_err());
    }
}
