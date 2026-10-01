#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]] || { printf 'Usage: %s ARCH DIRECTORY\n' "$0" >&2; exit 64; }
arch=$1 out=$2
case $arch in
    x86_64) checksum=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 ;;
    aarch64) checksum=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158 ;;
    *) exit 64 ;;
esac
mkdir -p "$out"
curl --fail --location --retry 3 "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$arch.AppImage" --output "$out/appimagetool"
printf '%s  %s\n' "$checksum" "$out/appimagetool" | sha256sum --check
chmod 755 "$out/appimagetool"
