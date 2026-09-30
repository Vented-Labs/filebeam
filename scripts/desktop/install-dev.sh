#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
home=${HOME:?HOME is required}
share="$home/.local/share"
applications="$share/applications"
desktop_entry="$applications/io.filebeam.desktop.desktop"

[[ -d $home/.local && -d $share ]] || {
    printf 'Expected per-user data directories are missing below %s/.local\n' "$home" >&2
    exit 1
}
if [[ -f $desktop_entry ]] && ! grep -Fqx 'X-Filebeam-Development=true' "$desktop_entry"; then
    printf 'Refusing to replace non-development launcher: %s\n' "$desktop_entry" >&2
    exit 1
fi
mkdir -p "$applications"
for size in 16 32 48 64 128 256 512 1024; do
    icon_dir="$share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    install -m 0644 "$root/desktop/packaging/icons/png/$size.png" "$icon_dir/io.filebeam.desktop.png"
done
cat >"$desktop_entry" <<EOF
[Desktop Entry]
Type=Application
Name=Filebeam
Exec=$root/scripts/desktop/launch-dev.sh %u
Icon=io.filebeam.desktop
StartupWMClass=io.filebeam.desktop
MimeType=x-scheme-handler/filebeam;
Categories=Network;
X-Filebeam-Development=true
EOF
update-desktop-database "$applications" 2>/dev/null || :
gtk-update-icon-cache -f -t "$share/icons/hicolor" 2>/dev/null || :
printf 'Installed the Filebeam development launcher and hicolor icons.\n'
