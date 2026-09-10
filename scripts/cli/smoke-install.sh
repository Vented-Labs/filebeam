#!/usr/bin/env bash
set -euo pipefail

tag=${1:?CLI tag required}
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || exit 64
version=${tag#beam-v}
base_url=${BEAM_RELEASE_BASE_URL:-https://releases.filebeam.io/cli}
temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-install-smoke.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
mkdir "$temporary/home"
curl --fail --silent --show-error --retry 6 --retry-all-errors --retry-delay 5 "$base_url/install.sh" \
    | HOME="$temporary/home" BEAM_RELEASE_BASE_URL="$base_url" sh
binary="$temporary/home/.filebeam/bin/beam"
[[ $("$binary" --version) == "beam $version" ]]
printf 'check_updates = false\n' > "$temporary/home/.filebeam/config.toml"
[[ $(env -u FILEBEAM_INSTANCE "$binary" --home "$temporary/home/.filebeam" --plain) == $'beam '"$version"$'\nhttps://filebeam.io' ]]
[[ ! -e "$temporary/home/.filebeam/bin/install.sh" ]]
printf 'Published installer smoke passed for %s.\n' "$tag"
