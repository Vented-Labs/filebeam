use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256, setup_receiver, setup_sender,
};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 24;
const NONCE_PREFIX_BYTES: usize = 16;
const AEAD_TAG_BYTES: usize = 16;
const HPKE_ENCAPSULATED_KEY_BYTES: usize = 32;
const HPKE_ENVELOPE_BYTES: usize = HPKE_ENCAPSULATED_KEY_BYTES + KEY_BYTES + AEAD_TAG_BYTES;
const PROTOCOL_VERSION: u16 = 1;
const HPKE_INFO: &[u8] = b"filebeam:v1:recipient-envelope";
const ITEM_KEY_INFO_PREFIX: &[u8] = b"filebeam:v1:item-key:";
const ARGON2_MEMORY_KIB: u32 = 65_536;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;
const PASSWORD_SALT_BYTES: usize = 16;
const PASSWORD_PROTECTED_KEY_INFO: &[u8] = b"filebeam:v1:password-protected";
const MAX_PASSWORD_BYTES: usize = 1_024;
const MAX_PLAINTEXT_BYTES: usize = 25_000_000 - AEAD_TAG_BYTES;
const MAX_MANIFEST_PLAINTEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_ASSOCIATED_DATA_BYTES: usize = 4 * 1024;

type RecipientKem = X25519HkdfSha256;
type RecipientPublicKey = <RecipientKem as KemTrait>::PublicKey;
type RecipientPrivateKey = <RecipientKem as KemTrait>::PrivateKey;
type RecipientEncappedKey = <RecipientKem as KemTrait>::EncappedKey;

#[cfg(target_arch = "wasm32")]
type ApiError = JsError;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(String);

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for ApiError {}

