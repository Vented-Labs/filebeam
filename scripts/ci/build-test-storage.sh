#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
case ${1:-all} in
    minio) targets=(minio) ;;
    all) targets=(minio mc) ;;
    *) printf 'Usage: %s [minio|all]\n' "$0" >&2; exit 64 ;;
esac
for target in "${targets[@]}"; do
    docker buildx build --load --target "$target" --tag "filebeam-test-$target:local" "$root/docker/testing/minio"
done
