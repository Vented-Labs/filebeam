#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -f "$root/crates/cli/Cargo.toml" ]] || { printf '%s\n' 'crates/cli/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/crates/transfer/Cargo.toml" ]] || { printf '%s\n' 'crates/transfer/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/crates/transfer-native/Cargo.toml" ]] || { printf '%s\n' 'crates/transfer-native/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/crates/transfer-wasm/Cargo.toml" ]] || { printf '%s\n' 'crates/transfer-wasm/Cargo.toml is required' >&2; exit 1; }

"$root/scripts/cli/run.sh" bash -ceu '
cargo fmt --manifest-path crates/cli/Cargo.toml --check
cargo clippy --manifest-path crates/cli/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path crates/transfer/Cargo.toml --check
cargo clippy --manifest-path crates/transfer/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path crates/transfer-native/Cargo.toml --check
cargo clippy --manifest-path crates/transfer-native/Cargo.toml --all-targets --locked -- -D warnings
cargo fmt --manifest-path crates/transfer-wasm/Cargo.toml --check
cargo clippy --manifest-path crates/transfer-wasm/Cargo.toml --all-targets --locked -- -D warnings
'
