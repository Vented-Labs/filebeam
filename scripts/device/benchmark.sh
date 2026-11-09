#!/usr/bin/env bash
# Times an explicitly supplied production command and captures synthetic metadata only.
set -euo pipefail

source "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/common.sh"
device_init benchmark
[[ ${1:-} == -- && $# -gt 1 ]] || device_die 'usage: benchmark.sh -- COMMAND [ARGUMENT ...]; supply a real production transfer command.'
shift

trace="$device_scratch/synthetic-trace.json"
started=$(date +%s%N)
set +e
"$@" >/dev/null 2>&1
status=$?
set -e
finished=$(date +%s%N)
elapsed_ms=$(( (finished - started) / 1000000 ))
printf '{"schema":"filebeam-device-benchmark-v1","input":"synthetic","elapsed_ms":%s,"exit_status":%s}\n' "$elapsed_ms" "$status" > "$trace"
printf '%s\n' "$(<"$trace")"
[[ $status == 0 ]] || exit "$status"
