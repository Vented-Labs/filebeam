#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -f "$root/cli/Cargo.toml" ]] || { printf '%s\n' 'cli/Cargo.toml is required' >&2; exit 1; }

"$root/scripts/cli/run.sh" cargo test --manifest-path cli/Cargo.toml --locked
