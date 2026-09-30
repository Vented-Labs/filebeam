#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || exit 64
binary=$1
tag=$2
out=$3
command -v appimagetool >/dev/null || { printf 'appimagetool is required for a native Linux installer\n' >&2; exit 1; }
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/usr/bin"
install -m 0755 "$binary" "$tmp/usr/bin/filebeam"
cp "$root/desktop/packaging/linux/io.filebeam.desktop.desktop" "$tmp/io.filebeam.desktop.desktop"
install -m 0644 "$root/desktop/packaging/icons/png/256.png" "$tmp/.DirIcon"
install -m 0644 "$root/desktop/packaging/icons/png/256.png" "$tmp/io.filebeam.desktop.png"
for size in 16 32 48 64 128 256 512 1024; do
    icon_dir="$tmp/usr/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    install -m 0644 "$root/desktop/packaging/icons/png/$size.png" "$icon_dir/io.filebeam.desktop.png"
done
ARCH=${ARCH:-x86_64}
appimagetool "$tmp" "$out/filebeam-desktop-$tag-linux-$ARCH.AppImage"
