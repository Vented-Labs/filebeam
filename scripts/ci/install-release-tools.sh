#!/usr/bin/env bash
set -euo pipefail

[[ $# -le 1 ]] || { printf 'Usage: %s [amd64|arm64]\n' "$0" >&2; exit 64; }

case ${1:-$(uname -m)} in
    amd64|x86_64) arch=amd64 ;;
    arm64|aarch64) arch=arm64 ;;
    *) printf 'Unsupported architecture: %s\n' "${1:-$(uname -m)}" >&2; exit 1 ;;
esac

case $arch in
    amd64) checksum=4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71 ;;
    arm64) checksum=c5d324e091826b0d7a78eb16fef316450b4eb9aaec045611c08ba06f5e73220a ;;
esac

version=v3.1.3
destination=${RUNNER_TEMP:-/tmp}/filebeam-ci/bin
binary="$destination/cosign"
download="$destination/cosign-linux-$arch"
mkdir -p "$destination"
curl --fail --location --retry 3 --silent --show-error \
    "https://github.com/sigstore/cosign/releases/download/$version/cosign-linux-$arch" \
    --output "$download"
printf '%s  %s\n' "$checksum" "$download" | sha256sum --check --status
chmod 0755 "$download"
mv "$download" "$binary"
if [[ -n ${GITHUB_PATH:-} ]]; then
    printf '%s\n' "$destination" >> "$GITHUB_PATH"
else
    printf '%s\n' "$destination"
fi
