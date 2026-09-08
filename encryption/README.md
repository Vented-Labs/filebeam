# Filebeam Encryption

`encryption/` is the Rust crate compiled to WebAssembly for the browser crypto worker. Filebeam's current v1 format encrypts file and note content before upload.

## Format

- Link-based transfers use a random 32-byte share key carried in the URL fragment.
- Password-protected transfers derive a separate key with Argon2id using a random 16-byte salt, 65,536 KiB memory, 3 iterations, and parallelism 1. HKDF-SHA-256 combines the share key and password key with info `filebeam:v1:password-protected`, so both factors are required.
- HKDF-SHA-256 derives per-item keys with info `filebeam:v1:item-key:${transferId}:${itemId}`. XChaCha20-Poly1305 encrypts chunks and manifests with a random 16-byte nonce prefix; the chunk index completes the 24-byte nonce and the manifest reserves `u64::MAX`.
- The authenticated associated data is `filebeam:v1:${transferId}:${itemId}:${index}`. Transfer and item IDs are canonical uppercase ULIDs; manifests use `manifest` for both item ID and index.
- Every file manifest entry includes a SHA-256 plaintext digest. The digest remains in the authenticated encrypted manifest and is checked before a downloaded file is committed.

## Recipients And Key Custody

Recipient delivery seals the random transfer key to the recipient's X25519 public key with HPKE (X25519/HKDF-SHA-256/ChaCha20-Poly1305), using HPKE info `filebeam:v1:recipient-envelope` and per-delivery AAD `filebeam:recipient:v1:${transferId}:${recipientId}:${bundleId}`. The envelope is 80 bytes: a 32-byte encapsulated key and 48 bytes of authenticated ciphertext. HPKE base mode does not authenticate the sender. Verify the recipient's public-key fingerprint out of band to detect key substitution.

Account onboarding can wrap a private key in the browser using the same Argon2id parameters, HKDF-SHA-256 with empty salt and info `filebeam:v1:item-key:account-key-v1:${publicKey}`, and XChaCha20-Poly1305 with AAD `filebeam:account-key:v1:${userId}:${publicKey}`. Self custody stores no private key on the service and exports an `fbsk1.` key. Neither option can protect against a server that captures an account password or serves malicious frontend code. Lost self-held keys and keys wrapped with forgotten passwords cannot be recovered.

## Build And Test

```sh
npm run build
cargo test --manifest-path encryption/Cargo.toml
```

The root build runs `wasm-pack build encryption --target web --release --no-opt` before the backend build. See [SECURITY.md](../SECURITY.md) for system-level limitations.
