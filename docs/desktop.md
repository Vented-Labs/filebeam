# Desktop Development

## Install from the web app

Open **Install Desktop App** in the web header to choose a platform and processor.
The dialog detects the operating system and, where browser client hints permit,
the processor architecture. Both selections can be changed manually. The install
entry is hidden on mobile devices and at viewport widths of 900px or less.

Downloads come from the newest stable GitHub release containing native desktop
installers: AppImage for Linux x86_64/aarch64, DMG for Intel/Apple Silicon macOS,
and the setup executable for Windows x86_64. The dialog shows availability when
an installer has not been published. Release metadata is cached for five minutes;
failed lookups are retried after a 30-second cache period.

## Build locally

The desktop client uses Rust and GPUI Kit. Run local Cargo commands through the
Docker runner from the repository root:

```sh
bash scripts/desktop/check.sh
bash scripts/desktop/build.sh
bash scripts/desktop/dev.sh
bash scripts/desktop/run.sh cargo test --manifest-path desktop/Cargo.toml --locked
```

The runner limits local builds to 4 GiB of memory, two CPUs, and one Cargo job.
CI builds use their allocated runner resources without Docker wrapper limits. The runner caches
builds under `${XDG_CACHE_HOME:-$HOME/.cache}/filebeam/desktop/<worktree-hash>`;
`FILEBEAM_DESKTOP_CACHE_DIR` overrides this location. Binaries are in its `target/`
directory (`/target` inside the container). Set `FILEBEAM_DESKTOP_BUILD_IMAGE=true`
to rebuild the tooling image.

## Run with local settings

The runner uses an isolated home by default. To launch on Wayland with an existing
Filebeam home owned by the current user:

```sh
FILEBEAM_DESKTOP_HOME="$HOME/.filebeam" \
FILEBEAM_DESKTOP_WAYLAND=1 \
FILEBEAM_DESKTOP_DRI=1 \
bash scripts/desktop/run.sh cargo run --manifest-path desktop/Cargo.toml --bin filebeam -- --home /filebeam-home
```

This grants the container access to the selected settings directory, compositor,
and GPU. The process holds the build-runner lock until it exits.

## Assets

`desktop/assets/filebeam-mark.svg` is the source for platform icons. Generating
them requires ImageMagick and `icotool`:

```sh
bash scripts/desktop/generate-icons.sh
node scripts/desktop/generate-assets.mjs
```

Platform icons are written to `desktop/packaging/icons/`. The native asset registry
uses `icons/approved.json`; its palette is checked against `desktop/assets/colors.json`.

To install a per-user Linux development launcher:

```sh
bash scripts/desktop/install-dev.sh
gtk-launch io.filebeam.desktop
```

The installer refuses to overwrite a non-development launcher.

## UI fixtures

The `visual-test` feature enables synthetic UI scenarios without starting transfer
workers or reading user settings:

```sh
bash scripts/desktop/run.sh cargo run --manifest-path desktop/Cargo.toml --example visual_studio --features visual-test -- transfers-mixed
```

## CI and releases

Desktop PR checks build optimized binaries and unsigned installers for Linux
x86_64/aarch64, macOS x86_64/aarch64, and Windows x86_64. Linux also runs the
headless transfer E2E suite; native runners test the shared configuration and
updater contracts. Production profiles use level-3 optimization, thin LTO, one
codegen unit, and stripped symbols, with debug information and assertions disabled.
Shared-crate tests use the desktop dependency graph, and native builds reuse the
host target directory. Cargo caches are saved after failed checks as well as
successful runs so a later job can reuse compiled dependencies.

The `vX.Y.Z` release workflow publishes Desktop and CLI alongside the server.
Signed desktop releases require these GitHub `release` environment secrets:

- `RELEASE_SIGNING_KEY` and `RELEASE_PUBLIC_KEY`: matching Ed25519 catalog keys.
- `APPLE_CERTIFICATE_BASE64`, `APPLE_CERTIFICATE_PASSWORD`, and
  `APPLE_SIGNING_IDENTITY`: exported Developer ID Application certificate and key.
- `APPLE_NOTARY_KEY_BASE64`, `APPLE_NOTARY_KEY_ID`, and `APPLE_NOTARY_ISSUER`:
  App Store Connect API key for notarization.
- `WINDOWS_CERTIFICATE_BASE64` and `WINDOWS_SIGNING_PASSWORD`: exported Authenticode
  signing certificate and key.

Certificate and API key files are Base64-encoded. CI imports them into temporary
storage and removes them after packaging. It notarizes the macOS app before
archiving it for updates, and signs both the Windows executable and installer.
Linux update archives contain the complete AppImage, including bundled libraries.
Both clients share `~/.filebeam/config.toml`, while their managed executables,
update state, and backups remain product-specific.
Portable Linux AppImages update in place, and macOS updates replace the running
application bundle. The installation location must be writable by the user.
`--home` selects an explicit shared directory. The legacy `FILEBEAM_HOME` override
retains its original meaning: a parent directory containing `.filebeam`. Flat CLI
settings migrate in place, preserving private state and the update opt-out; a
legacy `FILEBEAM_INSTANCE` is imported only before the configuration is migrated.
