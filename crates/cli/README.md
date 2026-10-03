# Beam CLI

`beam` sends and receives Filebeam files over stored HTTP by default. Start a
Turbo HTTP transfer, which publishes the encrypted share so recipients can
progressively receive verified chunks while the sender is still uploading, with:

```sh
beam up --turbo report.pdf
```

The download link appears as soon as the server accepts the encrypted transfer
descriptor, while the upload continues. Keep Beam running until uploading
finishes. In the full-screen interface, press `c` to copy the link during upload.
With `beam up --turbo`, the link is printed once to stdout, including when stdout
is redirected. Resuming a Turbo upload makes the same link available again.

In the full-screen interface, use `1`, `2`, and `3` to switch between Send,
Receive, and Transfers. When editing a field, press `Esc` first to return to
page shortcuts.

### Account history

After `beam account login EMAIL`, new file and note shares use the signed-in
account, including link and live shares. An expired saved session requires a new
login; Beam does not silently switch to anonymous sharing.

```sh
beam history list
beam history list --kind files --status available --driver http
beam history list --cursor CURSOR
beam history extend TRANSFER_ID --retention-hours 48
beam history delete TRANSFER_ID
```

History lists outgoing account-owned transfer IDs, type, transport, state, item
count, encrypted bytes, and expiry. `--limit` accepts 1–100 entries (default 25).
Filters must be repeated when requesting another page. Extension sets total
retention from completion/publication, capped by the current account plan and,
for WebRTC, the instance live-session limit. It cannot revive expired or ended
shares. Deletion removes access for everyone; minimal summaries remain for 90
days after cleanup.

In the full-screen interface, press `p` for History list/delete/extend actions.
`beam transfers` remains the local resume-job list; account history works across
devices and does not recover filenames or share-link keys.

### Paste on Send

Paste text with your terminal's paste shortcut to open the Notes composer. Press
`Esc` to leave editing, then `n` to switch between Files and Notes, preserving
both drafts. In Notes, `e` edits the selected field, `Shift+Tab` changes fields,
and `Enter` inserts a newline in the note body. Press `Esc`, then `Enter` to share.
Pasted notes preserve whitespace and Unicode, up to 64 KiB.

From Send's page shortcuts, press `v` to read the local system clipboard. Images
are added to the file queue as PNG; text opens Notes. This requires a desktop
clipboard on Windows, macOS, or Linux (X11 or a compositor supporting Wayland
data-control). Over SSH, it accesses the remote machine's clipboard; terminal
text paste still works without desktop clipboard access.

Clipboard images are limited to 128 MiB of decoded pixels. Their private temporary
sources are removed when dequeued or when Beam exits normally. Uploads retain a
private source copy with their recovery state, so they remain resumable; removing
the saved transfer removes that copy. Paste never starts a transfer automatically.

Turbo is HTTP-only and cannot be combined with `--transport webrtc`. Use native
live WebRTC for a file send with:

```sh
beam up --transport webrtc report.pdf
```

After the sender has encrypted its selection into a private local ciphertext
spool and published the share, it writes the link to stdout immediately and
continues serving it. Redirecting stdout therefore captures the link without
stopping the sender. `Ctrl+C` stops the live sender and preserves its local job;
`beam transfers` shows the job and `beam resume JOB_ID` serves the same share
again until it expires. `beam cancel JOB_ID` removes the local resume state.

Native WebRTC downloads expose the receiver's network address to the sender
unless the receiver chooses relay-only mode. Interactive users must approve the
prompt; non-interactive callers must pass `--accept-peer-address-exposure`.
That flag consents only to address exposure. `--webrtc-relay-only` requires a
TURN UDP relay instead and bypasses the direct-peer prompt. The native
`webrtc` crate 0.20.5 does not support TURN TCP or TLS, so relay-only must not
be described as full TURN support or as a security guarantee.

The spool and resume records are private filesystem state, not encrypted at
rest. They can contain ciphertext, transfer credentials, source metadata, and
share links. The CLI applies its `--memory-limit-mib` budget to managed
transfer buffers; this is not a process-RSS limit. Ciphertext artifacts and
WebRTC framing are size-bounded.

Native WebRTC supports file links only. Notes, burn-on-read notes, and
account-key inbox delivery are unsupported. In particular, the native download
path requires ordinary manifest items and cannot currently turn an itemless
note payload into a synthetic item.
