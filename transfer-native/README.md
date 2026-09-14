# filebeam-transfer-native

Native transfer orchestration for the CLI and future adapter implementations.
`protocol` owns HTTP upload/download workflows, recovery, manifest handling,
and cryptographic orchestration using `filebeam-encryption`. `uploads` prepares
file selections, and `control` exposes platform-independent settings, progress,
cancellation, and prompt channels. CLI modules are thin adapters around these
APIs. The pure `filebeam-transfer` crate supplies policy; its WASM bindings
provide that same policy to browser transport adapters.

Native WebRTC file transport is implemented for the CLI. A sender encrypts its
selection into immutable local ciphertext artifacts, publishes a live share,
and serves compatible receivers over an ordered reliable data channel. The
share-ready event is emitted after publication so the CLI can write the link to
stdout while the sender remains alive. Cancellation stops serving but retains
the job, allowing a resume to serve the same link until server expiry. Native
notes, burn-on-read notes, and account-key inbox delivery are not implemented.
The live downloader requires ordinary manifest items; it does not synthesize an
item for an itemless note payload.

Direct native peers require explicit receiver consent because the sender can
learn the receiver's network address. CLI callers may accept this with
`--accept-peer-address-exposure`. Relay-only callers use
`--webrtc-relay-only`, which requires a `turn:` UDP ICE URL and bypasses the
direct-peer prompt. The `webrtc` crate 0.20.5 used here does not support TURN
over TCP or TLS. TURN-over-UDP relay-only has been exercised, but that is not
full TURN support and is not a security guarantee or a general proof that a
relay prevents every form of address exposure.

The CLI upload pipeline uses one ciphertext producer with a bounded consumer
window: it prepares and persists one encrypted chunk ahead while concurrent
network consumers upload completed chunks. Downloading uses one global,
fair file/chunk queue rather than serializing every chunk of the first file.
The protocol schedulers bound active requests and blocking preparation work
across the selected transfer.

`checkpoint::Store::create` creates a new private job directory and fails if it
already exists; `Store::open` requires an existing directory and never creates
one. Each store takes an exclusive advisory lock which the OS releases after a
crash. Checkpoints and immutable prepared ciphertext are written to a synced
temporary `0600` file, atomically renamed, then followed by a directory sync on
Unix. An incomplete temporary file is validated and cleared on retry; an older
checkpoint remains intact if writing the replacement fails.

Job storage is protected by filesystem permissions, not encryption at rest.
Private checkpoint JSON and sidecars can contain transfer keys, tokens, source
metadata, and completion links. Live WebRTC senders retain encrypted ciphertext
artifacts locally for their lifetime and across a restart. Jobs may also retain
a generated plaintext ZIP or authenticated partial download while work is
incomplete. Passwords and password-derived download keys are not persisted.
Callers must not log private job contents. Checkpoint reads and immutable
ciphertext have bounded sizes; runtime buffer budgets and fixed-size WebRTC
framing prevent each peer from multiplying a full ciphertext chunk in memory.

`runtime::Runtime` provides separate network, CPU, and filesystem semaphores.
`retry(cancelled, attempts, staging, operation, retryable)` retries only errors
classified by the caller and stops both active operations and waits when the
supplied `CancellationToken` is cancelled. It is for idempotent operations.

`http::get_range(client, &RangeRequest { url, start, end, ciphertext_total,
expected_etag, headers_timeout, idle_timeout }, cancelled)` fetches one bounded
ciphertext continuation range. It validates `206`, ETag, range capability,
content range, and exact length; callers must reassemble and authenticate the
whole encrypted chunk before releasing plaintext. This replaces the older
positional range API.

`policy` re-exports the exact types and functions from `filebeam-transfer`.
Native adapters must use these rather than copy transfer sizing, staging, or
retry decisions.

Unix job directories are created with mode `0700` and files with mode `0600`.
Windows creates roots, job directories, and files with a protected DACL that
contains only the current token owner and `SYSTEM`, with object/container
inheritance. Existing Windows paths are verified against that owner SID and
DACL before use. Symlinks and unsafe names are rejected on both platforms.

The Windows DACL path was cross-compiled and its test executable linked with
real MinGW during development. This is not validation of every final CLI
release artifact. Runtime Windows test execution has not yet been performed;
it remains part of the platform verification work.

Verification results are specific to the source revision and binaries tested.
Earlier native and browser runs passing does not certify later source changes
or release artifacts whose SHA-256 values differ. A small temporary benchmark
showing about a 3% improvement is not sufficiently repeated or controlled for
a production performance claim; it remains outside the repository under
`/tmp` and should not be generalized.
