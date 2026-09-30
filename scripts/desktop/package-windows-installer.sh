#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 && -n ${WINDOWS_SIGNING_CERTIFICATE:-} && -n ${WINDOWS_SIGNING_PASSWORD:-} ]] || { printf 'binary tag output plus Windows signing credentials are required\n' >&2; exit 1; }
command -v makensis >/dev/null; command -v signtool >/dev/null
binary=$1 tag=$2 out=$3; root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd); makensis -DINPUT="$binary" -DOUTPUT="$out/filebeam-desktop-$tag-windows-x86_64-setup.exe" -DICON="$root/desktop/packaging/icons/filebeam.ico" "$root/desktop/packaging/windows/filebeam.nsi"; signtool sign /fd SHA256 /f "$WINDOWS_SIGNING_CERTIFICATE" /p "$WINDOWS_SIGNING_PASSWORD" "$out/filebeam-desktop-$tag-windows-x86_64-setup.exe"
