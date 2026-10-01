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
