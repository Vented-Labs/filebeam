#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
manifest=$root/desktop/Cargo.toml
[[ -f $manifest ]] || { printf 'Desktop manifest is not available yet: %s\n' "$manifest" >&2; exit 1; }
exec "$root/scripts/desktop/run.sh" bash -ceu '
    cargo fmt --manifest-path /workspace/desktop/Cargo.toml --check
    cargo check --manifest-path /workspace/desktop/Cargo.toml --all-targets --locked "$@"
    cargo clippy --manifest-path /workspace/desktop/Cargo.toml --all-targets --locked "$@" -- -D warnings
' bash "$@"
