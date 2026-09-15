use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use filebeam_encryption::{
    decrypt_manifest, derive_account_wrapping_key, derive_password_key, encrypt_manifest,
    generate_account_keypair, generate_nonce_prefix, open_recipient_envelope,
    seal_key_for_recipient,
};
use filebeam_transfer::manifest::{Manifest, ManifestServerItem, validate_manifest};
use reqwest::{Url, blocking::Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::client::url;

#[derive(Clone)]
pub struct AccountService {
    instance: Url,
    http: Client,
}
#[derive(Clone, Debug, Deserialize)]
pub struct AccountSession {
    pub id: u64,
    pub name: String,
    pub username: Option<String>,
    pub email: String,
    #[serde(rename = "inboxEnabled")]
    pub inbox_enabled: bool,
    #[serde(rename = "usernameRoutingEnabled")]
    pub username_routing_enabled: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct AccountKeyBundle {
    pub id: u64,
    pub user_id: u64,
    pub version: u32,
    pub public_key: String,
    pub fingerprint: String,
    pub custody_mode: String,
    pub encrypted_private_key: Option<String>,
    pub is_active: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct InboxTransfer {
    pub id: String,
    pub ciphertext_bytes: u64,
    pub item_count: u64,
    pub completed_at: String,
    pub expires_at: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct InboxMetadata {
    pub id: String,
    pub encrypted_manifest: Option<String>,
    pub recipient_key: RecipientKey,
    pub driver: String,
    pub protocol_version: u8,
    pub chunk_bytes: u64,
    pub items: Vec<InboxServerItem>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct InboxServerItem {
    pub id: String,
    pub position: u64,
    pub chunk_count: u64,
}
#[derive(Clone, Debug)]
pub struct OpenedInbox {
    pub transfer_id: String,
    pub key_bundle_id: u64,
    pub filenames: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct RecipientKey {
    pub bundle: AccountKeyBundle,
    pub encrypted_key: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct AccountKeyUpload {
    pub public_key: String,
    pub fingerprint: String,
    pub custody_mode: String,
    pub encrypted_private_key: Option<String>,
    pub current_password: Option<String>,
    pub replace: bool,
}
#[derive(Clone, Debug)]
pub struct AccountKeyMaterial {
    pub private_key: Zeroizing<Vec<u8>>,
    pub public_key: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Recipient {
    pub id: u64,
    pub username: String,
    pub public_key: String,
    pub account_key_bundle_id: u64,
    pub version: u32,
    pub fingerprint: String,
}
#[derive(Deserialize)]
struct Api<T> {
    data: T,
}
#[derive(Serialize)]
struct Login<'a> {
    email: &'a str,
    password: &'a str,
    remember: bool,
}

#[derive(Serialize)]
struct Register<'a> {
    username: &'a str,
    name: Option<&'a str>,
    email: &'a str,
    password: &'a str,
    password_confirmation: &'a str,
}
#[derive(Deserialize)]
struct ManifestEnvelope {
    nonce_prefix: String,
    ciphertext: String,
}

impl AccountService {
    pub(crate) fn new(instance: Url, http: Client) -> Self {
        Self { instance, http }
    }
    pub fn login(&self, email: &str, password: &str, remember: bool) -> Result<AccountSession> {
        let response = self
            .http
            .post(url(&self.instance, "api/native/v1/session")?)
            .header("Sec-Fetch-Site", "same-origin")
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&Login {
                email,
                password,
                remember,
            })
            .send()
            .context("native login")?;
        if !response.status().is_success() {
            bail!("native login returned {}", response.status());
        }
        response
            .json::<Api<AccountSession>>()
            .context("decode native session")
            .map(|v| v.data)
    }
    pub fn register(
        &self,
        username: &str,
        name: Option<&str>,
        email: &str,
        password: &str,
    ) -> Result<AccountSession> {
        let response = self
            .http
            .post(url(&self.instance, "api/native/v1/register")?)
            .header("Sec-Fetch-Site", "same-origin")
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&Register {
                username,
                name,
                email,
                password,
                password_confirmation: password,
            })
            .send()
            .context("native registration")?;
        if response.status().as_u16() != 201 {
            bail!("native registration returned {}", response.status());
        }
        response
            .json::<Api<AccountSession>>()
            .context("decode native registration")
            .map(|v| v.data)
    }
    pub fn logout(&self) -> Result<()> {
        let response = self
            .http
            .delete(url(&self.instance, "api/native/v1/session")?)
            .header("Sec-Fetch-Site", "same-origin")
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .context("native logout")?;
        if response.status().as_u16() != 204 {
            bail!("native logout returned {}", response.status());
        }
        Ok(())
    }
    pub fn session(&self) -> Result<AccountSession> {
        self.get("api/native/v1/session")
    }
    pub fn inbox(&self) -> Result<Vec<InboxTransfer>> {
        self.get("api/native/v1/inbox")
    }
    pub fn inbox_metadata(&self, id: &str) -> Result<InboxMetadata> {
        self.get(&format!("api/native/v1/inbox/{id}/metadata"))
    }
    /// Opens an inbox transfer entirely in Rust. The key is returned only to
    /// the caller for an authenticated, request-scoped inbox download.
    pub fn open_inbox(&self, id: &str, private_key: &[u8]) -> Result<OpenedInbox> {
        let metadata = self.inbox_metadata(id)?;
        if metadata.id != id || metadata.protocol_version != 1 || metadata.driver != "http" {
            bail!("inbox transfer metadata is not a supported HTTP v1 transfer");
        }
        validate_self_key(private_key, &metadata.recipient_key.bundle.public_key)?;
        let key = open_recipient_key(private_key, &metadata.recipient_key, &metadata.id)?;
        let envelope: ManifestEnvelope = serde_json::from_str(
            metadata
                .encrypted_manifest
                .as_deref()
                .context("inbox transfer has no encrypted manifest")?,
        )
        .context("invalid encrypted inbox manifest")?;
        let prefix = URL_SAFE_NO_PAD
            .decode(envelope.nonce_prefix)
            .context("inbox manifest nonce is not base64url")?;
        let ciphertext = URL_SAFE_NO_PAD
            .decode(envelope.ciphertext)
            .context("inbox manifest ciphertext is not base64url")?;
        let bytes = decrypt_manifest(
            &key,
            &prefix,
            &ciphertext,
            format!("filebeam:v1:{}:manifest:manifest", metadata.id).as_bytes(),
        )
        .context("could not decrypt inbox manifest")?;
        let manifest: Manifest =
            serde_json::from_slice(&bytes).context("invalid decrypted inbox manifest")?;
        let items = metadata
            .items
            .iter()
            .map(|item| ManifestServerItem {
                id: item.id.clone(),
                position: item.position,
                chunk_count: item.chunk_count,
            })
            .collect::<Vec<_>>();
        validate_manifest(&manifest, &metadata.driver, metadata.chunk_bytes, &items)
            .map_err(anyhow::Error::msg)?;
        Ok(OpenedInbox {
            transfer_id: metadata.id.clone(),
            key_bundle_id: metadata.recipient_key.bundle.id,
            filenames: manifest.items.into_iter().map(|item| item.name).collect(),
        })
    }
    pub fn account_keys(&self) -> Result<Vec<AccountKeyBundle>> {
        self.get("api/native/v1/account/keys")
    }
    pub fn recipient(&self, username: &str) -> Result<Recipient> {
        self.get(&format!("api/native/v1/recipients/{username}"))
    }
    pub fn upload_key(&self, key: &AccountKeyUpload) -> Result<AccountKeyBundle> {
        let response = self
            .http
            .post(url(&self.instance, "api/native/v1/account/keys")?)
            .header("Sec-Fetch-Site", "same-origin")
            .json(key)
            .send()
            .context("upload native account key")?;
        if response.status().as_u16() != 201 {
            bail!("native account key upload returned {}", response.status());
        }
        response
            .json::<Api<AccountKeyBundle>>()
            .context("decode native account key")
            .map(|value| value.data)
    }
    pub fn set_inbox_enabled(&self, enabled: bool) -> Result<()> {
        let response = self
            .http
            .patch(url(&self.instance, "api/native/v1/inbox")?)
            .header("Sec-Fetch-Site", "same-origin")
            .json(&serde_json::json!({"enabled": enabled}))
            .send()
            .context("update native inbox")?;
        if !response.status().is_success() {
            bail!("native inbox update returned {}", response.status());
        }
        Ok(())
    }
    fn get<T: for<'a> Deserialize<'a>>(&self, path: &str) -> Result<T> {
        let response = self
            .http
            .get(url(&self.instance, path)?)
            .send()
            .context("native account request")?;
        if !response.status().is_success() {
            bail!("native account request returned {}", response.status());
        }
        response
            .json::<Api<T>>()
            .context("decode native account response")
            .map(|v| v.data)
    }
}

pub fn export_self_key(private_key: &[u8]) -> Result<String> {
    if private_key.len() != 32 {
        bail!("account private key must be 32 bytes");
    }
    Ok(format!("fbsk1.{}", URL_SAFE_NO_PAD.encode(private_key)))
}
pub fn import_self_key(value: &str) -> Result<Zeroizing<Vec<u8>>> {
    let value = value
        .strip_prefix("fbsk1.")
        .context("a self-custody key must begin with fbsk1.")?;
    let key = URL_SAFE_NO_PAD
        .decode(value)
        .context("self-custody key is not base64url")?;
    if key.len() != 32 || URL_SAFE_NO_PAD.encode(&key) != value {
        bail!("self-custody key must be canonical and 32 bytes");
    }
    Ok(Zeroizing::new(key))
}
pub fn generate_self_keypair() -> Result<AccountKeyMaterial> {
    let pair = generate_account_keypair().map_err(|error| anyhow::anyhow!(error))?;
    let private = Zeroizing::new(pair[..32].to_vec());
    let public = URL_SAFE_NO_PAD.encode(&pair[32..]);
    let fingerprint = Sha256::digest(&pair[32..])
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(AccountKeyMaterial {
        private_key: private,
        public_key: public,
        fingerprint,
    })
}
pub fn validate_self_key(private_key: &[u8], public_key: &str) -> Result<()> {
    let public_key = decode_key(public_key, "account public key")?;
    let challenge = [0x42_u8; 32];
    let envelope = seal_key_for_recipient(
        &public_key,
        &challenge,
        b"filebeam:account-key:v1:validation",
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    let opened = open_recipient_envelope(
        private_key,
        &envelope,
        b"filebeam:account-key:v1:validation",
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    if opened != challenge {
        bail!("the private key does not match this account key");
    }
    Ok(())
}
pub fn seal_recipient_key(
    master_key: &[u8],
    recipient: &Recipient,
    transfer_id: &str,
) -> Result<String> {
    let public_key = decode_key(&recipient.public_key, "recipient public key")?;
    let aad = format!(
        "filebeam:recipient:v1:{transfer_id}:{}:{}",
        recipient.id, recipient.account_key_bundle_id
    );
    let envelope = seal_key_for_recipient(&public_key, master_key, aad.as_bytes())
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(URL_SAFE_NO_PAD.encode(envelope))
}
pub fn open_recipient_key(
    private_key: &[u8],
    recipient_key: &RecipientKey,
    transfer_id: &str,
) -> Result<Zeroizing<Vec<u8>>> {
    let envelope = URL_SAFE_NO_PAD
        .decode(&recipient_key.encrypted_key)
        .context("recipient envelope is not base64url")?;
    let aad = format!(
        "filebeam:recipient:v1:{transfer_id}:{}:{}",
        recipient_key.bundle.user_id, recipient_key.bundle.id
    );
    let key = open_recipient_envelope(private_key, &envelope, aad.as_bytes())
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(Zeroizing::new(key))
}
fn decode_key(value: &str, label: &str) -> Result<Vec<u8>> {
    let key = URL_SAFE_NO_PAD
        .decode(value)
        .with_context(|| format!("{label} is not base64url"))?;
    if key.len() != 32 || URL_SAFE_NO_PAD.encode(&key) != value {
        bail!("{label} must be canonical and 32 bytes");
    }
    Ok(key)
}
pub fn wrap_password_key(
    private_key: &[u8],
    password: &[u8],
    user_id: u64,
    public_key: &str,
) -> Result<String> {
    let salt =
        filebeam_encryption::generate_nonce_prefix().map_err(|error| anyhow::anyhow!(error))?;
    let salt = &salt[..16];
    let prefix = generate_nonce_prefix().map_err(|error| anyhow::anyhow!(error))?;
    let password_key = Zeroizing::new(
        derive_password_key(password, salt, 65_536, 3, 1)
            .map_err(|error| anyhow::anyhow!(error))?,
    );
    let wrapping = Zeroizing::new(
        derive_account_wrapping_key(&password_key, public_key)
            .map_err(|error| anyhow::anyhow!(error))?,
    );
    let ciphertext = encrypt_manifest(
        &wrapping,
        &prefix,
        private_key,
        format!("filebeam:account-key:v1:{user_id}:{public_key}").as_bytes(),
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    Ok(serde_json::json!({"v":1,"kdf":{"name":"argon2id","memory_kib":65536,"iterations":3,"parallelism":1},"salt":URL_SAFE_NO_PAD.encode(salt),"nonce_prefix":URL_SAFE_NO_PAD.encode(prefix),"ciphertext":URL_SAFE_NO_PAD.encode(ciphertext)}).to_string())
}
pub fn unwrap_password_key(
    envelope: &str,
    password: &[u8],
    user_id: u64,
    public_key: &str,
) -> Result<Zeroizing<Vec<u8>>> {
    #[derive(Deserialize)]
    struct Envelope {
        v: u8,
        salt: String,
        nonce_prefix: String,
        ciphertext: String,
    }
    let envelope: Envelope =
        serde_json::from_str(envelope).context("invalid account key envelope")?;
    if envelope.v != 1 {
        bail!("unsupported account key envelope");
    }
    let salt = URL_SAFE_NO_PAD
        .decode(envelope.salt)
        .context("invalid account key salt")?;
    let prefix = URL_SAFE_NO_PAD
        .decode(envelope.nonce_prefix)
        .context("invalid account key nonce")?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(envelope.ciphertext)
        .context("invalid account key ciphertext")?;
    let password_key = Zeroizing::new(
        derive_password_key(password, &salt, 65_536, 3, 1)
            .map_err(|error| anyhow::anyhow!(error))?,
    );
    let wrapping = Zeroizing::new(
        derive_account_wrapping_key(&password_key, public_key)
            .map_err(|error| anyhow::anyhow!(error))?,
    );
    let key = decrypt_manifest(
        &wrapping,
        &prefix,
        &ciphertext,
        format!("filebeam:account-key:v1:{user_id}:{public_key}").as_bytes(),
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    if key.len() != 32 {
        bail!("invalid account private key");
    }
    Ok(Zeroizing::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn self_key_exports_round_trip() {
        let key = [7_u8; 32];
        assert_eq!(
            import_self_key(&export_self_key(&key).unwrap())
                .unwrap()
                .as_slice(),
            &key
        );
        assert!(import_self_key("fbsk1.invalid").is_err());
    }
    #[test]
    fn password_envelope_round_trips() {
        let private = [4_u8; 32];
        let public = URL_SAFE_NO_PAD.encode([3_u8; 32]);
        let envelope = wrap_password_key(&private, b"password", 7, &public).unwrap();
        assert_eq!(
            unwrap_password_key(&envelope, b"password", 7, &public)
                .unwrap()
                .as_slice(),
            &private
        );
        assert!(unwrap_password_key(&envelope, b"wrong", 7, &public).is_err());
    }

    #[test]
    fn inbox_recipient_envelopes_reject_wrong_private_keys_and_tampering() {
        let recipient_pair = generate_account_keypair().unwrap();
        let wrong_pair = generate_account_keypair().unwrap();
        let public = URL_SAFE_NO_PAD.encode(&recipient_pair[32..]);
        let recipient = Recipient {
            id: 7,
            username: "receiver".into(),
            public_key: public.clone(),
            account_key_bundle_id: 11,
            version: 1,
            fingerprint: "unused".into(),
        };
        let transfer_key = [9_u8; 32];
        let encrypted_key =
            seal_recipient_key(&transfer_key, &recipient, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let bundle = AccountKeyBundle {
            id: 11,
            user_id: 7,
            version: 1,
            public_key: public,
            fingerprint: "unused".into(),
            custody_mode: "self".into(),
            encrypted_private_key: None,
            is_active: true,
        };
        let key = RecipientKey {
            bundle,
            encrypted_key,
        };
        assert_eq!(
            open_recipient_key(&recipient_pair[..32], &key, "01ARZ3NDEKTSV4RRFFQ69G5FAV")
                .unwrap()
                .as_slice(),
            transfer_key
        );
        assert!(open_recipient_key(&wrong_pair[..32], &key, "01ARZ3NDEKTSV4RRFFQ69G5FAV").is_err());
        let mut tampered = key.clone();
        tampered.encrypted_key.pop();
        assert!(
            open_recipient_key(
                &recipient_pair[..32],
                &tampered,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV"
            )
            .is_err()
        );
    }
}
