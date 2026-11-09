# Daemon Jobs

The daemon is the sole owner of device-to-device jobs. IPC observers may pause,
resume, or cancel a job, but never own its transfer worker. Active preparation
and transfer controls are retained by the daemon and cancellation is forwarded
to the native worker before the persisted job state changes.

On startup, an incomplete preparation is queued for a new preparation pass.
That pass creates fresh transfer key and nonce material. A prepared transfer is
queued using its immutable encrypted artifacts, so it can continue after the
original source has been removed. Cancelled jobs are not resumed.

The pending queue admits at most 32 non-terminal jobs. Completed checkpoints
remain available for receipt verification; they are not discarded by job
recovery.

On Unix, accepted peer receives retain a private staging directory descriptor.
The native receive worker uses descriptor-relative, no-follow operations for
ciphertext, plaintext, and staged output; it does not expose a `/proc`-derived
path. Linux uses `renameat2` and macOS uses `renameatx_np` with exclusive
rename semantics when publishing a selected root.

On Windows, the daemon uses a profile-specific named pipe with user and logon
identity checks plus authenticated request framing. Private profile directories
are checked for unsafe ACLs and reparse points before credentials are opened.
Destination grants retain volume/file identities and directory handles;
publication uses handle-relative, no-replace operations. Stored commit records
bind the requested alias to the accepted grant so receipt replay does not depend
on a later alias lookup.

CLI and desktop installations share a signed daemon component and per-profile
service identity. Component upgrades drain the running owner before switching
the stable launcher, and restore the previous launcher if activation fails.
Removing one client retains the service while another client owns it. Received
files and profile data are retained when service ownership is removed.
