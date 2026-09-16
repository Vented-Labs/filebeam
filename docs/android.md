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

## Verified Non-Device Peers

- Native-service acceptance passed for hosted and live notes, passwords,
burn-on-read, single claim, replay rejection, and authenticated account inbox
delivery/download. It retains request-scoped inbox cookies and keys; no public
link or checkpoint stores them.
- Disposable Web/CLI HTTP acceptance passed in both directions at 513 MiB and
4097 MiB with matching SHA-256 values. Browser OPFS RSS clean peaks were
930692 KiB and 936332 KiB (delta 5640 KiB).
- Disposable native CLI/browser WebRTC acceptance passed for direct and forced
TURN relay in both directions with small fixtures, plus native-to-browser direct
4097 MiB. The runner verifies relay candidates for TURN.

## Android Emulator Evidence

Durable results cover API 26 (4 KiB) and API 35 (16 KiB) x86_64 KVM guests.
`adb reverse` was used only for disposable HTTP backends; WebRTC/TURN used UDP.

- Small HTTP transfers passed between Android and both browser and CLI in both
  directions. Android's 513 MiB upload was also verified by browser and CLI,
  with matching source/destination hashes. Browser-to-Android and
  CLI-to-Android download hashes were respectively
  `1efea6573c7826100ad1d2c84d2520c4a1cf2b8d4f9c2fb36d3a959973354152` and
  `612c20d93564635f24dbc967fbfcf6719b05ae6725c53a9a9940837f9fc1cc1b`.
- Android-to-browser and browser-to-Android WebRTC direct and forced TURN all
  passed with hashes. Direct used `host/prflx`; relay runs recorded non-loopback
  `relay/relay` candidates. The receiver regression evidence is
  `.filebeam/receiver-fix.md`.
- API 35 sent a 4097 MiB HTTP file. A fresh API 26 guest downloaded it
  (`4296015872` bytes), matching SHA-256
  `117b24fae2fe12a5145b1b11ec31540994554977cf4a357f7cb602bc5c9b8de6`.
  Peak sampled PSS was 157746 KiB, including 113332 KiB native PSS. This is
  measured PSS, not the managed-memory budget or a process-memory limit.
- API 26 normal instrumentation passed 12 tests. The latest test-signed R8 APK
  installed and launched without an application crash or native-init fatal log.
- Current API 35 normal instrumentation passed 12 tests on a fresh 16 KiB
  x86_64 guest in 47.571 s using debug APK SHA-256
  `9036e068bfb888e46820015c5d7700828af86b988a190c52c9fda4c8d9d8b42d`.
  `DocumentStorageExportRecoveryTest` completed; the run contains no Filebeam
  uncaught-exception, `EPIPE`, or native-crash record. Evidence:
  `.filebeam/final-device/api35-normal-postrebase/`.
- The final rebuild passed Android lint/unit tests, debug and R8 builds, all
  ABIs, 16 KiB alignment checks, and App Link fixtures. Test signing does not
  establish production signing.
- Android/CLI WebRTC passed all four 1 MiB direct/forced-TURN cases. The
  CLI-to-Android hash was
  `725a235ef66fd5a356daf0cfc732d952a7af8af396beab002a87501578e330f4`; the
  Android-to-CLI hash was
  `9b51165390144f966f5447b00ea8880fc090d2c7b98dd989aa7178dd2b0b10b5`.

Prior API 35 16 KiB emulator failures remain recorded: LMKD killed the app at
212272 KiB RSS, followed by a separate `SEGV_MAPERR` run. These emulator events
do not establish an application heap leak.

## Remaining Release Acceptance

- **External release matrix:** untested: physical ARM64/ARMv7, physical
low-memory behavior, real-device OS force-stop, network and low-storage paths,
manual accessibility, and the production domain/signing matrix. No complete
release-acceptance claim is made.

### Result Fields

| Gate | Result | Evidence path/run |
| --- | --- | --- |
| Export and inbox Android build/instrumentation | passed | `.filebeam/final-android-build.md` |
| Android HTTP peer matrix | passed, browser and CLI both directions | `.filebeam/android-final-peer-summary.md` |
| Android/browser direct WebRTC | passed, both directions | `.filebeam/android-final-peer-summary.md`, `.filebeam/receiver-fix.md` |
| Android/browser TURN WebRTC | passed, both directions and relay candidates | `.filebeam/android-final-peer-summary.md`, `.filebeam/receiver-fix.md` |
| CLI/Android WebRTC, four small cases | passed: both directions, direct and forced TURN | `.filebeam/android-final-peer-summary.md`, `.filebeam/receiver-fix.md` |
| API 35 (16 KiB) normal device run | passed, 12 tests | `.filebeam/final-device/api35-normal-postrebase/` |
| API 26 device run | passed, 12 tests and fresh 4097 MiB receive | `.filebeam/final-device-summary.md` |
| Physical ARM/release/domain acceptance | pending | |
