#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf '%s\n' 'Usage: scripts/docker/build.sh [--variant all|light|omnibus] [--platform linux/amd64|linux/arm64]'
}

variant=all
platform=""
while (($#)); do
    case "$1" in
        --variant) variant=${2:?missing variant}; shift 2 ;;
        --platform) platform=${2:?missing platform}; shift 2 ;;
        --help|-h) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done

case "$variant" in all|light|omnibus) ;; *) usage >&2; exit 2 ;; esac
if [[ -z "$platform" ]]; then
    case "$(uname -m)" in
        x86_64|amd64) platform=linux/amd64 ;;
        aarch64|arm64) platform=linux/arm64 ;;
        *) printf 'Unsupported native architecture: %s\n' "$(uname -m)" >&2; exit 2 ;;
    esac
fi
case "$platform" in linux/amd64|linux/arm64) ;; *) usage >&2; exit 2 ;; esac

command -v docker >/dev/null || { printf '%s\n' 'docker is required' >&2; exit 1; }
docker buildx version >/dev/null

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
[[ -f "$root/docker/production/Dockerfile" ]] || { printf '%s\n' 'docker/production/Dockerfile is not available yet' >&2; exit 1; }

if [[ -n ${FILEBEAM_IMAGE:-} && $variant == all ]]; then
    printf '%s\n' 'FILEBEAM_IMAGE requires --variant light or --variant omnibus' >&2
    exit 2
fi
if [[ ${FILEBEAM_PUSH:-false} == true && $variant == all ]]; then
    printf '%s\n' 'FILEBEAM_PUSH=true requires one explicit variant and one exact tag' >&2
    exit 2
fi

build_variant() {
    local current=$1 tag
    tag=${FILEBEAM_IMAGE:-filebeam/$current:local}
    local -a args=(buildx build --platform "$platform" --file "$root/docker/production/Dockerfile" --target "$current" --tag "$tag")
    [[ -n ${FILEBEAM_VERSION:-} ]] && args+=(--build-arg "VERSION=$FILEBEAM_VERSION")
    [[ -n ${FILEBEAM_COMMIT:-} ]] && args+=(--build-arg "COMMIT=$FILEBEAM_COMMIT")
    [[ -n ${FILEBEAM_BUILD_METADATA:-} ]] && args+=(--build-arg "BUILD_METADATA=$FILEBEAM_BUILD_METADATA")

    if [[ ${FILEBEAM_PUSH:-false} == true ]]; then
        args+=(--push --sbom="${FILEBEAM_SBOM:-true}" --provenance="${FILEBEAM_PROVENANCE:-mode=max}")
    else
        args+=(--load)
    fi
    args+=("$root")
    docker "${args[@]}"
}

if [[ $variant == all ]]; then
    build_variant light
    build_variant omnibus
else
    build_variant "$variant"
fi
