# filebeam-client-updater

The updater owns trusted Filebeam release retrieval and the shared `~/.filebeam/update`
state. Products use distinct signed catalog identities (`cli` and `desktop`) while sharing
the lock and atomic state journal.

```no_run
use filebeam_client_updater::{Options, Product, Updater};

let updater = Updater::new(Options::for_product(home, Product::Cli, version, public_key)?);
updater.launch(true)?; // Returns immediately; network work is done by the detached worker.
// In the product's hidden worker entry point:
updater.bootstrap(|| config.auto_update)?;
```

At startup, after parsing the product's home/config, call `activate_staged` with a gate that
reloads `updates.auto_update`. It returns `Activation::{None, Reexec, RelaunchGui}`; the host
must re-exec or quit/relaunch before dispatching the original command. `launch_due(&[...], true)`
schedules every installed product known to the host so a rarely launched product is not starved.

`run_manual` bypasses only the two-hour schedule, never the lock, signature, expiry,
generation, origin, checksum, size, archive-path, or managed-install checks. Signed catalog
assets have an explicit `kind`: `tar-gz` for regular Linux binaries, `appimage` for Linux
AppImages, `zip-exe` for Windows executables, and `app-tar-gz` containing `Filebeam.app` for
macOS. macOS validates the complete bundle with `codesign` and the expected bundle identifier.
