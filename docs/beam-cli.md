# Beam CLI

`beam` is the Linux CLI for Filebeam. Merging to `master` and pushing an application release tag such as `v0.2.0` runs the application release workflow and its reusable Beam pipeline. That pipeline builds and signs the CLI artifacts, publishes the installer/catalog to R2, smoke-tests the public installer, and creates the matching `beam-v0.2.0` GitHub release automatically. No separate CLI tag or manual Cargo version edit is needed. Standalone `beam-vX.Y.Z` tags from master remain supported.

Install the latest release with:

```sh
curl -fsSL 'https://releases.filebeam.io/cli/install.sh' -o beam-install.sh && sh beam-install.sh
```

Use `--dir DIRECTORY` to choose a different state directory. The installer creates `bin`, `config.toml`, and `cache`, validates the signed release catalog and selected archive, replaces `bin/beam` atomically, and idempotently adds the selected `bin` directory to bash, zsh, and fish startup files.

Open a new terminal after installation to load the PATH change.

## Web instructions

The web application defaults `FILEBEAM_CLI_INSTALLER_URL` to `https://releases.filebeam.io/cli/install.sh`; self-hosted installations may override this with another verified HTTPS distribution URL. The installer uses POSIX `sh` and installs the `beam` executable. Opening the instructions does not contact the release service or run the command.

The web amendment's supplied `up <url or ulid>` reference differs from the parser: `beam up <files...>` uploads local paths, while `beam down <url or ulid>` receives a shared transfer. Download commands preserve the original link's key fragment, never add a separately shared key or a password, and use interactive key/password prompts. Clipboard failure leaves the command selected for manual copying.

The current CLI supports stored HTTP files. Notes, burn-on-read, account-key inbox delivery, and WebRTC are not supported by the web command card. Pending Turbo transfers become eligible after completion as ordinary stored HTTP files. Expired or unavailable transfers have no copyable command. URLs must have a single ULID path, no query, and an optional unencoded v1 share key, matching the CLI parser.

Full URLs select their own origin, including self-hosted servers and local development. The web download card copies the full URL directly:

```sh
beam down 'https://files.company.test/01K46FN13WJVCWKMBWRMC9Q9KN'
```

For a ULID alone, `beam down` uses `https://filebeam.io`, or the explicit `FILEBEAM_INSTANCE` override. Uploads also use the configured instance.

Installer publication and signed-binary smoke testing are separate from local CLI builds; a successful web or fixture test does not certify published artifacts.

Local Rust commands are Docker-only and default to Rust 1.98.0, 2 GB memory and memory-swap, 2 CPUs, and one Cargo build job:

```sh
scripts/cli/check.sh
scripts/cli/test.sh
scripts/cli/build.sh x86_64
BEAM_RELEASE_PUBLIC_KEY=BASE64_ED25519_PUBLIC_KEY scripts/cli/package.sh beam-v1.2.3 dist/beam
```

Set `BEAM_DOCKER_MEMORY`, `BEAM_DOCKER_MEMORY_SWAP`, `BEAM_DOCKER_CPUS`, or `CARGO_BUILD_JOBS` to change those limits. `BEAM_DOCKER_BUILD=false` reuses an already-built tooling image.

## Terminal experience

Run `beam` for the full-screen Send / Receive workspace. The interface uses Filebeam's violet surfaces, gradient meter, file queue, and transfer receipts. Instance information loads in the background.

- `Space`: select files; `Enter`: open a folder; `Backspace`: parent folder.
- `/`: enter search mode, `Enter`: apply, `Esc`: clear.
- `Tab` / `Shift+Tab`: move focus between browser, queue, and action (or receive fields).
- `1` / `2`: Send / Receive; `u`: upload; `U`: update; `?`: help.
- `c`: request a copy of the complete result through the terminal clipboard (OSC 52).
- `Ctrl+C`: cancel the active transfer; on an idle screen, exit. Cancellation is cooperative: an active HTTP request may need to finish or time out before stopping.

