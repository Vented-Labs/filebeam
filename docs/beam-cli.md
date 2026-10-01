# Beam CLI

`beam` is the Linux, macOS, and Windows CLI for Filebeam.

The Linux and macOS installer requires OpenSSL 3 or newer for release signature
verification. On macOS, install it with `brew install openssl@3`; the installer
automatically finds Homebrew's OpenSSL when the system provides LibreSSL.

Install the latest release with:

```sh
curl -fsSL 'https://releases.filebeam.io/cli/install.sh' | sh
```

On Windows, run this in PowerShell after installing Git for Windows, whose OpenSSL executable verifies the release signature:

```powershell
Invoke-RestMethod -Uri 'https://releases.filebeam.io/cli/install.ps1' | Invoke-Expression
```

Use `--dir DIRECTORY` with the shell installer or `-InstallDir DIRECTORY` with the PowerShell script block to choose a state directory. The installer verifies the signed release, installs Beam, and adds its `bin` directory to PATH.

Open a new terminal after installation to load the PATH change.

## Links and instances

`beam up` uploads local paths and `beam down` receives a shared transfer. Full URLs select their own origin, including self-hosted servers and local development. A ULID alone uses `https://filebeam.io`, or `FILEBEAM_INSTANCE`. Quote complete links literally. Passwords are never accepted as command arguments.

For example:

```sh
beam down 'https://files.company.test/01K46FN13WJVCWKMBWRMC9Q9KN'
```

## Terminal experience

Run `beam` for the full-screen Send / Receive workspace.

- `Space`: select files; `Enter`: open a folder; `Backspace`: parent folder.
- `/`: enter search mode, `Enter`: apply, `Esc`: clear.
- `Tab` / `Shift+Tab`: move focus between browser, queue, and action (or receive fields).
- `1` / `2`: Send / Receive; `u`: upload; `U`: update; `p`: native services; `?`: help.
- `p`: notes, inbox, accounts, key management, and transfer actions.
- In Send, `Enter` uploads normally; `Shift+Enter` starts Turbo Transfer.
  If the terminal cannot distinguish modified Enter, use `beam up --turbo FILE`.
- `c`: request a copy of the complete result through the terminal clipboard (OSC 52).
- `Ctrl+C`: cancel the active transfer; on an idle screen, exit. Cancellation is cooperative: an active HTTP request may need to finish or time out before stopping.

`beam up` and `beam down` display compact inline progress with transferred bytes, throughput, and ETA when there is enough terminal width and measurement history. Preparing, archiving, and verification have separate activity states. Completion is shown only after server finalization or local integrity verification.

## Notes

Hosted notes are created and opened by the native client, with the same encrypted note service used by other native clients. Input is a file or standard input, so note text is not placed in shell history.

```sh
beam note create --input incident.md --title 'Incident notes' --language markdown
printf '%s' 'one-time note' | beam note create --burn-on-read
beam note create --input secrets.txt --password --separate-key
beam note create --input standup.md --live
beam note open 'https://files.company.test/01...#k=v1....' --password
```

`--separate-key` prints the link and key as separate sensitive output lines; deliver them independently. `--burn-on-read` consumes a note only after successful decryption and integrity verification. `--password` opens a masked terminal prompt. For automation, use exactly one of `--password-file FILE` or `--password-stdin`; password files must be owner-only on Unix (`chmod 600 FILE`). Passwords are never persisted by Beam.

`--live` serves the note through the native WebRTC service. Ctrl+C explicitly ends the remote live share after the service has published its link; it is not a local discard.

## Accounts

Account sessions are scoped to the selected instance origin. After sign-in or registration, Beam stores only the opaque same-origin session cookie in encrypted, owner-only local state under `~/.filebeam/accounts`; it never writes an account password.

```sh
beam account register alice alice@example.test --name Alice
beam account login alice@example.test
beam account profile
beam account resend-verification
beam account verify 'https://files.example.test/verify-email/12/SIGNED_HASH?expires=...&signature=...'
beam account recovery-request alice@example.test
beam account recovery-reset alice@example.test RECOVERY_TOKEN
beam account key-setup
beam account key-setup --custody password
beam account key-export --acknowledge-export
beam account key-import ~/.filebeam-recovery-key --acknowledge-replace
beam account logout
```

Use the same password input rules as Notes. `key-setup --custody self` (the default) stores an owner-only local recovery key; `--custody password` encrypts the recovery key with the supplied password via the native password-key wrapper. `logout` ends the remote session and removes the local encrypted session state. Verification requires the complete, unexpired signed email link, not its public hash fragment. Recovery and custody keys use native account APIs. A local custody key is encrypted in owner-only state; export requires `--acknowledge-export`, and replacement requires `--acknowledge-replace`. Import files must be owner-only on Unix.

## Inbox and directed delivery

```sh
beam up --username alice report.pdf
beam inbox list
beam inbox download 01K... --output ./received
```

`--username` resolves exactly one authenticated recipient and encrypts the transfer key to that account key. It requires HTTP and cannot be combined with Turbo or password protection. Inbox metadata stays locked until the stored custody key opens it; downloads use the authenticated inbox endpoint and a request-scoped key.

## End and revoke

```sh
beam end-live JOB_ID
beam revoke JOB_ID
beam cancel JOB_ID
```

`end-live` remotely ends a live WebRTC share while retaining local recovery state. `revoke` remotely deletes an upload after server confirmation. `cancel` only discards local resumable state; it does not revoke a remote transfer.

## Turbo and WebRTC uploads

`beam up --turbo FILE...` publishes a share link while the HTTP upload is still
running, allowing recipients to download and verify available chunks immediately.

```sh
beam up --turbo --password --retention-hours 24 report.pdf
beam up --transport webrtc report.pdf
beam --webrtc-relay-only up --transport webrtc report.pdf
```

Turbo requires the HTTP transport. `beam up --turbo --transport webrtc FILE`
fails before a transfer starts. WebRTC remains a live peer transfer: direct
connections retain the address-exposure consent prompt unless the receiver
uses `--accept-peer-address-exposure` or `--webrtc-relay-only` is configured.

Progress goes to stderr; stdout contains the share URL or saved paths, so `link=$(beam up file.zip)` works. Use **Copy full link** in the TUI to copy the complete URL, including its key.

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
