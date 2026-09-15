# Portable policy integration

Portable, untrusted protocol policy is owned by `filebeam-transfer`:

- `link::parse_share_link(&str) -> Result<ShareLink, String>` validates a ULID
  and a `v1.` 32-byte base64url key representation. It returns the encoded key;
  callers decode or use key material through `filebeam-encryption`.
- `manifest::validate_manifest(&Manifest, driver, chunk_bytes, server_items)`
  checks a decrypted final manifest against public transfer metadata. It neither
  decrypts envelopes nor accesses storage or a runtime.
- `capabilities::select_driver_limits(driver, advertised, legacy_http_bytes,
  legacy_http_count)` applies legacy limits only to HTTP. Missing WebRTC limits
  remain unbounded for compatibility with old servers, whose reservation is
  authoritative.

Native adapts its protocol response types to these APIs. Browser consumers use
the same policy through `@filebeam/transfer` WASM bindings after
`initialiseTransferPolicy()`. Keep encryption keys, envelope parsing, KDFs, and
AEAD operations in `filebeam-encryption`.
