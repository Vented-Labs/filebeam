# Filebeam for Android

Native Kotlin / Jetpack Compose / Material 3, Android 8 (API 26) and newer.
Encryption and HTTP/WebRTC transfers execute in the shared Rust engine through
UniFFI. There is no WebView or JavaScript application runtime.

## Build

From the repository root on a Linux Docker host:

```sh
bash scripts/android/run.sh ./gradlew :app:assembleDebug
bash scripts/android/check.sh
```

The first command builds the pinned tooling image. Subsequent runs reuse Docker,
Cargo, and Gradle caches. `FILEBEAM_ANDROID_BUILD_IMAGE=false` skips rebuilding the
tooling image. Local containers default to 6 GiB RAM, four CPUs, and two Cargo
jobs; `FILEBEAM_ANDROID_MEMORY`, `FILEBEAM_ANDROID_CPUS`, and `CARGO_BUILD_JOBS`
override these. CI uses its available CPUs. `FILEBEAM_ANDROID_CACHE` overrides
the default `~/.cache/filebeam/android` cache directory.

Output: `mobile/android/app/build/outputs/apk/debug/app-debug.apk`.
Debug application ID: `io.filebeam.android.debug`; release ID:
`io.filebeam.android`. Release signing is supplied by the eventual distribution
pipeline; the check command builds an unsigned release APK and exercises R8.

For a faster emulator build:

```sh
bash scripts/android/run.sh ./gradlew :app:assembleDebug :app:assembleDebugAndroidTest \
  -PrustAbis=x86_64 -PrustProfile=debug
```

Default builds include `arm64-v8a`, `armeabi-v7a`, and `x86_64` with optimized
Rust. Cargo builds and Kotlin generation are Gradle dependencies, so an APK
cannot silently substitute a mock or omit the Rust engine. Generated Kotlin and
native binaries live under `core-rust/build/` and are not checked in.

### Android Studio

Open this directory. Install JDK 21, SDK platform 37.0, build-tools 36.0.0, NDK
28.2.13676358, Rust 1.98.0, and cargo-ndk 4.1.2. Set `ANDROID_HOME` (and
`ANDROID_NDK_HOME` if using a different SDK location) in the IDE build environment.
Install the Rust targets:

```sh
rustup target add --toolchain 1.98.0 aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo +1.98.0 install cargo-ndk --version 4.1.2 --locked
```

Use Rust 1.98.0 for the IDE's Cargo invocations. The Gradle wrapper pins 9.7.1;
the version catalog pins AGP 9.4.0, Kotlin Compose compiler 2.4.20, and Compose
BOM 2026.09.00. AGP supplies built-in Kotlin support.

## Device checks

Build both the debug and instrumentation APKs, then run the disposable Linux
emulator harness:

```sh
docker build -t filebeam-android-tooling:rust-1.98.0-sdk37 docker/android
docker build -f docker/android/Emulator.Dockerfile -t filebeam-android-emulator:api35-16k docker/android
bash scripts/android/device-test.sh
```

The harness uses KVM when available, checks the 16-KiB page size, installs the
APKs, and executes real Rust encryption, native WebRTC UDP/ICE/DTLS/framing, and
binding-lifetime tests. Reports are written under `test-results/android/`.
For Android 8 coverage, build a second emulator image with
`--build-arg 'SYSTEM_IMAGE=system-images;android-26;google_apis;x86_64'` and select
it using `FILEBEAM_ANDROID_EMULATOR_IMAGE`.

The manual `Android acceptance` workflow exposes the same API 26 4 KiB and API
35 16 KiB image choices. Its 513 MiB and 4097 MiB fixtures are an explicit
workflow input, never part of normal pull-request coverage. See
[`docs/acceptance-ci.md`](../../docs/acceptance-ci.md) for its evidence limits
and the local test-signed R8 smoke procedure; that key is not production signing.

`check.sh` also checks APK ZIP alignment and native ELF load/RELRO layout for
all packaged 64-bit libraries, including transitive AndroidX and JNA libraries.

## Current scaffold

- Native Send, Receive, Transfers, and Settings screens; responsive navigation,
  system light/dark themes, and wallpaper colors on Android 12+.
- Document picker, incoming share intents, link input, native share sheet, ZIP
  preparation, and saving verified downloads through document providers.
- HTTP and WebRTC file jobs, progress, secret/peer-consent prompts, pause, local
  recovery catalog, resume, and local-state removal through the shared engine.
- Turbo HTTP file uploads publish their encrypted early descriptor and share link
  before chunk production; native receivers progressively consume those links.
- User-initiated transfer jobs on Android 14+ for finite transfers; foreground
  services for older versions and live senders. Export also runs in a foreground
  service. Foreground-service timeouts pause work rather than keeping a hidden
  sender alive indefinitely.
- Incoming link fragments in pending Android requests are protected by Android
  Keystore. Recovery data and source snapshots live in `noBackupFilesDir`; cloud
  and device-transfer backups are disabled.

The native checkpoints remain in the existing filesystem-protected format;
Keystore wrapping of all native checkpoint secrets is a remaining milestone.
The app currently runs one transfer at a time with a 128 MiB **managed buffer**
budget and up to two requests. This is not a bound on total process memory.

### Large files and storage

The browser's 512 MiB fallback limit is not applied. File selection is streamed
into durable, seekable app-private snapshots with a 64 KiB buffer; file bytes
never traverse UniFFI. This supports non-seekable and cloud-backed providers and
keeps an upload's source stable across process restarts. Original file bytes are
preserved. Snapshots are removed after a completed upload or local job removal.

The tradeoff is disk usage. WebRTC senders retain the complete encrypted transfer
while live; downloads retain ciphertext and verified plaintext until publication.
Peak native download space can approach three times an item's size. Verified
downloads remain available locally until their recovery entry is removed.
Source snapshots, native artifacts, and the user's exported destination must all
fit on the relevant volumes. Server/plan limits still apply, including AEAD tag
overhead; `/api/v1/info` advertises independent HTTP/WebRTC limits.

Pause retains recovery material. **Remove local state** removes local data and
does not revoke the remote share. A WebRTC sender must be resumed before peers
can continue; server expiry still applies. Process death during an initial
import from a transient share grant can require selecting the source again.
Export failures retain the verified private file for another save attempt.

## Shared ownership

```text
Kotlin UI + Android storage/background adapters
                 ↓ generated UniFFI bindings
client-ffi → client-core → transfer-native → transfer + encryption
                 ↑
                CLI
```

`client-core` owns worker lifetime, cancellation, stable snapshots, and pending
prompts. Its channels are also used by the CLI. `client-ffi` translates a small
control API and generates Kotlin/Swift bindings; it exposes no chunk byte arrays
or networking objects. Transfer/checkpoint versions remain separate from the FFI
interface. Bindings and the native library are generated from the same checkout.

## Next milestones

See [the Android roadmap](../../docs/android.md) for the remaining work before a
 feature-parity release: source descriptors, a unified runtime/scheduler, full
 checkpoint-secret wrapping, publication recovery, send passwords/retention,
 remote deletion and live-end controls, notes, account/inbox APIs, and large-file
 interoperability/device validation. The available test harness builds debug and
 unsigned release/R8 APKs; it does not claim release signing or store publication.
