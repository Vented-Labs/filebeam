#!/usr/bin/env bash
# Verifies durable commit behavior that is implemented today without simulating success.
set -euo pipefail

source "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/common.sh"
device_init crash-matrix

cargo test --manifest-path "$device_root/crates/daemon/Cargo.toml" --locked grant_identity_and_commit_receipt_are_durable_and_idempotent
cargo test --manifest-path "$device_root/crates/daemon/Cargo.toml" --locked commit_never_replaces_an_existing_destination
cargo test --manifest-path "$device_root/crates/daemon/Cargo.toml" --locked owner_authenticated_ipc_persists_receive_policy_and_stops

printf '%s\n' 'Crash/restart fault cells are not implemented; the durable journal unit cases above are the executed coverage.' >&2
