#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -f "$root/cli/Cargo.toml" ]] || { printf '%s\n' 'cli/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/transfer/Cargo.toml" ]] || { printf '%s\n' 'transfer/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/transfer-native/Cargo.toml" ]] || { printf '%s\n' 'transfer-native/Cargo.toml is required' >&2; exit 1; }
[[ -f "$root/transfer-wasm/Cargo.toml" ]] || { printf '%s\n' 'transfer-wasm/Cargo.toml is required' >&2; exit 1; }

"$root/scripts/cli/run.sh" bash -ceu '
cargo test --manifest-path cli/Cargo.toml --locked
cargo test --manifest-path transfer/Cargo.toml --locked
cargo test --manifest-path transfer-native/Cargo.toml --locked
cargo test --manifest-path transfer-wasm/Cargo.toml --locked
'
