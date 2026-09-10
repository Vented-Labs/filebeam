#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
crate=${1:-}
case "$crate" in
    encryption|transfer-wasm) ;;
    *) printf 'Usage: %s {encryption|transfer-wasm}\n' "$0" >&2; exit 64 ;;
esac

command -v docker >/dev/null || { printf '%s\n' 'docker is required' >&2; exit 1; }
[[ -f "$root/docker/wasm/Dockerfile" ]] || { printf '%s\n' 'docker/wasm/Dockerfile is required' >&2; exit 1; }
cache_home=${XDG_CACHE_HOME:-${HOME:+$HOME/.cache}}
[[ -n $cache_home ]] || { printf '%s\n' 'XDG_CACHE_HOME or HOME is required' >&2; exit 1; }
cache_root=${FILEBEAM_WASM_CACHE_DIR:-"$cache_home/filebeam/wasm"}
mkdir -p "$cache_root/cargo" "$cache_root/target"
cache_root=$(CDPATH='' cd -- "$cache_root" && pwd)

docker build --pull --tag filebeam-wasm-tooling:rust-1.98.0 \
    --file "$root/docker/wasm/Dockerfile" "$root/docker/wasm"

exec docker run --rm --init \
    --memory "${BEAM_DOCKER_MEMORY:-512m}" \
    --memory-swap "${BEAM_DOCKER_MEMORY_SWAP:-512m}" \
    --cpus "${BEAM_DOCKER_CPUS:-1}" \
    --env CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}" \
    --env HOME=/tmp \
    --user "$(id -u):$(id -g)" \
    --env CARGO_HOME=/cargo \
    --env CARGO_TARGET_DIR=/target \
    --volume "$cache_root/cargo:/cargo" \
    --volume "$cache_root/target:/target" \
    --volume "$root:/workspace" \
    --workdir /workspace \
    filebeam-wasm-tooling:rust-1.98.0 \
    wasm-pack build "$crate" --target web --release --no-opt
