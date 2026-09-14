#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
binary=${BEAM_TRANSFER_TEST_BINARY:-"$root/cli/target/release/beam"}
[[ -x "$binary" ]] || { printf 'Missing explicit release candidate: %s\n' "$binary" >&2; exit 1; }
binary=$(realpath -- "$binary")
sha256sum "$binary"
case "$binary" in
    "$root"/*) container_binary=/workspace/${binary#"$root"/} ;;
    *) container_binary=/beam-bin/$(basename -- "$binary") ;;
esac
limits=(--memory "${BEAM_DOCKER_MEMORY:-512m}" --memory-swap "${BEAM_DOCKER_MEMORY_SWAP:-512m}" --cpus "${BEAM_DOCKER_CPUS:-1}")
volumes=(--volume "$root:/workspace:ro")
case "$binary" in "$root"/*) ;; *) volumes+=(--volume "$(dirname -- "$binary"):/beam-bin:ro") ;; esac
exec docker run --rm --init "${limits[@]}" --user "$(id -u):$(id -g)" \
    --env PYTHONDONTWRITEBYTECODE=1 --env BEAM_TRANSFER_TEST_FULL --env BEAM_TRANSFER_TEST_FULL_BYTES \
    --env BEAM_TRANSFER_TEST_CHUNK_BYTES "${volumes[@]}" --workdir /workspace \
    python:3.13.7-slim-bookworm python scripts/cli/transfer.test.py "$container_binary"
