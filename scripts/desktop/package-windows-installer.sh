#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || exit 64
command -v makensis >/dev/null
[[ -s $1 ]] || { printf 'Missing Windows desktop executable: %s\n' "$1" >&2; exit 1; }
binary=$(realpath "$1") tag=$2 out=$3
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
mkdir -p "$out"
out=$(CDPATH='' cd -- "$out" && pwd)
MSYS_NO_PATHCONV=1 makensis \
    "-DINPUT=$(cygpath -w "$binary")" \
    "-DOUTPUT=$(cygpath -w "$out/filebeam-desktop-$tag-windows-x86_64-setup.exe")" \
    "-DICON=$(cygpath -w "$root/desktop/packaging/icons/filebeam.ico")" \
    "$(cygpath -w "$root/desktop/packaging/windows/filebeam.nsi")"
bash "$root/scripts/desktop/package-update-payload.sh" "$tag" windows x86_64 "$binary" "$out"
