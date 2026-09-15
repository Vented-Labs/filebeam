# Native Services Acceptance

Run `bash scripts/services-acceptance.sh` to use only the disposable Android HTTP peer at port 8019. Evidence is ignored under `.filebeam/test-results/native-services-acceptance`.

For live notes, first attach the existing isolated WebRTC peer with `KEEP_PEER_WEBRTC_ENV=1 PEER_WEBRTC_CASES=none bash scripts/android/peer-webrtc.sh`, then run `FILEBEAM_LIVE_ACCEPTANCE=1 bash scripts/services-acceptance.sh`. The harness uses the project-pinned `filebeam-beam-tooling:rust-1.98.0` image and `.filebeam/cargo-target-1.98`.

The harness covers native hosted password/burn notes; native registration/login cookie, self-custody `fbsk1` export/import, key upload, recipient lookup, authenticated inbox upload, inbox envelope decryption, recipient download SHA-256, wrong-account rejection, and logout; and native-to-browser plus browser-to-native hosted password/burn notes.

No live-note engine edits are included. Live single-claim/two-receiver acceptance is blocked while the sender owner is paused; it needs an owner-managed concurrent sender for valid end-to-end evidence.

## Coordination Note

Acceptance execution found that native registration sent JSON without `Accept: application/json`; Laravel followed the browser redirect and returned `200` instead of the native API's `201`. The narrowly scoped fix adds that representation header to native login and registration. No live-note code is changed.

The disposable peer returned `405` for native logout although the source route declares `DELETE`. Logout is outside this harness's register/login acceptance path and is recorded as a backend-environment blocker rather than changed here.

Inbox downloads use the authenticated native metadata and chunk endpoints. The decoded delivery key and cookie are request-scoped and are never converted to a public link or saved in a checkpoint. A paused inbox job requires same-origin session restoration and recipient-key reauthentication before it can resume.

The account-download owner resolved the inbox download path and the temporary double-`Zeroizing` build error. The live harness leaves that owned implementation untouched.
