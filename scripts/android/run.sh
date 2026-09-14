#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
if [[ ${FILEBEAM_ANDROID_IN_CONTAINER:-} == 1 ]]; then
    exec "$@"
fi
image=${FILEBEAM_ANDROID_IMAGE:-filebeam-android-tooling:rust-1.98.0-sdk37}
cache=${FILEBEAM_ANDROID_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/filebeam/android}
mkdir -p "$cache/cargo" "$cache/gradle" "$cache/home"
if [[ ${FILEBEAM_ANDROID_BUILD_IMAGE:-true} == true ]]; then
    docker build --tag "$image" "$root/docker/android"
fi
limits=()
cargo_jobs=${CARGO_BUILD_JOBS:-2}
if [[ ${CI:-false} != true ]]; then
    limits+=(--memory "${FILEBEAM_ANDROID_MEMORY:-6g}" --cpus "${FILEBEAM_ANDROID_CPUS:-4}")
else
    cargo_jobs=${CARGO_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}
fi
exec docker run --rm --init "${limits[@]}" \
    --user "$(id -u):$(id -g)" \
    --env HOME=/tmp/home --env CARGO_HOME=/tmp/cargo --env GRADLE_USER_HOME=/tmp/gradle \
    --env CI="${CI:-false}" --env CARGO_BUILD_JOBS="$cargo_jobs" \
    --volume "$cache/home:/tmp/home" --volume "$cache/cargo:/tmp/cargo" --volume "$cache/gradle:/tmp/gradle" \
    --volume "$root:/workspace" --workdir /workspace/mobile/android \
    "$image" "$@"
