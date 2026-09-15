# Native And Browser Peer WebRTC Harness

`scripts/android/peer-webrtc.sh` runs a disposable local Filebeam backend at
`http://127.0.0.1:8027` and a local coturn service. It builds the current CLI
and browser assets, then attempts the four actual peer combinations below with
SHA-256 digests:

- Browser sender to native CLI receiver, direct ICE.
- Native CLI sender to browser receiver, direct ICE.
- Browser sender to native CLI receiver, forced TURN UDP relay.
- Native CLI sender to browser receiver, forced TURN UDP relay.

Run it from the repository root:

```sh
scripts/android/peer-webrtc.sh
```

To run a single case while diagnosing a peer, set `PEER_WEBRTC_CASES`; accepted
values are `web-to-native-direct`, `native-to-web-direct`,
`web-to-native-turn`, and `native-to-web-turn`.

Native-to-browser cases use Chromium's OPFS-backed File System Access flow and
incrementally hash writes, rather than Playwright's download event or a Blob.
For an opt-in real streaming run, provide a size in bytes; this creates a sparse
native source and still reads and hashes every transferred byte:

```sh
PEER_WEBRTC_CASES=native-to-web-direct PEER_WEBRTC_LARGE_BYTES=$((513 * 1024 * 1024)) scripts/android/peer-webrtc.sh
PEER_SCRATCH_ROOT="$PWD/test-results/android/peers/.scratch" PEER_WEBRTC_CASES=native-to-web-direct PEER_WEBRTC_LARGE_BYTES=$((4097 * 1024 * 1024)) scripts/android/peer-webrtc.sh
```

The 4,097 MiB case needs roughly 9 GiB of scratch space for the native spool
and OPFS output; use a filesystem with that capacity rather than `/tmp`.

The harness selects a non-loopback host address and publishes its concrete
TURN URL as `turn:<host-ip>:34790?transport=udp`; the exact URL and relay port
range are recorded in `test-results/android/peers/webrtc-<run>/environment.txt`.
The backend mints coturn REST credentials using its per-run HMAC secret and a
120-second expiry. The secret, live links, session tokens, and credentials are
not written to results.

Each TURN browser connection is constructed with `iceTransportPolicy: relay`.
Each native connection uses `--webrtc-relay-only`. The runner reads browser
WebRTC stats and fails unless the selected candidate pair is `relay/relay` and
neither selected candidate address is loopback. This makes the relay result a
real blocked-direct condition, rather than a local direct connection that
happened to have TURN configured.

The app container, coturn container, and four named volumes are removed on
exit. To inspect a failed environment, run:

```sh
KEEP_PEER_WEBRTC_ENV=1 scripts/android/peer-webrtc.sh
```

Use the cleanup command recorded in that run's `environment.txt` afterward.
The harness does not use or modify the existing RTC verification servers.

## Recorded Results

- Small fixtures passed browser-to-native and native-to-browser over direct ICE
and forced TURN UDP relay, with SHA-256 verification.
- Native-to-browser direct WebRTC passed with a 4097 MiB sparse fixture and
incremental browser hashing.

The checked-in runner is suitable for the Android `acceptance-ci` owner to reuse
for its URLs and signaling lifecycle. It intentionally does not build or run an
Android APK, so these results are not Android peer acceptance.
