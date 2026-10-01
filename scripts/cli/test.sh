#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -f "$root/crates/cli/Cargo.toml" ]] || { printf '%s\n' 'crates/cli/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/crates/transfer/Cargo.toml" ]] || { printf '%s\n' 'crates/transfer/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/crates/transfer-native/Cargo.toml" ]] || { printf '%s\n' 'crates/transfer-native/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/crates/transfer-wasm/Cargo.toml" ]] || { printf '%s\n' 'crates/transfer-wasm/Cargo.toml is required' >&2; exit 1; }

"$root/scripts/cli/run.sh" bash -ceu '
cargo test --manifest-path crates/cli/Cargo.toml --locked
cargo test --manifest-path crates/transfer/Cargo.toml --locked
cargo test --manifest-path crates/transfer-native/Cargo.toml --locked
cargo test --manifest-path crates/transfer-wasm/Cargo.toml --locked
'
