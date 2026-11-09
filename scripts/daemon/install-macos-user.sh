#!/usr/bin/env sh
set -eu

daemon=${1:?Path to the verified filebeamd binary is required}
version=${2:?Release version is required}
home=${FILEBEAM_HOME:-"${HOME:?HOME is required}/.filebeam"}
name=daemon
if [ "$home" != "$HOME/.filebeam" ]; then
    if command -v sha256sum >/dev/null 2>&1; then suffix=$(printf '%s' "$home" | sha256sum | cut -c1-12)
    else suffix=$(printf '%s' "$home" | shasum -a 256 | cut -c1-12); fi
    name="$name-$suffix"
fi
label=io.filebeam.$name
agents="$HOME/Library/LaunchAgents"
plist="$agents/$label.plist"

daemon=$("$(dirname "$0")/install-component.sh" "$daemon" "$version" desktop)
case "$daemon$home" in *'&'*|*'<'*|*'>'*|*'"'*|*"'"*) printf '%s\n' 'daemon and profile paths contain unsupported plist characters' >&2; exit 1;; esac
mkdir -p "$agents"
umask 077
launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || :
cat >"$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$daemon</string><string>--home</string><string>$home</string></array>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Background</string>
</dict></plist>
EOF
chmod 600 "$plist"
plutil -lint "$plist"
launchctl bootstrap "gui/$(id -u)" "$plist"
printf '%s\n' 'Installed and started the per-user Filebeam LaunchAgent.'
