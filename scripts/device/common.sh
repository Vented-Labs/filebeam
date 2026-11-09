#!/usr/bin/env bash
# Shared safety and scratch-directory handling for device verification runners.

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

    if [[ -n ${FILEBEAM_DEVICE_LOG_DIR:-} ]]; then
        case $FILEBEAM_DEVICE_LOG_DIR in
            "$device_root/.filebeam"|"$device_root/.filebeam"/*) ;;
            *) device_die 'FILEBEAM_DEVICE_LOG_DIR must be an explicitly selected directory under .filebeam/.' ;;
        esac
        mkdir -p "$FILEBEAM_DEVICE_LOG_DIR"
        chmod 700 "$FILEBEAM_DEVICE_LOG_DIR"
        device_log="$FILEBEAM_DEVICE_LOG_DIR/${1}.log"
        exec > >(tee "$device_log") 2>&1
    fi
}

device_wait_for_socket() {
    local socket=$1
    for _ in {1..100}; do
        [[ -S $socket ]] && return 0
        sleep 0.05
    done
    return 1
}
