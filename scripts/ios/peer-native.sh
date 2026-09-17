#!/usr/bin/env bash
# Runs the Rust background descriptor acceptance example when its owning branch supplies it.
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
url=${FILEBEAM_ACCEPTANCE_INSTANCE:?FILEBEAM_ACCEPTANCE_INSTANCE is required}
example="$root/client-ffi/examples/background_acceptance.rs"
[[ -f $example ]] || { printf '%s\n' 'background_acceptance.rs is not present; native descriptor acceptance is not runnable on this worktree.' >&2; exit 78; }
FILEBEAM_ACCEPTANCE_INSTANCE="$url" FILEBEAM_ACCEPTANCE_ALLOW_HTTP=1 \
    cargo +1.98.0 run --manifest-path "$root/client-ffi/Cargo.toml" --example background_acceptance --locked
