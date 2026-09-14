#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
if [[ ${FILEBEAM_RUST_IN_CONTAINER:-} == 1 ]]; then
    exec "$@"
fi
git_dir=$(git -C "$root" rev-parse --path-format=absolute --git-dir)
git_common_dir=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)
container_git_dir=/git/common
if [[ $git_dir != "$git_common_dir" ]]; then
    [[ $git_dir == "$git_common_dir"/* ]] || { printf 'Worktree Git directory is outside its common directory: %s\n' "$git_dir" >&2; exit 1; }
    container_git_dir=/git/common/${git_dir#"$git_common_dir"/}
fi

image=${BEAM_DOCKER_IMAGE:-filebeam-beam-tooling:rust-1.98.0}
docker_limits=()
docker_env=()
docker_volumes=()
if [[ ${CI:-false} != true ]]; then
    docker_limits+=(--memory "${BEAM_DOCKER_MEMORY:-2g}" --memory-swap "${BEAM_DOCKER_MEMORY_SWAP:-2g}" --cpus "${BEAM_DOCKER_CPUS:-2}")
    docker_env+=(--env CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}")
elif [[ -n ${CARGO_BUILD_JOBS:-} ]]; then
    docker_env+=(--env CARGO_BUILD_JOBS)
fi
if [[ -n ${CARGO_TARGET_DIR:-} ]]; then
    docker_env+=(--env CARGO_TARGET_DIR)
fi
if [[ -n ${BEAM_CARGO_CACHE_DIR:-} ]]; then
    mkdir -p "$BEAM_CARGO_CACHE_DIR"
    cargo_cache=$(CDPATH='' cd -- "$BEAM_CARGO_CACHE_DIR" && pwd)
    docker_volumes+=(--volume "$cargo_cache:/tmp/cargo")
fi
if [[ -n ${BEAM_RELEASE_PUBLIC_KEY:-} ]]; then
    BEAM_RELEASE_PUBLIC_KEY=$(php "$root/scripts/release/cli-public-key.php" public)
    export BEAM_RELEASE_PUBLIC_KEY
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
    "${docker_limits[@]}" \
    --user "$(id -u):$(id -g)" \
    --env CARGO_HOME=/tmp/cargo \
    --env CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    --env GIT_DIR="$container_git_dir" \
    --env GIT_WORK_TREE=/workspace \
    "${docker_env[@]}" \
    "${docker_volumes[@]}" \
    --volume "$root:/workspace" \
    --volume "$git_common_dir:/git/common:ro" \
    --workdir /workspace \
    "$image" "$@"
