#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
for script in common.sh check.sh interop.sh network-matrix.sh crash-matrix.sh benchmark.sh production-backend.sh; do
    bash -n "$root/scripts/device/$script"
done

scratch=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-device-script-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
if FILEBEAM_DEVICE_LOG_DIR="$scratch" bash "$root/scripts/device/benchmark.sh" -- true >/dev/null 2>&1; then
    printf '%s\n' 'benchmark accepted a log directory outside .filebeam/' >&2
    exit 1
fi
