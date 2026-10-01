#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
source="$root/desktop/assets/filebeam-mark.svg"
output="$root/desktop/packaging/icons"
converter=${MAGICK_BIN:-}

if [[ -z $converter ]]; then
    if command -v magick >/dev/null; then converter=magick
    elif command -v convert >/dev/null; then converter=convert
    else
        printf 'ImageMagick (magick or convert) is required to generate icons\n' >&2
        exit 1
    fi
fi
[[ -f $source ]] || { printf 'Approved Filebeam mark is missing: %s\n' "$source" >&2; exit 1; }
command -v icotool >/dev/null || { printf 'icotool is required to generate the Windows ICO\n' >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/png"
sizes=(16 32 48 64 128 256 512 1024)
for size in "${sizes[@]}"; do
    mark_size=$((size * 3 / 4))
    "$converter" -background none "$source" -resize "${mark_size}x${mark_size}" -gravity center -extent "${size}x${size}" "$tmp/png/$size.png"
done
icotool --create --output "$tmp/filebeam.ico" "$tmp/png/16.png" "$tmp/png/32.png" \
    "$tmp/png/48.png" "$tmp/png/64.png" "$tmp/png/128.png" "$tmp/png/256.png"
python3 "$root/scripts/desktop/generate-icns.py" "$tmp/filebeam.icns" \
    "$tmp/png/16.png" "$tmp/png/32.png" "$tmp/png/64.png" "$tmp/png/128.png" \
    "$tmp/png/256.png" "$tmp/png/512.png" "$tmp/png/1024.png"
rm -rf "$output"
mkdir -p "$output"
mv "$tmp/png" "$output/png"
mv "$tmp/filebeam.ico" "$tmp/filebeam.icns" "$output/"
