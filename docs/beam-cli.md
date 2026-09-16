# Beam CLI

`beam` is the Linux, macOS, and Windows CLI for Filebeam.

Install the latest release with:

```sh
curl -fsSL 'https://releases.filebeam.io/cli/install.sh' | sh
```

On Windows, run this in PowerShell after installing Git for Windows, whose OpenSSL executable verifies the release signature:

```powershell
Invoke-RestMethod -Uri 'https://releases.filebeam.io/cli/install.ps1' | Invoke-Expression
```

Use `--dir DIRECTORY` with the shell installer or `-InstallDir DIRECTORY` when invoking the PowerShell script block directly to choose a different state directory. The installer creates `bin`, `config.toml`, and `cache`, validates the signed release catalog and selected archive, replaces the executable, and idempotently adds the selected `bin` directory to PATH. Installer scripts execute from the network stream and are not retained after installation.

Open a new terminal after installation to load the PATH change.

## Web instructions

The web application detects supported desktop operating systems and initially selects the matching command; users can always choose Linux, macOS, or Windows manually. `FILEBEAM_CLI_INSTALLER_URL` and `FILEBEAM_CLI_WINDOWS_INSTALLER_URL` configure the Unix and PowerShell HTTPS distribution URLs. Opening the instructions does not contact the release service or run the command.

The web amendment's supplied `up <url or ulid>` reference differs from the parser: `beam up <files...>` uploads local paths, while `beam down <url or ulid>` receives a shared transfer. Download commands preserve the original link's key fragment, never add a separately shared key or a password, and use interactive key/password prompts. Clipboard failure leaves the command selected for manual copying.

The web command card supports stored HTTP files, live WebRTC file links, and pending Turbo file links once their authenticated early descriptor is available. The CLI receives Turbo chunks progressively while the sender continues uploading. Notes, burn-on-read, and account-key inbox delivery remain browser-only because those APIs are not exposed through the CLI. Expired, ended, or otherwise unavailable transfers have no copyable command. URLs must have a single ULID path, no query, and an optional unencoded v1 share key, matching the CLI parser. Commands quote the complete link literally; a separate key and any password stay out of shell history and are entered only at the CLI prompt.

Full URLs select their own origin, including self-hosted servers and local development. The web download card copies the full URL directly:

```sh
beam down 'https://files.company.test/01K46FN13WJVCWKMBWRMC9Q9KN'
```

For a ULID alone, `beam down` uses `https://filebeam.io`, or the explicit `FILEBEAM_INSTANCE` override. Uploads also use the configured instance.

## Terminal experience

Run `beam` for the full-screen Send / Receive workspace. The interface uses Filebeam's violet surfaces, gradient meter, file queue, and transfer receipts. Instance information loads in the background.

- `Space`: select files; `Enter`: open a folder; `Backspace`: parent folder.
- `/`: enter search mode, `Enter`: apply, `Esc`: clear.
- `Tab` / `Shift+Tab`: move focus between browser, queue, and action (or receive fields).
- `1` / `2`: Send / Receive; `u`: upload; `U`: update; `?`: help.
- In the Send action, `Enter` sends the normal encrypted HTTP transfer and
  `Shift+Enter` starts Turbo Transfer. The two actions are displayed side by
  side. Beam requests enhanced keyboard reporting only while its full-screen
  UI is active and restores the terminal setting on exit. Older terminals that
  do not report modified Enter use normal `Enter`; they cannot select Turbo by
  hotkey, so use `beam up --turbo FILE`.
- `c`: request a copy of the complete result through the terminal clipboard (OSC 52).
- `Ctrl+C`: cancel the active transfer; on an idle screen, exit. Cancellation is cooperative: an active HTTP request may need to finish or time out before stopping.

`beam up` and `beam down` display compact inline progress with transferred bytes, throughput, and ETA when there is enough terminal width and measurement history. Preparing, archiving, and verification have separate activity states. Completion is shown only after server finalization or local integrity verification.

## Turbo and WebRTC uploads

