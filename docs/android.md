# Native Android Status

Android supports API 26+ and uses Kotlin/Compose with the shared Rust HTTP and
WebRTC engine. This is an implementation and acceptance record, not a claim of
complete release acceptance.

## Implemented

1. **Storage and durable publication:** seekable provider inputs retain bounded,
reopenable identities; transient/non-seekable inputs snapshot. Export journals,
persisted grants, mutation checks, commit/abort handling, and cleanup are
implemented. Instrumentation covers provider recovery and interrupted export.
2. **Shared execution:** `client-core` owns worker lifetime, cancellation,
prompt snapshots, stale-response rejection, and the managed transfer budget.
The managed budget is not a total-process-memory bound.
3. **Secret custody:** Android Keystore wraps checkpoint/catalog secrets;
invalidated or restored keys fail closed. Transfer passwords are re-requested
after restart.
4. **File options:** password, retention, delete/revocation, and live-end
semantics are implemented in shared Rust. Pause, ending live sharing, revoking,
and removing local state remain separate operations.
5. **Platform integration:** UIDT/foreground-service adapters, recovery,
network and permission handling, document export, and paste/share links are
implemented. App Links still require production signing and domain association.
6. **Parity:** document trees, notes including burn-on-read and live single
claim, Turbo, account authentication/key `fbsk1.` import/export, username/inbox
delivery, and native inbox/download UI are implemented.

Server push notifications remain a separate backend capability; existing email
or database notices are not treated as push delivery.

## Verified Outside Android Devices

- Native-service acceptance passed for hosted and live notes, passwords,
burn-on-read, single claim, replay rejection, and authenticated account inbox
delivery/download. It retains request-scoped inbox cookies and keys; no public
link or checkpoint stores them.
- Disposable Web/CLI HTTP acceptance passed in both directions at 513 MiB and
4097 MiB with matching SHA-256 values. Browser OPFS RSS clean peaks were
930692 KiB and 936332 KiB (delta 5640 KiB).
- Disposable native CLI/browser WebRTC acceptance passed for direct and forced
TURN relay in both directions with small fixtures, plus native-to-browser direct
4097 MiB. The runner verifies relay candidates for TURN; it does not run Android.

## Remaining Release Acceptance

- **Android full gate:** the previous 10-test gate passed. Rebuild and run the
new export and inbox coverage, then record its final result: `pending`.
- **Real Android peer matrix:** run Android-to-browser/CLI HTTP and direct/TURN
WebRTC in both directions, including 513 MiB and 4097 MiB hashes and Android RSS:
`pending`.
- **Device/release matrix:** API 26 and current Android, physical low-memory
ARM64 and ARMv7, 4/16 KiB kernels, accessibility, signed release/R8 smoke, and
production App Links/domain keys: `pending`. Test signing is not production
signing; physical ARM hardware and production-domain keys are not available as
acceptance evidence.

### Result Fields

| Gate | Result | Evidence path/run |
| --- | --- | --- |
| Export and inbox Android build/instrumentation | pending | |
| Android HTTP peer matrix | pending | |
| Android direct WebRTC peer matrix | pending | |
| Android TURN WebRTC peer matrix | pending | |
| API 26 device run | pending | |
| Physical ARM/release/domain acceptance | pending | |
