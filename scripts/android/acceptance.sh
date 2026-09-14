#!/usr/bin/env bash
# Prepare and collect evidence for opt-in Android interoperability acceptance.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
results="$root/test-results/android/acceptance"
fixtures="$results/fixtures"

usage() {
    printf '%s\n' 'Usage: acceptance.sh --prepare | --sample-pid <android-pid> --label <label>'
}

prepare() {
    mkdir -p "$fixtures"
    # Sparse files retain their advertised size without consuming /tmp. Their
    # hashes are still computed across every byte, so fixture identity is real.
    truncate --size 513MiB "$fixtures/android-parity-513MiB.bin"
    truncate --size 4097MiB "$fixtures/android-parity-4097MiB.bin"
    {
        printf 'revision=%s\n' "$(git -C "$root" rev-parse HEAD)"
        for file in "$fixtures"/*.bin; do
            printf '%s\t%s\t%s\n' "$(basename "$file")" "$(stat -c %s "$file")" "$(sha256sum "$file" | cut -d ' ' -f 1)"
        done
    } > "$results/fixtures.tsv"
    printf 'Prepared fixtures and evidence: %s\n' "$results/fixtures.tsv"
}

sample_pid() {
    local pid=$1 label=$2 status
    mkdir -p "$results"
    status=$(adb shell "cat /proc/$pid/status" | tr -d '\r')
    {
        printf 'timestamp=%s\n' "$(date --iso-8601=seconds)"
        printf 'label=%s\n' "$label"
        printf 'pid=%s\n' "$pid"
        printf '%s\n' "$status" | command grep -E '^(VmRSS|VmHWM|VmSize):'
    } >> "$results/memory.tsv"
    printf 'Recorded memory sample: %s\n' "$results/memory.tsv"
}

case ${1:-} in
    --prepare)
        [[ $# == 1 ]] || { usage >&2; exit 2; }
        prepare
        ;;
    --sample-pid)
        [[ $# == 4 && ${3:-} == --label && -n ${4:-} ]] || { usage >&2; exit 2; }
        sample_pid "$2" "$4"
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
