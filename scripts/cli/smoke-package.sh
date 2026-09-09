#!/usr/bin/env bash
set -euo pipefail

tag=${1:?CLI tag required}
directory=${2:?Package directory required}
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || exit 64
version=${tag#beam-v}
temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-package-smoke.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
for architecture in x86_64 aarch64; do
    archive="$tag-linux-$architecture.tar.gz"
    [[ -s "$directory/$archive" ]] || exit 1
    tar -tzf "$directory/$archive" > "$temporary/entries"
    grep -Fxq beam/beam "$temporary/entries"
    [[ $(tar -xOzf "$directory/$archive" beam/version) == "$version" ]]
    tar -xOzf "$directory/$archive" beam/install.sh > "$temporary/install-$architecture.sh"
    ! grep -q '__BEAM_RELEASE_PUBLIC_KEY__' "$temporary/install-$architecture.sh"
    cmp "$directory/install.sh" "$temporary/install-$architecture.sh"
done
(cd "$directory" && sha256sum --check checksums.txt)
tar -xzf "$directory/$tag-linux-x86_64.tar.gz" -C "$temporary" beam/beam
mkdir "$temporary/home"
printf 'check_updates = false\n' > "$temporary/home/config.toml"
[[ $("$temporary/beam/beam" --version) == "beam $version" ]]
[[ $(env -u FILEBEAM_INSTANCE "$temporary/beam/beam" --home "$temporary/home" --plain) == $'beam '"$version"$'\nhttps://filebeam.io' ]]
printf 'CLI package smoke passed for %s.\n' "$tag"
