#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
if [[ -n ${WAYLAND_DISPLAY:-} && -n ${XDG_RUNTIME_DIR:-} ]]; then
    export FILEBEAM_DESKTOP_WAYLAND=1 FILEBEAM_DESKTOP_DRI=1
elif [[ -n ${DISPLAY:-} ]]; then
    export FILEBEAM_DESKTOP_X11=1
else
    printf 'Filebeam development launch requires a Wayland or X11 desktop session\n' >&2
    exit 1
fi
exec "$root/scripts/desktop/dev.sh" "$@"
