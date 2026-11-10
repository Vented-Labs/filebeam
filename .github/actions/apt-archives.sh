#!/usr/bin/env bash
# Lets third-party setup steps reuse cached apt downloads instead of slow Ubuntu mirrors.
#   use DIRECTORY           point apt's download cache at DIRECTORY
#   finish DIRECTORY PREFIX restore apt, keep installed versions, print key=PREFIX<content hash>
set -euo pipefail

config=/etc/apt/apt.conf.d/90filebeam-archives
mode=${1:-}
directory=${2:-}
case $mode in
    use)
        [[ -n $directory ]] || exit 64
        mkdir -p "$directory"
        printf '%s\n' \
            "Dir::Cache::Archives \"$directory/\";" \
            'APT::Keep-Downloaded-Packages "true";' \
            'APT::Sandbox::User "root";' | sudo tee "$config" >/dev/null
        ;;
    finish)
        prefix=${3:?usage: $0 finish DIRECTORY PREFIX}
        sudo rm -rf "$config" "$directory/partial" "$directory/lock"
        installed=$(mktemp)
        trap 'rm -f "$installed"' EXIT
        # Archive names encode an epoch colon as %3a. Superseded versions age out of the cache.
        dpkg-query --show --showformat='${Package}_${Version}_${Architecture}.deb\n' | sed 's/:/%3a/' | LC_ALL=C sort -u > "$installed"
        find "$directory" -maxdepth 1 -name '*.deb' -printf '%f\n' | LC_ALL=C sort | comm -23 - "$installed" |
            while IFS= read -r name; do rm -f "$directory/$name"; done
        hash=$(find "$directory" -maxdepth 1 -name '*.deb' -printf '%f\n' | LC_ALL=C sort | sha256sum | cut -c1-16)
        printf 'key=%s%s\n' "$prefix" "$hash"
        ;;
    *)
        printf 'Usage: %s use DIRECTORY | finish DIRECTORY PREFIX\n' "$0" >&2
        exit 64
        ;;
esac
