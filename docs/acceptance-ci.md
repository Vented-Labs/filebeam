# Android acceptance CI

This inventory separates repeatable build coverage from release acceptance. A
passing loopback, binding, or APK check is not Android-to-peer interoperability
evidence.

## Fast CI

`bash scripts/android/check.sh` is the required Android build gate. It runs Rust
formatting, Clippy with warnings denied, Rust tests, Android lint/unit tests,
debug and unsigned release (R8) APK builds, and native ELF/APK 16 KiB alignment
validation. `bash scripts/android/device-test.sh` boots the pinned API 35 16 KiB
emulator and runs installed-ABI instrumentation. The standard workflow retains
the debug APK and reports; it deliberately does not run large fixtures or depend
on a public TURN service.

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

The harness reserves these result names for verified runs:

| Requirement | Required evidence | Current status |
| --- | --- | --- |
| Android to CLI, HTTP, both directions | source and destination SHA-256, Android instrumentation output | pending peer harness integration |
| Android to Web, HTTP, both directions | source and destination SHA-256, browser trace/output | pending peer harness integration |
| Android to CLI/Web, direct WebRTC, both directions | peer candidates/transport classification with redacted links, hashes | pending; self-test is not sufficient |
| Android to CLI/Web, TURN relay, both directions | relay-only configuration and relay candidate classification, hashes | pending; no shared TURN fixture in CI |
| Greater than 512 MiB and greater than 4 GiB | real fixture size, completed hashes, configured instance limits | fixtures available; transfers pending |
| Flat memory growth | repeated 513 MiB/4097 MiB run RSS samples and configuration/revision | harness sampling available; measurements pending |
| Debug and release/R8 | APK paths, build logs, native alignment report | covered by `check.sh`; release remains unsigned |
| API 26/current, ARM64/ARMv7, physical low-memory, 4/16 KiB, release signing | device inventory and signed artifact provenance | pending hardware/signing availability |

## Memory evidence

While an instrumentation command is running, call:

```sh
bash scripts/android/acceptance.sh --sample-pid <android-pid> --label 513m
```

It stores timestamped `VmRSS`, `VmHWM`, and `VmSize` samples in the acceptance
result directory. Record the device, configured managed-buffer budget, fixture,
transport, revision, and both runs before comparing growth. RSS is observational
evidence, not a process-memory limit.

No test is marked complete here until it has the specified peer and hash evidence.
