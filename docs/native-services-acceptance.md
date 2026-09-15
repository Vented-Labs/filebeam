# Native Services Acceptance

Run `bash scripts/services-acceptance.sh` to use only the disposable Android HTTP peer at port 8019. Evidence is ignored under `.filebeam/test-results/native-services-acceptance`.

For live notes, first attach the existing isolated WebRTC peer with `KEEP_PEER_WEBRTC_ENV=1 PEER_WEBRTC_CASES=none bash scripts/android/peer-webrtc.sh`, then run `FILEBEAM_LIVE_ACCEPTANCE=1 bash scripts/services-acceptance.sh`. The harness uses the project-pinned `filebeam-beam-tooling:rust-1.98.0` image and `.filebeam/cargo-target-1.98`.

The harness has passed native hosted and live password/burn notes, live
single-claim and replay rejection; registration/login/logout cookie handling,
self-custody `fbsk1` export/import, key upload, recipient lookup, authenticated
inbox upload, inbox envelope decryption, recipient download SHA-256, and
wrong-account rejection. Native-to-browser and browser-to-native hosted
password/burn note acceptance also passed.

The native API requests JSON for registration, login, and logout, avoiding HTML
redirect representation handling. Live acceptance uses the isolated WebRTC peer
and a concurrent sender; no live-note engine change is implied by this harness.

Inbox downloads use the authenticated native metadata and chunk endpoints. The decoded delivery key and cookie are request-scoped and are never converted to a public link or saved in a checkpoint. A paused inbox job requires same-origin session restoration and recipient-key reauthentication before it can resume.

The account-download path and its temporary double-`Zeroizing` build failure are
resolved. This is native CLI/browser service acceptance, not Android APK/device
acceptance.
