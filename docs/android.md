# Native Android roadmap

The Android project is under `mobile/android/`, supports API 26+, and shares Rust
client logic with the CLI. Both HTTP and WebRTC are part of the first usable
file-transfer milestone. Kotlin/Compose handles Android UI; Rust owns encryption
and transfer behavior. Future iOS/desktop frontends reuse those Rust boundaries.

## Scaffold delivered

- Reproducible Android/Rust/UniFFI build, native Material 3 screens, native
  transfer control, file-provider snapshots and exports, and background adapters.
- Shared `client-core` worker ownership adopted by the CLI, with non-blocking
  prompt snapshots, stale-response rejection, and explicit cancellation.
- Per-transport capability discovery instead of applying HTTP limits to WebRTC.
- WebRTC receiver ciphertext retention for authenticated partial-item recovery.
- ABI/crypto/WebRTC instrumentation and native-library alignment checks.

## Complete the first file-transfer milestone

1. **Storage and durable publication:** add a source handle abstraction for
   seekable document-provider descriptors, with explicit descriptor ownership,
   reopenable identities, offset/length bounds, and source-mutation checks. Keep
   snapshots for transient/non-seekable providers. Persist destination grants
   and export offsets where supported; test provider-specific commit/abort and
   process death during publication. Add abandoned-import cleanup.
2. **Shared execution:** consolidate operation-specific Tokio runtimes and
   cancellation bridges into an owned scheduler with a global memory budget.
   Account for KDF, manifests, crypto copies, and runtime overhead. Provide
   structured error categories and authenticated, versioned job catalogs.
3. **Secret custody:** wrap all native checkpoint keys/tokens through a host
   secret-store interface using Android Keystore. Preserve the existing policy
   of re-prompting for transfer passwords after restart. Test invalidated keys,
   restored backups, and interrupted checkpoint migrations.
4. **Core file options:** add password-protected sending, retention options,
   remote delete-token persistence/revocation, and explicit live-end semantics.
   Pause, ending a live share, revoking a transfer, and removing local data must
   remain distinct operations. Implement these in shared Rust.
5. **Platform reliability:** test system-stopped UIDT jobs, foreground-service
   time budgets, force-stop, network changes, notification denial, low storage,
   permission revocation, and resume while the original worker is stopping.
   Configure verified App Links once release signing/domain association exists;
   retain paste/share handling for arbitrary self-hosted instances.
6. **Release acceptance:** Android↔Web and Android↔CLI in both directions, HTTP,
   direct WebRTC, and TURN relay; >512 MiB and >4 GiB real file fixtures on an
   appropriately configured instance; flat memory growth as file size grows;
   physical low-memory ARM64 and ARMv7 devices, API 26/current Android, 4/16 KiB
   kernels, accessibility, and signed release/R8 smoke tests.

The scaffold is not a claim that this acceptance matrix has been completed.

## Remaining parity

- Document-tree selection with existing Rust ZIP/individual-file semantics.
- Shared note operations and best-effort burn-on-read consumption; native
  editor/viewer, including the live-note receiver's single-claim semantics.
- Turbo Transfer is available for HTTP file uploads without inbox recipients. The
  native sender checkpoints and publishes the encrypted early descriptor before
  chunk production, exposes the normal share link immediately, and heartbeats
  while upload work is active. Received Turbo links are opened through the
  normal native download job, not a transfer-ID endpoint.
- Native account authentication API, inbox/username delivery, and account-key
  custody/import/export compatible with `fbsk1.`. These flows use native screens.
- Local completion/inbox UX; server push notifications need their own backend
  capability rather than being inferred from existing email/database notices.

## Cross-platform direction

Move portable link/manifest validation and capability policies into `transfer/`
as those areas are extracted; expose them to the browser through `transfer-wasm`.
Keep key/envelope operations in `encryption/`. The native engine and client
facade are shared by CLI, Android, future Swift/iOS, and desktop applications.

iOS must retain a transport/scheduling seam for background `URLSession`
operations on encrypted artifacts. Kotlin/Swift bindings do not abstract away
platform file permissions, secure storage, or background-execution restrictions.
