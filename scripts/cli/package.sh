#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s beam-vX.Y.Z [output-dir]\n' "$0" >&2
    exit 64
}

[[ $# -ge 1 && $# -le 2 ]] || usage
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -f "$root/cli/Cargo.toml" && -f "$root/cli/Cargo.lock" ]] || { printf '%s\n' 'cli/Cargo.toml and cli/Cargo.lock are required' >&2; exit 1; }
tag=$1
output_dir=${2:-dist/beam}
mkdir -p "$output_dir"
output_dir=$(CDPATH='' cd -- "$output_dir" && pwd)

BEAM_DOCKER_OUTPUT_DIR=$output_dir \
    "$root/scripts/cli/run.sh" bash scripts/cli/package-inner.sh "$tag" "$output_dir"
