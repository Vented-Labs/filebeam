#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
command -v docker >/dev/null || { printf '%s\n' 'docker is required' >&2; exit 1; }
command -v flock >/dev/null || { printf '%s\n' 'flock is required' >&2; exit 1; }
[[ $# -gt 0 ]] || { printf 'usage: %s COMMAND [ARG...]\n' "$0" >&2; exit 2; }
[[ -f "$root/docker/desktop/Dockerfile" ]] || { printf '%s\n' 'docker/desktop/Dockerfile is required' >&2; exit 1; }
git_dir=$(git -C "$root" rev-parse --path-format=absolute --git-dir)
git_common_dir=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)
container_git_dir=/git/common
if [[ $git_dir != "$git_common_dir" ]]; then
    [[ $git_dir == "$git_common_dir"/* ]] || { printf 'Worktree Git directory is outside its common directory: %s\n' "$git_dir" >&2; exit 1; }
    container_git_dir=/git/common/${git_dir#"$git_common_dir"/}
fi

# Include the absolute worktree path so branches/worktrees never share mutable Cargo state.
cache_key=$(printf '%s' "$root" | sha256sum | cut -c1-24)
cache_root=${FILEBEAM_DESKTOP_CACHE_DIR:-"${XDG_CACHE_HOME:-$HOME/.cache}/filebeam/desktop/$cache_key"}
umask 077
mkdir -p "$cache_root/cargo" "$cache_root/target" "$cache_root/locks"
cache_root=$(CDPATH='' cd -- "$cache_root" && pwd)
[[ ! -L $cache_root && $(stat -c '%u' "$cache_root") == $(id -u) ]] || { printf 'Desktop cache must be a directory owned by the current user: %s\n' "$cache_root" >&2; exit 1; }
chmod 700 "$cache_root" "$cache_root/cargo" "$cache_root/target" "$cache_root/locks"

# Restored Docker images must match the recipe used by the Cargo cache generation.
tooling_key=$(sha256sum "$root/docker/desktop/Dockerfile" | cut -c1-12)
image=${FILEBEAM_DESKTOP_IMAGE:-filebeam-desktop:rust-1.98.0-$tooling_key}
build_image=${FILEBEAM_DESKTOP_BUILD_IMAGE:-auto}
case $build_image in
    auto|true|false) ;;
    *) printf 'FILEBEAM_DESKTOP_BUILD_IMAGE must be auto, true, or false\n' >&2; exit 2 ;;
esac

limits=()
cargo_env=()
if [[ ${CI:-false} != true ]]; then
    limits+=(--memory 4g --memory-swap 4g --cpus 2)
    cargo_env+=(--env CARGO_BUILD_JOBS=1)
elif [[ -n ${CARGO_BUILD_JOBS:-} ]]; then
    cargo_env+=(--env CARGO_BUILD_JOBS)
fi

docker_args=(
    run --rm --init
    "${limits[@]}"
    --user "$(id -u):$(id -g)"
    --env CARGO_HOME=/cargo
    --env CARGO_TARGET_DIR=/target
    "${cargo_env[@]}"
    --env CARGO_PROFILE_DEV_DEBUG=0
    --env CARGO_PROFILE_TEST_DEBUG=0
    --env CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=off
    --env CARGO_PROFILE_TEST_SPLIT_DEBUGINFO=off
    --env HOME=/tmp
    --env "GIT_DIR=$container_git_dir"
    --env GIT_WORK_TREE=/workspace
    --volume "$cache_root/cargo:/cargo"
    --volume "$cache_root/target:/target"
    --volume "$root:/workspace"
    --volume "$git_common_dir:/git/common:ro"
    --workdir /workspace
)

# Desktop config is deliberately not inherited by default. A real-app launch may
# explicitly bind a user-owned home and select it with `filebeam --home`.
if [[ -v FILEBEAM_DESKTOP_HOME ]]; then
    [[ -n $FILEBEAM_DESKTOP_HOME && $FILEBEAM_DESKTOP_HOME == /* && -d $FILEBEAM_DESKTOP_HOME ]] || {
        printf '%s\n' 'FILEBEAM_DESKTOP_HOME must be an existing absolute directory' >&2
        exit 2
    }
    desktop_home=$(CDPATH='' cd -- "$FILEBEAM_DESKTOP_HOME" && pwd -P)
    [[ $(stat -c '%u' "$desktop_home") == $(id -u) ]] || {
        printf 'Desktop home must be owned by the current user: %s\n' "$desktop_home" >&2
        exit 2
    }
    docker_args+=(--volume "$desktop_home:/filebeam-home:rw")
fi

# Release metadata is consumed by desktop/build.rs. Keep the container boundary
# explicit rather than inheriting the complete host environment.
for name in FILEBEAM_RELEASE_VERSION FILEBEAM_RELEASE_TAG FILEBEAM_RELEASE_SHA FILEBEAM_RELEASE_PUBLIC_KEY; do
    [[ -v $name ]] && docker_args+=(--env "$name=${!name}")
done

if [[ ${FILEBEAM_DESKTOP_X11:-0} == 1 ]]; then
    [[ -n ${DISPLAY:-} ]] || { printf '%s\n' 'DISPLAY is required when FILEBEAM_DESKTOP_X11=1' >&2; exit 2; }
    [[ -d /tmp/.X11-unix ]] || { printf '%s\n' '/tmp/.X11-unix is required when FILEBEAM_DESKTOP_X11=1' >&2; exit 2; }
    docker_args+=(--env DISPLAY --volume /tmp/.X11-unix:/tmp/.X11-unix:rw)
    if [[ -n ${XAUTHORITY:-} ]]; then
        [[ -f $XAUTHORITY ]] || { printf 'XAUTHORITY does not exist: %s\n' "$XAUTHORITY" >&2; exit 2; }
        docker_args+=(--env XAUTHORITY --volume "$XAUTHORITY:$XAUTHORITY:ro")
    fi
fi

if [[ ${FILEBEAM_DESKTOP_WAYLAND:-0} == 1 ]]; then
    [[ -n ${XDG_RUNTIME_DIR:-} && -d ${XDG_RUNTIME_DIR:-} ]] || { printf '%s\n' 'a valid XDG_RUNTIME_DIR is required when FILEBEAM_DESKTOP_WAYLAND=1' >&2; exit 2; }
    [[ -n ${WAYLAND_DISPLAY:-} ]] || { printf '%s\n' 'WAYLAND_DISPLAY is required when FILEBEAM_DESKTOP_WAYLAND=1' >&2; exit 2; }
    docker_args+=(--env XDG_RUNTIME_DIR --env WAYLAND_DISPLAY --volume "$XDG_RUNTIME_DIR:$XDG_RUNTIME_DIR:rw")
fi

if [[ ${FILEBEAM_DESKTOP_DRI:-0} == 1 ]]; then
    [[ -e /dev/dri ]] || { printf '%s\n' '/dev/dri is required when FILEBEAM_DESKTOP_DRI=1' >&2; exit 2; }
    docker_args+=(--device /dev/dri)
fi

# Any command can invoke Cargo through a shell script, so serialize the complete
# runner rather than trying to identify Cargo from its first argument.
exec {runner_lock}>"$cache_root/locks/runner.lock"
flock "$runner_lock"

if [[ $build_image == true ]] || { [[ $build_image == auto ]] && ! docker image inspect "$image" >/dev/null 2>&1; }; then
    docker build --pull --tag "$image" --file "$root/docker/desktop/Dockerfile" "$root/docker/desktop"
fi

exec docker "${docker_args[@]}" "$image" "$@"
