#!/bin/sh
set -eu
# Consumer installer: updater archives are never used as native installers.
home=${HOME:?HOME is required}; root=${FILEBEAM_DESKTOP_HOME:-$home/.filebeam}; app=${1:?Path to verified AppImage is required}; source_root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[ -f "$app" ] || exit 1; mkdir -p "$root/apps/desktop" "$root/bin" "$home/.local/share/applications"
install -m 0755 "$app" "$root/apps/desktop/Filebeam.AppImage"; ln -sfn ../apps/desktop/Filebeam.AppImage "$root/bin/filebeam"
for size in 16 32 48 64 128 256 512 1024; do
    icon_dir="$home/.local/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    install -m 0644 "$source_root/desktop/packaging/icons/png/$size.png" "$icon_dir/io.filebeam.desktop.png"
done
legacy="$home/.local/share/applications/filebeam.desktop"
if [ -f "$legacy" ] && grep -Fqx "Exec=$root/bin/filebeam %u" "$legacy"; then rm -f "$legacy"; fi
cat >"$home/.local/share/applications/io.filebeam.desktop.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Filebeam
Exec=$root/bin/filebeam %u
Icon=io.filebeam.desktop
StartupWMClass=io.filebeam.desktop
MimeType=x-scheme-handler/filebeam;
Categories=Network;
EOF
printf 'Installed Filebeam without modifying beam, config.toml, or PATH.\n'
