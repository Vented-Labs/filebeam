# Device Validation Harness

Run `bash tests/device/check.sh` to validate runner syntax and output-directory safeguards.

`scripts/device/production-backend.sh` creates a disposable container-mode Laravel SQLite instance and serves it through a locally trusted HTTPS certificate. It runs two production daemon processes through the test-only explicit CA constructor, verifies real private bytes and a receiver receipt, revokes a receiver during a multi-chunk direct transfer before commit, and checks that the web revoke disables the credential and new rendezvous admission.

`scripts/device/check.sh` runs formatting and production Rust tests for the transfer-native, client-core, and daemon crates. The other runners use a temporary `CARGO_TARGET_DIR` and remove it on exit. Set `FILEBEAM_DEVICE_LOG_DIR=$PWD/.filebeam/device/<run>` to retain their console output; no other persistent output location is accepted.

`interop.sh` starts two `filebeamd` processes with separate profiles and controls them through the production `beam` CLI. It exits 77 after that verified IPC coverage because the current daemon exposes no peer pairing, signaling, or dispatch interface.

`network-matrix.sh` and `crash-matrix.sh` execute the relevant production Rust tests then exit 77 while the required production route API and crash fault-injection boundaries are absent. They do not mark unimplemented matrix cells as passed.

`benchmark.sh -- COMMAND [ARGUMENT ...]` times the supplied production command. Its only capture is a synthetic JSON record containing elapsed time and exit status; command output is suppressed so it does not record transfer names, paths, content, credentials, or protocol payloads.
