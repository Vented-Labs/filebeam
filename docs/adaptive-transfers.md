# Adaptive Transfers

## Staging Model

Adaptive upload parts are encrypted ciphertext written to a private local staging file before they are acknowledged. The default root is `storage/app/transfer-staging`; `FILEBEAM_STAGING_ROOT` may select another private local directory. It is disk-backed by default, never a RAM default, and must not be an S3/object-store mount or a web-served path.

The staging root contains transient ciphertext and lock files. The runtime user needs directory create/delete permissions and reliable `flock`. Staged files are mode `0600` and the root is mode `0700` where the filesystem permits it. Keep staging separate from public files and do not expose it through a browser.

Staging is not the final filestore. On completion, the request hashes the whole staged ciphertext and synchronously writes the full chunk to the selected private filestore(s), retrying through the normal upload flow. There is no asynchronous finalization daemon and no browser-to-S3 round trip for adaptive parts. Final ciphertext chunks remain whole files; download behavior and the canonical maximum encrypted chunk size of 25,000,000 bytes are unchanged.

## Capacity And Limits

Defaults are global to each shared staging root: 2 GiB of reserved ciphertext, 4,096 rows, a 1-hour TTL, and per-transfer limits of 512 MiB and 512 rows. Parts have a fixed 64 KiB minimum, a configurable maximum of 4 MiB, and a configurable maximum count of 512. `FILEBEAM_STAGING_PART_MAX_BYTES` is also capped at `CHUNK_MAX_SIZE`.

Available environment settings are listed in `backend/.env.example`: `FILEBEAM_STAGING_ROOT`, `FILEBEAM_STAGING_TTL_SECONDS`, byte and row quotas, and part maximum size/count. `FILEBEAM_STAGING_REQUEST_TARGET_MS` (20,000 by default) and `FILEBEAM_STAGING_REQUEST_BUDGET_MS` (120,000 by default) provide transmission timing guidance. Match the budget to the hosting proxy's request limit; the client cannot discover every external timeout automatically.

Size the filesystem beyond the configured staging quotas. It also needs request-body buffers, staging lock files, filesystem metadata, application temporary space, and space for concurrent final writes. Monitor free space. A full or unavailable staging disk causes upload backpressure (normally a retryable service error); it must not be treated as successful receipt.

Run `php artisan schedule:run` every minute as documented in [Deployment](deployment.md). The scheduled transfer-pruning command runs every 15 minutes and removes expired stages and stale crash remnants. This cron cleanup is mandatory: quotas track active reservations and cannot recover reliably without it.

## Client Behavior

Client byte progress means ciphertext has been sent; durable/downloadable progress begins only after the entire chunk is staged, verified, and synchronously published to final storage. A receiver cannot decrypt or download a partial chunk, so the first full chunk still determines first-download latency.

Clients start with one active chunk and probe additional concurrency within server and memory limits. Fast connections retain whole-chunk PUTs. Slow connections switch to staged parts, uploaded sequentially within each active chunk. Part size adapts within server-advertised bounds, targeting roughly 20 seconds per request by default. Completion is synchronous; clients query stage status when retrying an uncertain request. They must not assume a stage exists after a `410`, expiry, or storage-loss response; retransmit the pending ciphertext while the sending browser page remains alive. There is no resume across a browser restart or another browser.

## Restart And Multi-Instance Deployment

Use persistent staging storage for normal deployments so a process, container, or host restart does not discard pending ciphertext. A tmpfs may be mounted externally at `FILEBEAM_STAGING_ROOT` only when that loss is acceptable; after loss, the live browser can retransmit pending ciphertext, but a closed or restarted browser cannot resume it.

All web and queue nodes that can handle a staged transfer must mount the same staging storage at the same configured path, and that storage must provide working cross-node `flock`. Global quotas apply per shared staging root, not per node. There is no owner-routing implementation, so arbitrary load balancing with separate node-local staging is unsupported.
