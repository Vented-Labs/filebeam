#!/usr/bin/env bash
# Runs the implemented direct loopback device route through production daemons.
set -euo pipefail

source "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/common.sh"
device_init network-matrix

cargo test --manifest-path "$device_root/crates/daemon/Cargo.toml" --locked --test d2d -- --nocapture
