# Contacts and receiving

Contacts are mutual friends stored on the selected Filebeam instance. Add someone
by their exact `@username`; they must accept the request before either account
treats the relationship as a friendship. Contact lists are private. Usernames on
different instances identify different accounts.

Open **Contacts** on the web or the account's contacts section in a native client
to manage requests, friends, blocked accounts, and receiving preferences.

## Receiving defaults and overrides

The account's **Who can send me files** setting supports:

- **Anyone:** includes anonymous senders when the instance permits anonymous uploads.
- **Signed-in users:** authenticated accounts on this instance.
- **Friends:** accepted friends.
- **Nobody:** only friends with an explicit Allow override.

New and upgraded accounts default to Anyone. Existing inbox enablement is retained.
The instance's restrictions and the account's inbox on/off switch always apply.

Each friend has separate **Can send me files** and **Automatic download** settings:
Inherit, Allow/On, or Deny/Off. Inherit follows the current account default. Your
settings affect incoming files from that friend; they do not change your friend's
settings for you.

Automatic download starts disabled and applies only to accepted friends. Allowing
someone to send files does not automatically enable downloading their deliveries.

Removing a friend removes their overrides, so subsequent receiving uses the
account default. Blocking rejects new and unfinished account-identified deliveries
and prevents queued automatic downloading. Completed inbox deliveries remain
manually accessible until expiry. Unblocking does not restore the friendship.
Anonymous deliveries have no sender account identity to match against a block;
choose Signed-in users or Friends to exclude them.

## Private automatic receiving

Enable automatic downloading for friends at the account level or for an individual
friend, then enable local receiving on the client/profile you want to use.

| Client | Execution |
| --- | --- |
| Web | While Filebeam is open, with catch-up when reopened. |
| Desktop | While running, including in the tray/menu bar. Explicit Quit stops receiving. |
| CLI | While `beam inbox watch` is running; `--once` performs a catch-up sweep. |
| Android/iOS | OS-scheduled background work and foreground catch-up. Timing depends on the OS. |

Incoming downloads contain ciphertext only and require no unlocked receiving key.
They stay in private staging. Unlock and verify them before explicitly saving or
exporting files; files are never automatically opened. Fully staged native/browser
deliveries retain their encrypted metadata and can be verified after server expiry.
Incomplete downloads still depend on the server retaining the transfer.

Native staging reserves up to 2 GiB per account/profile; browser staging uses a
512 MiB storage budget and the browser's available quota. Remove local staging
when it is no longer needed. Dismissed deliveries are remembered to prevent an
automatic redownload. Removing staging does not remove files already saved elsewhere.
Desktop and CLI profiles sharing the same local home coordinate receiving jobs.

## Beam commands

Sessions and contacts use the configured instance, or the global `--instance` option.

```sh
beam contacts request @alice
beam contacts requests
beam contacts accept @alice
beam contacts list
beam contacts set @alice --can-send allow --auto-download inherit
beam contacts block @alice
beam contacts unblock @alice

beam account receiving --policy friends --auto-download true
beam to @alice report.pdf
beam inbox watch
beam inbox watch --once
beam inbox staged
beam inbox save TRANSFER_ID --output ./received
beam inbox dismiss TRANSFER_ID
```

`beam to` uses encrypted HTTP inbox delivery. Its account destination must begin
with `@`. Directory inputs support `--zip` or `--individual`. Existing
`beam up --username alice FILE` remains available. The TUI's native-services palette
provides contact actions, receiving defaults, catch-up, and staged-file actions.
