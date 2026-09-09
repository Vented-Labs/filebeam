#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s [x86_64|aarch64]\n' "$0" >&2
    exit 64
}

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -f "$root/cli/Cargo.toml" ]] || { printf '%s\n' 'cli/Cargo.toml is required' >&2; exit 1; }
architecture=${1:-}
case "$architecture" in
    x86_64) target=x86_64-unknown-linux-gnu ;;
    aarch64) target=aarch64-unknown-linux-gnu ;;
    '') "$root/scripts/cli/run.sh" cargo build --manifest-path cli/Cargo.toml --release --locked; exit 0 ;;
    *) usage ;;
esac

"$root/scripts/cli/run.sh" cargo build --manifest-path cli/Cargo.toml --release --locked --target "$target"
