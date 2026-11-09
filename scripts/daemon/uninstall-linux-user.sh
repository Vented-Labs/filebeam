#!/usr/bin/env sh
set -eu

unit_dir=${XDG_CONFIG_HOME:-"${HOME:?HOME is required}/.config"}/systemd/user
home=${FILEBEAM_HOME:-"$HOME/.filebeam"}
service=filebeam-daemon
if [ "$home" != "$HOME/.filebeam" ]; then
    if command -v sha256sum >/dev/null 2>&1; then suffix=$(printf '%s' "$home" | sha256sum | cut -c1-12)
    else suffix=$(printf '%s' "$home" | shasum -a 256 | cut -c1-12); fi
    service="$service-$suffix"
fi
unit="$unit_dir/$service.service"
result=$("$(dirname "$0")/install-component.sh" --remove cli)
if [ "$result" = last ]; then
    systemctl --user disable --now "$service.service" 2>/dev/null || :
    rm -f "$unit"
    systemctl --user daemon-reload 2>/dev/null || :
fi
# Deliberately retain profile state, versioned daemon binaries, and dropzones.
