use crate::{Result, operation};

/// Exercises native entropy and authenticated encryption on the installed ABI.
#[uniffi::export]
pub fn crypto_self_test() -> Result<bool> {
    use filebeam_encryption::{
        decrypt_chunk, encrypt_chunk, generate_nonce_prefix, generate_transfer_key,
    };
    let key = generate_transfer_key().map_err(operation)?;
    let prefix = generate_nonce_prefix().map_err(operation)?;
    let plain = b"Filebeam native encryption";
    let aad = b"filebeam:native:self-test";
    let cipher = encrypt_chunk(&key, &prefix, 0, plain, aad).map_err(operation)?;
    let decoded = decrypt_chunk(&key, &prefix, 0, &cipher, aad).map_err(operation)?;
    Ok(decoded == plain && decrypt_chunk(&key, &prefix, 0, &cipher, b"wrong").is_err())
}

/// Opens two local UDP peers and transfers an encrypted, multi-frame record.
#[uniffi::export]
pub fn webrtc_self_test() -> Result<bool> {
    filebeam_client_core::webrtc_self_test().map_err(operation)
}
