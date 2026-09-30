#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 5 ]] || { printf 'Usage: %s vX.Y.Z OS ARCH BINARY OUTPUT\n' "$0" >&2; exit 64; }
tag=$1 os=$2 arch=$3 binary=$4 output=$5
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 64
mkdir -p "$output"; tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT; mkdir -p "$tmp/filebeam"
suffix=''; [[ $os == windows ]] && suffix=.exe
name="filebeam-desktop-$tag-$os-$arch"
if [[ $os == macos ]]; then
    [[ -d $binary && -f $binary/Contents/MacOS/filebeam ]] || { printf 'macOS payload must be a Filebeam.app bundle\n' >&2; exit 64; }
    cp -R "$binary" "$tmp/Filebeam.app"
    (cd "$tmp" && tar -czf "$output/$name.tar.gz" Filebeam.app)
else
    [[ -f $binary ]] || exit 64
    install -m 0755 "$binary" "$tmp/filebeam/filebeam$suffix"
    if [[ $os == windows ]]; then (cd "$tmp" && zip -q "$output/$name.zip" filebeam/filebeam.exe); else (cd "$tmp" && tar -czf "$output/$name.tar.gz" filebeam/filebeam); fi
fi
