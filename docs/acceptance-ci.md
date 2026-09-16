# Android acceptance CI

This inventory separates repeatable build coverage from release acceptance. A
passing loopback, binding, or APK check is not Android-to-peer interoperability
evidence.

## Fast CI

`Tests / Android` is the required Android gate and runs on every pull request,
including changes under `client-core/`, `client-ffi/`, `transfer-native/`,
`encryption/`, `transfer/`, and `mobile/`. It remains part of the required
`Tests / Test` aggregate alongside the web, CLI, and server jobs. It runs `bash scripts/android/check.sh`, which runs Rust
formatting, Clippy with warnings denied, Rust tests, Android lint/unit tests,
debug and unsigned release (R8) APK builds, and native ELF/APK 16 KiB alignment
validation. `bash scripts/android/device-test.sh` boots the pinned API 35 16 KiB
emulator and runs installed-ABI instrumentation. The standard workflow retains
the debug APK and reports; it deliberately does not run large fixtures or depend
on a public TURN service.

## Opt-in acceptance

The `Android acceptance` workflow is manual-only. Its default API 35 image has
16 KiB pages; select `api26-4k` to build the existing Docker image with
`SYSTEM_IMAGE=system-images;android-26;google_apis;x86_64`. It runs the normal
native binding instrumentation but does not create large fixtures unless
`prepare_large_fixtures` is selected. It does not claim peer interoperability
until the peer harness records the evidence below.

## Opt-in peer acceptance

`bash scripts/android/acceptance.sh --prepare` creates deterministic sparse 513
MiB and 4097 MiB fixtures on the workspace filesystem under
`test-results/android/acceptance/`. It writes size and SHA-256 evidence without
putting data in `/tmp`. This is intentionally opt-in because hashing and moving
the 4 GiB fixture are real I/O operations.

For an instance configured to admit those transfers, use an Android test class
that accepts `instance`, `transport`, `direction`, and fixture arguments and run
it through `scripts/android/device-test.sh`. The emulator reaches a host-local
backend through `10.0.2.2`; the backend must advertise the selected HTTP or
WebRTC driver and sufficiently large per-transport limits. Direct WebRTC needs
two routable peers. TURN validation needs a dedicated TURN-over-UDP deployment
and relay-only client configuration. Do not use a loopback ICE self-test as
evidence for either condition.

### Local WebRTC peer environment

`scripts/android/peer-webrtc.sh` owns an isolated local backend at
`http://127.0.0.1:8027` and coturn at
`turn:<host-lan-ip>:34790?transport=udp`, with relay UDP ports
`49200-49220`. It configures the backend with:

```dotenv
FILEBEAM_ENABLED_TRANSFER_DRIVERS=["http","webrtc"]
FILEBEAM_DEFAULT_TRANSFER_DRIVER=webrtc
FILEBEAM_WEBRTC_ICE_SERVERS=[]
FILEBEAM_WEBRTC_TURN_URLS=turn:<host-lan-ip>:34790?transport=udp
FILEBEAM_WEBRTC_TURN_SECRET=<per-run-private-secret>
FILEBEAM_WEBRTC_TURN_TTL_SECONDS=120
```

The backend generates the ephemeral coturn REST username and HMAC credential;
the Android peer must use the `ice_servers` returned by the signaling API, not
the private secret. For an emulator, configure its instance/signaling base URL
as `http://10.0.2.2:8027`; use the host LAN TURN address emitted by the run's
`environment.txt`, not `127.0.0.1`. A physical device needs a reachable LAN
binding and the same generated ICE response. Relay validation must set Android
relay-only mode and record selected `relay/relay` candidates plus both hashes.
The script removes its app, coturn, and volumes at exit; set
`KEEP_PEER_WEBRTC_ENV=1` only while an acceptance owner is attached, then run
the cleanup command in `environment.txt`.

The harness reserves these result names for verified runs. Current durable
reports are `.filebeam/final-android-build.md`,
`.filebeam/final-device-summary.md`, `.filebeam/android-final-peer-summary.md`,
and `.filebeam/receiver-fix.md`.

| Requirement | Required evidence | Current status |
| --- | --- | --- |
| Android to CLI, HTTP, both directions | source and destination SHA-256, Android instrumentation output | passed; recorded hashes |
| Android to Web, HTTP, both directions | source and destination SHA-256, browser trace/output | passed; recorded hashes |
| Android to browser, direct WebRTC, both directions | peer candidates/transport classification with redacted links, hashes | passed; direct `host/prflx` routes and hashes |
| Android to browser, TURN relay, both directions | relay-only configuration and relay candidate classification, hashes | passed; non-loopback `relay/relay` routes and hashes |
| Android to CLI, direct/TURN WebRTC, both directions | peer candidates/transport classification, hashes | passed; four small cases, both directions, direct and relay-only |
| Greater than 512 MiB and greater than 4 GiB | real fixture size, completed hashes, configured instance limits | API 35 4097 MiB send and fresh API 26 receive passed; SHA-256 `117b24fa...c9b8de6` |
| Memory observation | PSS/RSS samples, device/configuration/revision | fresh API 26 4097 MiB receive: PSS 157746 KiB, native PSS 113332 KiB; not a memory limit |
| Debug and release/R8 | APK paths, build logs, native alignment report | lint/unit, R8, all ABIs, 16 KiB alignment, App Link fixtures, and test-signed R8 launch passed |
| API 26/current, ARM64/ARMv7, physical low-memory, 4/16 KiB, release signing | device inventory and signed artifact provenance | API 26 and emulator 16 KiB evidence passed; physical ARM, low-memory, and production matrix pending |

### Local signed R8 smoke

[`docs/signed-r8.md`](signed-r8.md) defines the local test-key properties and
post-build `signedR8Smoke` command. It validates unsigned APK native alignment,
creates a separate test-signed minified APK, and verifies that signature without
claiming production signing or release provenance. `validateVerifiedAppLinks`
remains independent and requires explicit production association evidence only
when `-PappLinkAutoVerify=true`.

## Memory evidence

While an instrumentation command is running, call:

```sh
bash scripts/android/acceptance.sh --sample-pid <android-pid> --label 513m
```

It stores timestamped `VmRSS`, `VmHWM`, and `VmSize` samples in the acceptance
result directory. Record the device, configured managed-buffer budget, fixture,
transport, revision, and both runs before comparing growth. RSS is observational
evidence, not a process-memory limit.

API 35 16 KiB emulator history includes an LMKD kill at 212272 KiB RSS and a
separate `SEGV_MAPERR`; this does not establish an application heap leak.
Untested external gates are physical ARM64/ARMv7, physical low-memory behavior,
real-device OS force-stop, network and low-storage paths, manual accessibility,
and the production domain/signing matrix. Test signing is not production signing.
