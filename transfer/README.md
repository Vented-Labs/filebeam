# filebeam-transfer

`filebeam-transfer` is the pure, deterministic transfer-policy crate shared by
native and browser adapters. It has no networking, async runtime, browser, or
encryption dependencies. Callers provide monotonic milliseconds and execute
the returned decisions themselves. Rust is the shared policy core; the WASM
policy package exposes it to browser callers, while TypeScript owns browser
transport. This crate defines no mobile application behavior, though its
interfaces can support future mobile adapters.

## Public policy API

* `AdaptiveConcurrency` learns aggregate body throughput from `sample` and
  completed requests via `observe`. It starts at one slot, uses bounded probes,
  and backs off on `congested`. `rate()` is bytes per millisecond. Call
  `forget` when a request ends. Request keys must be unique for a request
  lifetime. Positive late callbacks after `forget` are ignored; a zero sample
  explicitly starts a new request using that key. A counter reset while active
  is handled by a lower `sample` value.
* `concurrency_limit(configured, chunk_bytes, memory_budget, platform_limit)`
  is the standard convenience model: 8 MiB shared work overhead, one plaintext
  copy and one ciphertext copy per in-flight request. Thus 128 MiB can start
  two 25 MB chunks rather than being pinned to one, while still accounting for
  both live representations. It always returns at least one so a caller can
  report its own insufficient-memory condition rather than deadlock.
* `TransferMemoryBudget` and `concurrency_limit_with_memory` are the adapter
  contract when retained buffers differ from the standard model. Supply the
  actual fixed reservation and plaintext/ciphertext copies; no copies are
  silently assumed. Browser adapters should use their 64/128 MiB budgets with
   this model, and native adapters should use their configured total budget.
   These budgets cover managed transfer buffers, not process RSS.
* `UploadTransport` validates untrusted server policy and supplies bounded
  direct/staged request sizing decisions. `part_max_count`, when supplied,
  limits the number of staged parts; adapters call `validate_part_count` before
  opening a stage and `remaining_part_count` after recovery before scheduling
  the remaining requests.
* `retryable_status` and `retry_delay_ms` centralize HTTP retry decisions.
  Supply deterministic injected jitter to `retry_delay_ms`; delays are capped
  at 60 seconds.
* `chunk_count` and `ciphertext_bytes` validate encrypted chunk layouts before
  allocation or request creation. Each ciphertext chunk includes exactly one
  `AEAD_TAG_BYTES` tag.

## Staging and progress API

`StageStatus` is the validated wire-status shape. Create `StageSession` with
the request identity, ciphertext byte count, and checksum, then call
`reconcile` for every begin/status response. `acknowledge_part` additionally
checks an immediate part acknowledgement, which must exactly equal the
requested range end. A later `reconcile` status may advance past that end to
recover a lost acknowledgement, but rejects backward offsets and
identity/checksum mismatches. `record_retry`, `reset`, and
`reprobe_after_recovery` enforce bounded retry/reset recovery; the latter is
the explicit safe point for an adapter to resume adaptive probing after a
recovered request. Adaptive decisions use only measured request/body
throughput: queue depth, encryption time, and disk events are not signals.

`TransferProgress::new` and `set_active`/`complete` keep plaintext and
ciphertext accounting separate and prevent totals or active byte counts from
exceeding their declared limits. All state types derive serde traits so native
resume storage and WASM persistence use the same representation.