fn error(message: impl Into<String>) -> ApiError {
    #[cfg(target_arch = "wasm32")]
    {
        JsError::new(&message.into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        ApiError(message.into())
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn protocol_version() -> u16 {
    PROTOCOL_VERSION
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn generate_transfer_key() -> ApiResult<Vec<u8>> {
    random_bytes(KEY_BYTES)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn generate_nonce_prefix() -> ApiResult<Vec<u8>> {
    random_bytes(NONCE_PREFIX_BYTES)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn derive_password_key(
    password: &[u8],
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> ApiResult<Vec<u8>> {
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        return Err(error("password must be between 1 and 1024 bytes"));
    }
    if salt.len() != PASSWORD_SALT_BYTES {
        return Err(error("the Argon2id salt must be exactly 16 bytes"));
    }
    if (memory_kib, iterations, parallelism)
        != (ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, ARGON2_PARALLELISM)
    {
        return Err(error("unsupported Argon2id parameters"));
    }

    let params = Params::new(memory_kib, iterations, parallelism, Some(KEY_BYTES))
        .map_err(|err| error(err.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = Zeroizing::new(vec![0_u8; KEY_BYTES]);

    argon2
        .hash_password_into(password, salt, &mut output)
        .map_err(|err| error(err.to_string()))?;

    Ok(output.to_vec())
}

/// Combines the separately shared transfer key and the Argon2id password key.
/// Neither input alone can decrypt a password-protected transfer.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn derive_password_protected_key(share_key: &[u8], password_key: &[u8]) -> ApiResult<Vec<u8>> {
    validate_key(share_key, "share key")?;
    validate_key(password_key, "password key")?;

    let mut output = Zeroizing::new([0_u8; KEY_BYTES]);
    Hkdf::<Sha256>::new(Some(share_key), password_key)
        .expand(PASSWORD_PROTECTED_KEY_INFO, output.as_mut())
        .map_err(|_| error("password-protected key derivation failed"))?;
    Ok(output.to_vec())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn derive_item_key(master_key: &[u8], transfer_id: &str, item_id: &str) -> ApiResult<Vec<u8>> {
    validate_key(master_key, "master key")?;
    validate_ulid(transfer_id, "transfer ID")?;
    validate_ulid(item_id, "item ID")?;

    let mut info =
        Vec::with_capacity(ITEM_KEY_INFO_PREFIX.len() + transfer_id.len() + 1 + item_id.len());
    info.extend_from_slice(ITEM_KEY_INFO_PREFIX);
    info.extend_from_slice(transfer_id.as_bytes());
    info.push(b':');
    info.extend_from_slice(item_id.as_bytes());
    let mut output = Zeroizing::new([0_u8; KEY_BYTES]);
    Hkdf::<Sha256>::new(None, master_key)
        .expand(&info, output.as_mut())
        .map_err(|_| error("item key derivation failed"))?;
    Ok(output.to_vec())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn encrypt_chunk(
    key: &[u8],
    nonce_prefix: &[u8],
    chunk_index: u32,
    plaintext: &[u8],
    associated_data: &[u8],
) -> ApiResult<Vec<u8>> {
    encrypt_chunk_at_index(
        key,
        nonce_prefix,
        u64::from(chunk_index),
        plaintext,
        associated_data,
        MAX_PLAINTEXT_BYTES,
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn decrypt_chunk(
    key: &[u8],
    nonce_prefix: &[u8],
    chunk_index: u32,
    ciphertext: &[u8],
    associated_data: &[u8],
) -> ApiResult<Vec<u8>> {
    decrypt_chunk_at_index(
        key,
        nonce_prefix,
        u64::from(chunk_index),
        ciphertext,
        associated_data,
        MAX_PLAINTEXT_BYTES,
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn encrypt_manifest(
    key: &[u8],
    nonce_prefix: &[u8],
    plaintext: &[u8],
    associated_data: &[u8],
) -> ApiResult<Vec<u8>> {
    encrypt_chunk_at_index(
        key,
        nonce_prefix,
        u64::MAX,
        plaintext,
        associated_data,
        MAX_MANIFEST_PLAINTEXT_BYTES,
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn decrypt_manifest(
    key: &[u8],
    nonce_prefix: &[u8],
    ciphertext: &[u8],
    associated_data: &[u8],
) -> ApiResult<Vec<u8>> {
    decrypt_chunk_at_index(
        key,
        nonce_prefix,
        u64::MAX,
        ciphertext,
        associated_data,
        MAX_MANIFEST_PLAINTEXT_BYTES,
    )
}

/// Incremental client-only file fingerprinting for encrypted manifests.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Default)]
pub struct Sha256Hasher(Sha256);

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Sha256Hasher {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn finalize(self) -> Vec<u8> {
        self.0.finalize().to_vec()
    }
}

/// Returns the private key followed by its public key. The caller must wrap the
/// first 32 bytes before persisting them.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn generate_account_keypair() -> ApiResult<Vec<u8>> {
    let private_bytes = Zeroizing::new(random_array()?);
    let private_key = StaticSecret::from(*private_bytes);
    let public_key = PublicKey::from(&private_key);
    let mut keypair = Zeroizing::new(Vec::with_capacity(KEY_BYTES * 2));
    keypair.extend_from_slice(private_key.as_bytes());
    keypair.extend_from_slice(public_key.as_bytes());

    Ok(keypair.to_vec())
}

/// Seals a 32-byte transfer key using base-mode HPKE.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn seal_key_for_recipient(
    recipient_public_key: &[u8],
    transfer_key: &[u8],
    associated_data: &[u8],
) -> ApiResult<Vec<u8>> {
    validate_key(recipient_public_key, "recipient public key")?;
    validate_key(transfer_key, "transfer key")?;
    validate_associated_data(associated_data)?;
    let recipient_public_key = RecipientPublicKey::from_bytes(recipient_public_key)
        .map_err(|_| error("invalid recipient public key"))?;
    let (encapped_key, mut sender_context) = setup_sender::<
        ChaCha20Poly1305,
        HkdfSha256,
        RecipientKem,
    >(&OpModeS::Base, &recipient_public_key, HPKE_INFO)
    .map_err(|_| error("recipient HPKE setup failed"))?;
    let ciphertext = sender_context
        .seal(transfer_key, associated_data)
        .map_err(|_| error("recipient key encryption failed"))?;
    if ciphertext.len() != KEY_BYTES + AEAD_TAG_BYTES {
        return Err(error("recipient envelope has an invalid ciphertext length"));
    }
    let encapped_key = encapped_key.to_bytes();
    if encapped_key.len() != HPKE_ENCAPSULATED_KEY_BYTES {
        return Err(error(
            "recipient envelope has an invalid encapsulated key length",
        ));
    }
    let mut envelope = Vec::with_capacity(HPKE_ENVELOPE_BYTES);
    envelope.extend_from_slice(&encapped_key);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn open_recipient_envelope(
    recipient_private_key: &[u8],
    envelope: &[u8],
    associated_data: &[u8],
) -> ApiResult<Vec<u8>> {
    validate_key(recipient_private_key, "recipient private key")?;
    validate_associated_data(associated_data)?;
    if envelope.len() != HPKE_ENVELOPE_BYTES {
        return Err(error("recipient envelope must be exactly 80 bytes"));
    }
    let recipient_private_key = RecipientPrivateKey::from_bytes(recipient_private_key)
        .map_err(|_| error("invalid recipient private key"))?;
    let encapped_key = RecipientEncappedKey::from_bytes(&envelope[..HPKE_ENCAPSULATED_KEY_BYTES])
        .map_err(|_| error("invalid recipient encapsulated key"))?;
    let mut recipient_context = setup_receiver::<ChaCha20Poly1305, HkdfSha256, RecipientKem>(
        &OpModeR::Base,
        &recipient_private_key,
        &encapped_key,
        HPKE_INFO,
    )
    .map_err(|_| error("recipient HPKE setup failed"))?;
    let plaintext = recipient_context
        .open(&envelope[HPKE_ENCAPSULATED_KEY_BYTES..], associated_data)
        .map_err(|_| error("recipient envelope authentication failed"))?;
    if plaintext.len() != KEY_BYTES {
        return Err(error("recipient envelope plaintext must be 32 bytes"));
    }
    Ok(plaintext)
}

fn encrypt_chunk_at_index(
    key: &[u8],
    nonce_prefix: &[u8],
    index: u64,
    plaintext: &[u8],
    associated_data: &[u8],
    maximum_plaintext_bytes: usize,
) -> ApiResult<Vec<u8>> {
    validate_plaintext(plaintext, maximum_plaintext_bytes)?;
    validate_associated_data(associated_data)?;
    cipher(key)?
        .encrypt(
            &nonce(nonce_prefix, index)?,
            Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|_| error("chunk encryption failed"))
}

fn decrypt_chunk_at_index(
    key: &[u8],
    nonce_prefix: &[u8],
    index: u64,
    ciphertext: &[u8],
    associated_data: &[u8],
    maximum_plaintext_bytes: usize,
) -> ApiResult<Vec<u8>> {
    if ciphertext.len() < AEAD_TAG_BYTES
        || ciphertext.len() > maximum_plaintext_bytes + AEAD_TAG_BYTES
    {
        return Err(error("ciphertext length is invalid"));
    }
    validate_associated_data(associated_data)?;
    cipher(key)?
        .decrypt(
            &nonce(nonce_prefix, index)?,
            Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|_| error("chunk authentication failed"))
}

fn cipher(key: &[u8]) -> ApiResult<XChaCha20Poly1305> {
    validate_key(key, "encryption key")?;
    XChaCha20Poly1305::new_from_slice(key).map_err(|_| error("encryption key must be 32 bytes"))
}

fn nonce(prefix: &[u8], index: u64) -> ApiResult<XNonce> {
    if prefix.len() != NONCE_PREFIX_BYTES {
        return Err(error("nonce prefix must be 16 bytes"));
    }

    let mut nonce = [0_u8; NONCE_BYTES];
    nonce[..NONCE_PREFIX_BYTES].copy_from_slice(prefix);
    nonce[NONCE_PREFIX_BYTES..].copy_from_slice(&index.to_be_bytes());

    XNonce::try_from(nonce.as_slice()).map_err(|_| error("nonce must be 24 bytes"))
}

fn validate_key(key: &[u8], label: &str) -> ApiResult<()> {
    if key.len() != KEY_BYTES {
        return Err(error(format!("{label} must be 32 bytes")));
    }
    Ok(())
}

fn validate_plaintext(plaintext: &[u8], maximum_plaintext_bytes: usize) -> ApiResult<()> {
    if plaintext.len() > maximum_plaintext_bytes {
        return Err(error("plaintext exceeds the configured limit"));
    }
    Ok(())
}

fn validate_associated_data(associated_data: &[u8]) -> ApiResult<()> {
    if associated_data.len() > MAX_ASSOCIATED_DATA_BYTES {
        return Err(error("associated data exceeds the 4 KiB limit"));
    }
    Ok(())
}

fn validate_ulid(value: &str, label: &str) -> ApiResult<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 26
        || !matches!(bytes[0], b'0'..=b'7')
        || bytes.iter().skip(1).any(|byte| !matches!(byte, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
    {
        return Err(error(format!("{label} must be a canonical uppercase ULID")));
    }
    Ok(())
}

fn random_bytes(length: usize) -> ApiResult<Vec<u8>> {
    let mut bytes = vec![0_u8; length];
    getrandom::getrandom(&mut bytes).map_err(|err| error(err.to_string()))?;

    Ok(bytes)
}

fn random_array() -> ApiResult<[u8; KEY_BYTES]> {
    let mut bytes = [0_u8; KEY_BYTES];
    getrandom::getrandom(&mut bytes).map_err(|err| error(err.to_string()))?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_round_trip_authenticates_context_and_index() {
        let key = [7_u8; KEY_BYTES];
        let nonce_prefix = [3_u8; NONCE_PREFIX_BYTES];
        let aad = b"filebeam:v1:01ARZ3NDEKTSV4RRFFQ69G5FAV:01ARZ3NDEKTSV4RRFFQ69G5FAW:4";
        let ciphertext = encrypt_chunk(&key, &nonce_prefix, 4, b"private", aad).unwrap();

        assert_eq!(
            decrypt_chunk(&key, &nonce_prefix, 4, &ciphertext, aad).unwrap(),
            b"private"
        );
        assert!(decrypt_chunk(&key, &nonce_prefix, 4, &ciphertext, b"wrong").is_err());
        assert!(decrypt_chunk(&key, &nonce_prefix, 5, &ciphertext, aad).is_err());
        assert!(decrypt_chunk(&[8_u8; KEY_BYTES], &nonce_prefix, 4, &ciphertext, aad).is_err());
        let mut tampered = ciphertext.clone();
        tampered[0] ^= 1;
        assert!(decrypt_chunk(&key, &nonce_prefix, 4, &tampered, aad).is_err());
        assert!(
            decrypt_chunk(
                &key,
                &nonce_prefix,
                4,
                &ciphertext[..AEAD_TAG_BYTES - 1],
                aad
            )
            .is_err()
        );
    }

    #[test]
    fn password_derivation_uses_only_the_fixed_parameters() {
        let first = derive_password_key(
            b"password",
            &[1_u8; PASSWORD_SALT_BYTES],
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
        )
        .unwrap();
        let second = derive_password_key(
            b"password",
            &[1_u8; PASSWORD_SALT_BYTES],
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
        )
        .unwrap();
        let other_salt = derive_password_key(
            b"password",
            &[2_u8; PASSWORD_SALT_BYTES],
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
        )
        .unwrap();

        assert_eq!(first, second);
        assert_ne!(first, other_salt);
        assert!(derive_password_key(b"password", &[1_u8; 16], 8, 1, 1).is_err());
        assert!(derive_password_key(b"password", &[1_u8; 15], 65_536, 3, 1).is_err());
    }

    #[test]
    fn password_protected_keys_require_both_factors() {
        let share_key = [7_u8; KEY_BYTES];
        let password_key = derive_password_key(
            b"correct password",
            &[1_u8; PASSWORD_SALT_BYTES],
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
        )
        .unwrap();
        let master_key = derive_password_protected_key(&share_key, &password_key).unwrap();
        let same_master_key = derive_password_protected_key(&share_key, &password_key).unwrap();
        let wrong_password_key = derive_password_key(
            b"wrong password",
            &[1_u8; PASSWORD_SALT_BYTES],
            ARGON2_MEMORY_KIB,
            ARGON2_ITERATIONS,
            ARGON2_PARALLELISM,
        )
        .unwrap();
        let wrong_master_key =
            derive_password_protected_key(&share_key, &wrong_password_key).unwrap();
        let prefix = [3_u8; NONCE_PREFIX_BYTES];
        let ciphertext = encrypt_manifest(&master_key, &prefix, b"private", b"v1").unwrap();

        assert_eq!(master_key, same_master_key);
        assert_eq!(
            master_key,
            [
                106, 167, 112, 204, 49, 177, 73, 122, 76, 79, 41, 233, 219, 158, 108, 66, 169, 49,
                93, 34, 191, 104, 82, 251, 82, 165, 21, 172, 213, 206, 145, 38,
            ]
        );
        assert_ne!(master_key, share_key);
        assert!(decrypt_manifest(&share_key, &prefix, &ciphertext, b"v1").is_err());
        assert!(decrypt_manifest(&wrong_master_key, &prefix, &ciphertext, b"v1").is_err());
    }

    #[test]
    fn item_keys_are_deterministic_and_domain_separated() {
        let master_key = [1_u8; KEY_BYTES];
        let transfer_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let item_id = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
        let key = derive_item_key(&master_key, transfer_id, item_id).unwrap();

        assert_eq!(
            key,
            derive_item_key(&master_key, transfer_id, item_id).unwrap()
        );
        assert_ne!(
            key,
            derive_item_key(&master_key, transfer_id, "01ARZ3NDEKTSV4RRFFQ69G5FAX").unwrap()
        );
        assert_ne!(
            key,
            derive_item_key(&master_key, "01ARZ3NDEKTSV4RRFFQ69G5FAX", item_id).unwrap()
        );
        assert!(derive_item_key(&master_key, "not-a-ulid", item_id).is_err());
        assert!(derive_item_key(&master_key[..31], transfer_id, item_id).is_err());
    }

    #[test]
    fn chunk_encryption_has_a_deterministic_vector() {
        let ciphertext = encrypt_chunk(
            &[7_u8; KEY_BYTES],
            &[3_u8; NONCE_PREFIX_BYTES],
            4,
            b"private",
            b"filebeam:v1:01ARZ3NDEKTSV4RRFFQ69G5FAV:01ARZ3NDEKTSV4RRFFQ69G5FAW:4",
        )
        .unwrap();
        assert_eq!(
            ciphertext,
            [
                242, 219, 73, 154, 253, 249, 67, 243, 98, 179, 27, 29, 252, 1, 84, 30, 33, 54, 24,
                121, 9, 31, 153,
            ]
        );
    }

    #[test]
    fn sha256_hasher_matches_known_incremental_vectors() {
        let empty = Sha256Hasher::new().finalize();
        assert_eq!(
            empty,
            [
                227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39,
                174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85
            ]
        );
        let mut hasher = Sha256Hasher::new();
        hasher.update(b"a");
        hasher.update(b"bc");
        assert_eq!(
            hasher.finalize(),
            [
                186, 120, 22, 191, 143, 1, 207, 234, 65, 65, 64, 222, 93, 174, 34, 35, 176, 3, 97,
                163, 150, 23, 122, 156, 180, 16, 255, 97, 242, 0, 21, 173
            ]
        );
    }

    #[test]
    fn chunks_allow_25_megabyte_ciphertext_but_manifests_remain_limited() {
        let key = [7_u8; KEY_BYTES];
        let prefix = [3_u8; NONCE_PREFIX_BYTES];
        let plaintext = vec![0_u8; MAX_PLAINTEXT_BYTES];
        assert_eq!(
            encrypt_chunk(&key, &prefix, 0, &plaintext, b"")
                .unwrap()
                .len(),
            25_000_000
        );
        assert!(encrypt_chunk(&key, &prefix, 0, &[0_u8; MAX_PLAINTEXT_BYTES + 1], b"").is_err());
        assert!(
            encrypt_manifest(
                &key,
                &prefix,
                &[0_u8; MAX_MANIFEST_PLAINTEXT_BYTES + 1],
                b""
            )
            .is_err()
        );
    }

    #[test]
    fn manifest_uses_reserved_nonce_and_allows_empty_plaintext() {
        let key = [7_u8; KEY_BYTES];
        let prefix = [3_u8; NONCE_PREFIX_BYTES];
        let aad = b"filebeam:v1:01ARZ3NDEKTSV4RRFFQ69G5FAV:manifest";
        let ciphertext = encrypt_manifest(&key, &prefix, b"", aad).unwrap();
        assert_eq!(
            decrypt_manifest(&key, &prefix, &ciphertext, aad).unwrap(),
            b""
        );
        assert_ne!(
            ciphertext,
            encrypt_chunk(&key, &prefix, u32::MAX, b"", aad).unwrap()
        );
    }

    #[test]
    fn recipient_hpke_envelope_round_trip_and_rejects_malformed_values() {
        let private_key = [9_u8; KEY_BYTES];
        let public_key = PublicKey::from(&StaticSecret::from(private_key));
        let transfer_key = [5_u8; KEY_BYTES];
        let envelope =
            seal_key_for_recipient(public_key.as_bytes(), &transfer_key, b"transfer").unwrap();

        assert_eq!(
            open_recipient_envelope(&private_key, &envelope, b"transfer").unwrap(),
            transfer_key
        );
        assert_eq!(envelope.len(), HPKE_ENVELOPE_BYTES);
        assert!(open_recipient_envelope(&private_key, &envelope[..79], b"transfer").is_err());
        let mut tampered = envelope.clone();
        tampered[HPKE_ENCAPSULATED_KEY_BYTES] ^= 1;
        assert!(open_recipient_envelope(&private_key, &tampered, b"transfer").is_err());
        let mut low_order = envelope.clone();
        low_order[..HPKE_ENCAPSULATED_KEY_BYTES].fill(0);
        assert!(open_recipient_envelope(&private_key, &low_order, b"transfer").is_err());
        assert!(seal_key_for_recipient(&[0_u8; KEY_BYTES], &transfer_key, b"transfer").is_err());
        assert!(
            seal_key_for_recipient(public_key.as_bytes(), &transfer_key[..31], b"transfer")
                .is_err()
        );
    }
}
