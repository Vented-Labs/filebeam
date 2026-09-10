#!/usr/bin/env bash
set -euo pipefail

[[ $# -ge 3 && $# -le 4 ]] || { printf 'Usage: %s CLI_TAG COMMIT DIRECTORY [RELEASE_TAG]\n' "$0" >&2; exit 64; }
tag=$1
commit=$2
directory=$3
release_tag=${4:-$tag}
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || exit 64
[[ $release_tag =~ ^(beam-)?v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || exit 64
[[ $commit =~ ^[a-f0-9]{40}$ ]] || exit 64
[[ $(git rev-parse HEAD) == "$commit" ]] || exit 1
existing=$(gh api "repos/{owner}/{repo}/commits/$release_tag" --jq .sha)
[[ $existing == "$commit" ]] || { printf 'Release tag points to another commit.\n' >&2; exit 1; }

if ! gh release view "$release_tag" >/dev/null 2>&1; then
    [[ $release_tag == "$tag" ]] || { printf 'Application GitHub release does not exist.\n' >&2; exit 1; }
    gh release create "$release_tag" --verify-tag --target "$commit" \
        --title "Beam CLI ${tag#beam-}" --latest=false \
        --notes "Signed Linux, macOS, and Windows CLI release from Filebeam commit $commit. Install from https://releases.filebeam.io/cli/. Full download URLs select their own instance; bare ULIDs default to https://filebeam.io."
fi

assets=(
    "$directory/$tag-linux-x86_64.tar.gz"
    "$directory/$tag-linux-aarch64.tar.gz"
    "$directory/$tag-macos-x86_64.tar.gz"
    "$directory/$tag-macos-aarch64.tar.gz"
    "$directory/$tag-windows-x86_64.zip"
    "$directory/checksums.txt"
    "$directory/install.sh"
    "$directory/install.ps1"
)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-github-release.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
for asset in "${assets[@]}"; do
    [[ -f $asset ]] || { printf 'Missing CLI release asset: %s\n' "$asset" >&2; exit 1; }
    name=${asset##*/}
    if gh release view "$release_tag" --json assets --jq '.assets[].name' | grep -Fxq "$name"; then
        gh release download "$release_tag" --pattern "$name" --dir "$temporary"
        cmp --silent "$asset" "$temporary/$name" || { printf 'Published CLI asset differs: %s\n' "$name" >&2; exit 1; }
        rm "$temporary/$name"
    else
        gh release upload "$release_tag" "$asset"
    fi
done
printf 'Published CLI assets on GitHub release %s.\n' "$release_tag"
