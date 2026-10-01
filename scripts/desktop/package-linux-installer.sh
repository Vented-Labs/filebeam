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
mkdir -p "$tmp/usr/bin" "$tmp/usr/lib" "$out"
install -m 0755 "$binary" "$tmp/usr/bin/filebeam"
install -m 0755 "$root/desktop/packaging/linux/AppRun" "$tmp/AppRun"
# Keep libc and graphics drivers supplied by the host. Bundle the remaining
# linked libraries so a normal desktop does not need development packages.
while IFS= read -r library; do
    case $(basename "$library") in
        libc.so.*|libm.so.*|libdl.so.*|libpthread.so.*|librt.so.*|ld-linux*|libGL.so.*|libEGL.so.*|libvulkan.so.*) continue ;;
    esac
    cp -L "$library" "$tmp/usr/lib/"
done < <(ldd "$binary" | awk '/=> \// { print $3 }')
cp "$root/desktop/packaging/linux/io.filebeam.desktop.desktop" "$tmp/io.filebeam.desktop.desktop"
install -m 0644 "$root/desktop/packaging/icons/png/256.png" "$tmp/.DirIcon"
install -m 0644 "$root/desktop/packaging/icons/png/256.png" "$tmp/io.filebeam.desktop.png"
for size in 16 32 48 64 128 256 512 1024; do
    icon_dir="$tmp/usr/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    install -m 0644 "$root/desktop/packaging/icons/png/$size.png" "$icon_dir/io.filebeam.desktop.png"
done
export ARCH=${ARCH:-x86_64}
APPIMAGE_EXTRACT_AND_RUN=1 appimagetool "$tmp" "$out/filebeam-desktop-$tag-linux-$ARCH.AppImage"
