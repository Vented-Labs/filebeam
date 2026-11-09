#!/usr/bin/env sh
set -eu
home=${FILEBEAM_HOME:-"${HOME:?HOME is required}/.filebeam"}
name=daemon
if [ "$home" != "$HOME/.filebeam" ]; then
    if command -v sha256sum >/dev/null 2>&1; then suffix=$(printf '%s' "$home" | sha256sum | cut -c1-12)
    else suffix=$(printf '%s' "$home" | shasum -a 256 | cut -c1-12); fi
    name="$name-$suffix"
fi
label=io.filebeam.$name
plist="${HOME:?HOME is required}/Library/LaunchAgents/$label.plist"
result=$("$(dirname "$0")/install-component.sh" --remove desktop)
if [ "$result" = last ]; then
    launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || :
    rm -f "$plist"
fi
