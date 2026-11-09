#!/usr/bin/env sh
set -eu

daemon=${1:?Path to the verified filebeamd binary is required}
version=${2:?Release version is required}
home=${FILEBEAM_HOME:-"${HOME:?HOME is required}/.filebeam"}
unit_dir=${XDG_CONFIG_HOME:-"$HOME/.config"}/systemd/user
service=filebeam-daemon
if [ "$home" != "$HOME/.filebeam" ]; then
    if command -v sha256sum >/dev/null 2>&1; then suffix=$(printf '%s' "$home" | sha256sum | cut -c1-12)
    else suffix=$(printf '%s' "$home" | shasum -a 256 | cut -c1-12); fi
    service="$service-$suffix"
fi

daemon=$("$(dirname "$0")/install-component.sh" "$daemon" "$version" cli)
case "$daemon$home" in *'"'*|*'\'*|*'
'*) printf '%s\n' 'daemon and profile paths contain unsupported systemd characters' >&2; exit 1;; esac
mkdir -p "$unit_dir"
umask 077
cat >"$unit_dir/$service.service" <<EOF
[Unit]
Description=Filebeam local device daemon

[Service]
Type=simple
ExecStart="$daemon" --home "$home"
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now "$service.service"
printf '%s\n' 'Installed and started the per-user Filebeam daemon service.'
