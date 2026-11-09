#!/usr/bin/env bash
# Shared scratch-directory handling for device verification runners.

device_root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)

device_die() {
    printf '%s\n' "device runner: $*" >&2
    exit 2
}

device_require() {
    command -v "$1" >/dev/null 2>&1 || device_die "'$1' is required; install it and retry."
}

device_init() {
    device_require cargo
    device_scratch=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-device.XXXXXX")
    chmod 700 "$device_scratch"
    export CARGO_TARGET_DIR="$device_scratch/target"
    trap 'rm -rf "$device_scratch"' EXIT
}
