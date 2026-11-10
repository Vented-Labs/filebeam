# Daemon

`filebeamd` runs device-to-device jobs for one Filebeam profile. The CLI and the
desktop app talk to it over an authenticated local socket; they can queue,
pause, resume, or cancel jobs, but the daemon owns the transfer workers, so
closing a client does not stop a job. One daemon runs per profile, and a second
instance refuses to start. See [Device-to-device transfers](devices.md) for the
user workflow.

## Installation

The daemon is a separate signed component published in the CLI release catalog
for Linux and macOS. `beam service enable` and the desktop app's **Install
device service** download it, verify the catalog signature and archive digest,
and register a per-user service: a systemd user unit on Linux and a LaunchAgent
on macOS. The service runs under your account and never needs sudo. The CLI
and the desktop app share one component and one service per profile.

Setup waits up to 20 seconds for the daemon to answer over its socket. Upgrades
drain the running daemon before switching the launcher and restore the previous
launcher if the new one fails this check; a failed first installation removes
its service registration. Running `beam service enable` again repairs a stopped
or failing service. Removing one client keeps the service while the other still
uses it; received files and profile data are always kept.

`beam service status` reports whether the service is running and exits with
status 3 when it is not. On Linux, `journalctl --user -u filebeam-daemon -n 50`
shows recent daemon output. Profiles other than `~/.filebeam` use a suffixed
unit name, which Beam prints when the service fails.

For development, run the daemon directly:

```sh
cargo run --manifest-path crates/daemon/Cargo.toml -- --home ~/.filebeam
```

## Jobs

The queue holds at most 32 unfinished jobs. Each job is prepared first: the
daemon encrypts a snapshot of the selected files into the profile's checkpoint
store, after which the original files are no longer needed. If the daemon stops
during preparation, the next start discards the partial snapshot and prepares
again with fresh key and nonce material. A prepared job survives restarts.

A job that cannot reach its receiver is retried with a per-device delay that
grows to five minutes, so an offline device never delays jobs for other
devices. Jobs fail when their seven-day delivery window ends. The last error is
stored with the job and shown by `beam jobs list`. Errors are also written to
standard error, which systemd records in the user journal.

## Receiving

Receiving is disabled until `beam receive enable`. Incoming files are written
only into destination grants created with `beam destinations add`. The daemon
holds a directory handle for each grant and uses descriptor-relative, no-follow
operations for staging, so a renamed or replaced path cannot redirect writes.
Each received root is published with an exclusive rename (`renameat2` on Linux,
`renameatx_np` on macOS) that never replaces an existing name. A commit journal
records each publication, so after a crash the daemon recognizes what it already
published and answers a retried delivery with the same signed receipt.

## Instance certificates

The daemon trusts the system certificate store. To use an instance signed by a
private or development certificate authority, set `FILEBEAM_INSTANCE_CA` to a
PEM file for both the daemon and the CLI. It adds a trusted root; hostname and
chain verification stay enabled.