`beam up` and `beam down` display compact inline progress with transferred bytes, throughput, and ETA when there is enough terminal width and measurement history. Preparing, archiving, and verification have separate activity states. Completion is shown only after server finalization or local integrity verification.

Progress goes to stderr. Stdout contains the share URL or saved file paths, so `link=$(beam up file.zip)` works. Interactive share links carry an explicit OSC 8 target including the full key fragment; wrapping or a shortened TUI label does not shorten that target. Use the TUI's **Copy full link** action for a complete clipboard value.

Use `--plain` for output without terminal control sequences. Redirected stderr automatically receives concise text. `--no-color` / `NO_COLOR` disable colors, and `--reduced-motion` disables decorative animation and interpolation. These preferences can also be set in `config.toml`:

```toml
no_color = false
reduced_motion = false
check_updates = true
```

## Sending directories

```sh
beam up ./photos                # Choose ZIP and send or Individual files
beam up ./photos --zip           # One encrypted ZIP, counted as one file
beam up ./photos --individual    # Each discovered file counts against the limit
```

The prompt shows the discovered file count and the instance's file limit. Plain/non-interactive directory uploads require an explicit mode. `--zip` and `--individual` are mutually exclusive.

ZIP mode combines all supplied paths into one archive, preserving nested folders (including empty folders). A single directory produces `directory-name.zip`; multiple paths produce `filebeam-transfer.zip`. The temporary ZIP is removed when the worker finishes or is cancelled.

Individual mode recursively sends regular files into the recipient's chosen destination folder. Duplicate basenames receive deterministic suffixes such as `readme (2).txt`. Directory traversal includes hidden files and skips nested symbolic links. The file-count limit is checked before reserving a transfer; the normal encrypted-byte limit also applies.

## Verification

```sh
scripts/cli/check.sh
scripts/cli/test.sh
scripts/cli/build.sh
bash scripts/cli/terminal.test.sh
```

The PTY suite runs in capped Docker and exercises slow single-chunk transfers, output redirection, resizing, key prompts, cancellation, full-screen input, directory modes, ZIP contents, and complete hyperlink/copy targets. Visual fixtures can be exported with `BEAM_VISUAL_DIR=/workspace/cli/target/visual` inside `scripts/cli/run.sh` when running the `export_visual_fixtures` Rust test.

## Local installation

To install a Docker-built development binary that targets the running Sail instance on port 8000:

```sh
scripts/cli/install-local.sh
export PATH="$HOME/.filebeam/bin:$PATH"
beam
```

Use `--instance http://localhost:PORT` or `--dir DIRECTORY` to override either local wrapper default. The CLI binary defaults to `https://filebeam.io`. `FILEBEAM_INSTANCE` overrides the compiled default; the local installer sets this explicitly for the development server.

Publishing requires R2 and Ed25519 release credentials, then writes immutable artifacts below `cli/versions/vX.Y.Z/`, a signed `cli/index.json`, and the mutable no-cache `cli/install.sh`:

```sh
scripts/release/cli-publish.sh beam-v1.2.3 dist/beam
```

The GitHub `release` environment supplies `R2_ENDPOINT_URL`, `R2_BUCKET`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, and `RELEASE_SIGNING_KEY`; `RELEASE_PUBLIC_KEY` is shared with the package build. The R2 bucket must be reachable at `https://releases.filebeam.io`. Both the CLI executable and installer embed the verification key. Production publishing only accepts a source tag reachable from `origin/master`. The `beam-v…` GitHub release is marked non-latest so it does not replace the main application release.

`BEAM_RELEASE_VERSION` is supplied by packaging from the release tag. It controls `beam --version`, the HTTP User-Agent, and the CLI updater's current-version comparison; development builds use the Cargo package version. `scripts/cli/smoke-package.sh` verifies both architecture archives and executes the native package. `scripts/cli/smoke-install.sh` installs from the public signed catalog into a disposable HOME and checks the installed version and production instance default. Neither smoke test writes to a user's normal shell configuration.
