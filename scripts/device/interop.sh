#!/usr/bin/env bash
# Exercises two production daemon owners through the self-contained D2D harness.
set -euo pipefail

source "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/common.sh"
device_init interop
[[ $(uname -s) != MINGW* && $(uname -s) != CYGWIN* ]] || device_die 'daemon IPC requires a Unix host.'
cargo test --manifest-path "$device_root/crates/daemon/Cargo.toml" --locked --test d2d -- --nocapture
