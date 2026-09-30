#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || exit 64
unsigned=${FILEBEAM_DESKTOP_UNSIGNED:-0}
[[ $unsigned == 1 || ( -n ${WINDOWS_SIGNING_CERTIFICATE:-} && -n ${WINDOWS_SIGNING_PASSWORD:-} ) ]] || { printf 'Windows signing credentials are required\n' >&2; exit 1; }
command -v makensis >/dev/null
binary=$(realpath "$1") tag=$2 out=$3
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
mkdir -p "$out"
out=$(CDPATH='' cd -- "$out" && pwd)
sign() {
    MSYS_NO_PATHCONV=1 signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f "$WINDOWS_SIGNING_CERTIFICATE" /p "$WINDOWS_SIGNING_PASSWORD" "$1"
    MSYS_NO_PATHCONV=1 signtool verify /pa "$1"
}
if [[ $unsigned != 1 ]]; then sign "$binary"; fi
makensis -DINPUT="$binary" -DOUTPUT="$out/filebeam-desktop-$tag-windows-x86_64-setup.exe" -DICON="$root/desktop/packaging/icons/filebeam.ico" "$root/desktop/packaging/windows/filebeam.nsi"
if [[ $unsigned != 1 ]]; then sign "$out/filebeam-desktop-$tag-windows-x86_64-setup.exe"; fi
bash "$root/scripts/desktop/package-update-payload.sh" "$tag" windows x86_64 "$binary" "$out"
