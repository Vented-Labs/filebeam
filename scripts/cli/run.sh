#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
image=${BEAM_DOCKER_IMAGE:-filebeam-beam-tooling:rust-1.98.0}
memory=${BEAM_DOCKER_MEMORY:-2g}
memory_swap=${BEAM_DOCKER_MEMORY_SWAP:-2g}
cpus=${BEAM_DOCKER_CPUS:-2}
jobs=${CARGO_BUILD_JOBS:-1}
docker_env=()
docker_volumes=()
if [[ -n ${BEAM_RELEASE_PUBLIC_KEY:-} ]]; then
    docker_env+=(--env BEAM_RELEASE_PUBLIC_KEY)
fi
if [[ -n ${BEAM_DOCKER_OUTPUT_DIR:-} ]]; then
    [[ -d $BEAM_DOCKER_OUTPUT_DIR ]] || { printf 'Docker output directory does not exist: %s\n' "$BEAM_DOCKER_OUTPUT_DIR" >&2; exit 1; }
    docker_volumes+=(--volume "$BEAM_DOCKER_OUTPUT_DIR:$BEAM_DOCKER_OUTPUT_DIR")
fi

command -v docker >/dev/null || { printf '%s\n' 'docker is required' >&2; exit 1; }
[[ -f "$root/docker/cli/Dockerfile" ]] || { printf '%s\n' 'docker/cli/Dockerfile is required' >&2; exit 1; }

if [[ ${BEAM_DOCKER_BUILD:-true} == true ]]; then
    docker build --pull --tag "$image" --file "$root/docker/cli/Dockerfile" "$root"
fi

exec docker run --rm --init \
    --memory "$memory" \
    --memory-swap "$memory_swap" \
    --cpus "$cpus" \
    --user "$(id -u):$(id -g)" \
    --env CARGO_BUILD_JOBS="$jobs" \
    --env CARGO_HOME=/tmp/cargo \
    --env CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    "${docker_env[@]}" \
    "${docker_volumes[@]}" \
    --volume "$root:/workspace" \
    --workdir /workspace \
    "$image" "$@"
