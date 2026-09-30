# Desktop client API

`DesktopClient` is the sole service boundary for GPUI. Construct it with
`DesktopClient::new(home: Option<PathBuf>)`, read a cloned `DesktopSnapshot`
with `snapshot()`, and submit a `ClientCommand` with `dispatch()`. `dispatch`
only pushes work into a bounded native worker queue; it never performs network,
KDF, filesystem, or transfer work on the UI thread. Call
`take_opened_note(id)` only after the user explicitly requests the received
plaintext; it consumes the in-memory value and it is never part of a snapshot.

The UI owns paths selected by platform pickers and presents typed snapshots. It
must not retain passwords, share keys, cookies, account private keys, delete
capabilities, or raw protocol types. Those values are command inputs only.

Required desktop manifest dependencies:

```toml
anyhow = "1"
filebeam-client-core = { path = "../crates/client-core" }
filebeam-client-config = { path = "../crates/client-config" }
filebeam-transfer-native = { path = "../crates/transfer-native" }
zeroize = "1"
```

`desktop/src/lib.rs` should expose `pub mod client; pub mod model;`.

`SendFiles` includes `turbo` and `include_key`; `turbo` maps directly to the
native upload option and `include_key` is presentation policy for the returned
share link. Recipient sends resolve the native recipient record and use the
authenticated same-origin session cookie request-scoped. The worker uses
`ClientRuntime`, so views may disappear without cancelling jobs; `Shutdown`
uses its bounded checkpoint/pause policy.

Snapshots expose only typed metadata: configured policy/settings, account,
inbox, managed jobs, prompts, notes, export retry state, and sanitized errors.
Passwords, keys, cookies, tokens, and note plaintext do not implement `Debug`
in desktop commands/models and are not retained in snapshots.

Persistent origin-scoped session/key custody and account-key import/export/
unlock plus inbox selected-download still need the desktop platform secret-store
adapter. The current process keeps an authenticated `ServiceClient` in worker
memory only; nothing is written to TOML or plaintext files.
