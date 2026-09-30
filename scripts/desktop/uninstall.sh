#!/bin/sh
set -eu
root=${FILEBEAM_DESKTOP_HOME:-"${HOME:?HOME is required}/.filebeam"}; rm -f "$root/bin/filebeam" "$root/apps/desktop/Filebeam.AppImage" "$HOME/.local/share/applications/filebeam.desktop"; rmdir "$root/apps/desktop" 2>/dev/null || :
