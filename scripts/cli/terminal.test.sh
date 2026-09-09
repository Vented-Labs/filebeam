#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ -x "$root/cli/target/release/beam" ]] || { printf '%s\n' 'Run scripts/cli/build.sh first.' >&2; exit 1; }
docker_limits=()
if [[ ${CI:-false} != true ]]; then
    docker_limits+=(--memory "${BEAM_DOCKER_MEMORY:-2g}" --memory-swap "${BEAM_DOCKER_MEMORY_SWAP:-2g}" --cpus "${BEAM_DOCKER_CPUS:-2}")
fi
exec docker run --rm --init "${docker_limits[@]}" \
    --user "$(id -u):$(id -g)" --env PYTHONDONTWRITEBYTECODE=1 \
    --volume "$root:/workspace:ro" --workdir /workspace \
    python:3.13.7-slim-bookworm python scripts/cli/terminal.test.py cli/target/release/beam
