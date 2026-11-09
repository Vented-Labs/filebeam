#!/usr/bin/env bash
set -euo pipefail

source "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/common.sh"
device_init check

for manifest in crates/transfer-native/Cargo.toml crates/client-core/Cargo.toml crates/daemon/Cargo.toml; do
    [[ -f "$device_root/$manifest" ]] || device_die "required Rust manifest is missing: $manifest"
    cargo fmt --manifest-path "$device_root/$manifest" --check
    cargo test --manifest-path "$device_root/$manifest" --locked
done