`beam up --turbo FILE...` creates a Turbo HTTP upload. It publishes the normal
encrypted share link as soon as the authenticated early descriptor is ready,
then recipients can progressively download and verify available chunks while
the upload continues. `--turbo` is explicit and does not add a confirmation
prompt. Password, retention, directory-mode, and security prompts retain their
normal behavior.

```sh
beam up --turbo --password --retention-hours 24 report.pdf
beam up --transport webrtc report.pdf
beam --webrtc-relay-only up --transport webrtc report.pdf
```

Turbo requires the HTTP transport. `beam up --turbo --transport webrtc FILE`
fails before a transfer starts. WebRTC remains a live peer transfer: direct
connections retain the address-exposure consent prompt unless the receiver
uses `--accept-peer-address-exposure` or `--webrtc-relay-only` is configured.

Progress goes to stderr. Stdout contains the share URL or saved file paths, so `link=$(beam up file.zip)` works. Interactive share links carry an explicit OSC 8 target including the full key fragment; wrapping or a shortened TUI label does not shorten that target. Use the TUI's **Copy full link** action for a complete clipboard value.

Use `--plain` for output without terminal control sequences. Redirected stderr automatically receives concise text. `--no-color` / `NO_COLOR` disable colors, and `--reduced-motion` disables decorative animation and interpolation. These preferences can also be set in `config.toml`:

```toml
no_color = false
reduced_motion = false
check_updates = true
```

## Limits And Resume State

`--memory-limit-mib` and `--max-concurrency` are global options, including when resuming. The memory limit is a budget for managed transfer buffers, not a process-RSS limit: allocator overhead, executable code, OS page cache, and unrelated allocations are outside it. The default buffer budget is 512 MiB; accepted values are 64 through 4096 MiB. Concurrent requests are capped at 64 and are further limited by the server and available managed buffer budget.

```sh
beam --memory-limit-mib 256 --max-concurrency 2 up report.pdf
beam --memory-limit-mib 256 --max-concurrency 2 resume JOB_ID
```

Resume state is private to the local user in `~/.filebeam/transfers` (or the directory selected with `--home` / `FILEBEAM_HOME`). It can include encrypted transfer metadata, credentials, and immutable ciphertext needed to continue; it is not portable state and must not be copied, logged, or shared.

```sh
beam transfers
beam resume JOB_ID
beam cancel JOB_ID
```

`beam transfers` prints an ID, direction, state, and completed/total bytes. `Ctrl+C` retains a running job for `beam resume`; `beam cancel` deliberately discards that job and all of its local resume material. The Transfers screen follows the same rule: `Enter` resumes and `x` discards.

## Sending directories

```sh
beam up ./photos                # Choose ZIP and send or Individual files
beam up ./photos --zip           # One encrypted ZIP, counted as one file
beam up ./photos --individual    # Each discovered file counts against the limit
```

The prompt shows the discovered file count and the instance's file limit. Plain/non-interactive directory uploads require an explicit mode. `--zip` and `--individual` are mutually exclusive.

ZIP mode combines all supplied paths into one archive, preserving nested folders (including empty folders). A single directory produces `directory-name.zip`; multiple paths produce `filebeam-transfer.zip`. The private archive is retained with the local job while it is needed for resume, then removed when the transfer finishes or the job is explicitly discarded.

Individual mode recursively sends regular files into the recipient's chosen destination folder. Duplicate basenames receive deterministic suffixes such as `readme (2).txt`. Directory traversal includes hidden files and skips nested symbolic links. The file-count limit is checked before reserving a transfer; the normal encrypted-byte limit also applies.

## Receipts And Expiry

A completed private-transfer receipt includes the complete share link, including its secret fragment. Treat the receipt as sensitive: anyone with the complete link can use the corresponding share capability.

The server initially retains a pending upload for two hours. Each accepted pending Turbo chunk refreshes that pending lifetime to two hours from the latest accepted chunk, but never beyond 24 hours from transfer creation. Adaptive server stages have a separate one-hour default TTL. A local resume therefore remains subject to server-side stage and pending-transfer expiry; resume may need to retransmit a missing chunk and cannot revive an expired transfer.
