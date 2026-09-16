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

## Links and instances

`beam up` uploads local paths and `beam down` receives a shared transfer. Full URLs select their own origin, including self-hosted servers and local development. A ULID alone uses `https://filebeam.io`, or `FILEBEAM_INSTANCE`. Quote complete links literally. Passwords are never accepted as command arguments.

For example:

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

## Notes

Hosted notes are created and opened by the native client, with the same encrypted note service used by other native clients. Input is a file or standard input, so note text is not placed in shell history.

```sh
beam note create --input incident.md --title 'Incident notes' --language markdown
printf '%s' 'one-time note' | beam note create --burn-on-read
beam note create --input secrets.txt --password --separate-key
beam note open 'https://files.company.test/01...#k=v1....' --password
```

`--separate-key` prints the link and key as separate sensitive output lines; deliver them independently. `--burn-on-read` consumes a note only after successful decryption and integrity verification. `--password` opens a masked terminal prompt. For automation, use exactly one of `--password-file FILE` or `--password-stdin`; password files must be owner-only on Unix (`chmod 600 FILE`). Passwords are never persisted by Beam.

## Accounts

Account sessions are scoped to the selected instance origin. After sign-in or registration, Beam stores only the opaque same-origin session cookie in encrypted, owner-only local state under `~/.filebeam/accounts`; it never writes an account password.

```sh
beam account register alice alice@example.test --name Alice
beam account login alice@example.test
beam account profile
beam account resend-verification
beam account recovery-request alice@example.test
beam account recovery-reset alice@example.test RECOVERY_TOKEN
beam account logout
```

Use the same password input rules as Notes. `logout` ends the remote session and removes the local encrypted session state. Verification and recovery use native account APIs. Custody-key setup/import/export/replacement, Inbox unlock/download, and username-directed delivery are not yet available in Beam; Beam does not open a browser as a substitute.

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
