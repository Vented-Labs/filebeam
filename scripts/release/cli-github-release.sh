#!/usr/bin/env bash
set -euo pipefail

tag=${1:?CLI tag required}
commit=${2:?Source commit required}
directory=${3:?Package directory required}
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || exit 64
[[ $commit =~ ^[a-f0-9]{40}$ ]] || exit 64
[[ $(git rev-parse HEAD) == "$commit" ]] || exit 1
if gh release view "$tag" >/dev/null 2>&1; then
    existing=$(gh api "repos/{owner}/{repo}/commits/$tag" --jq .sha)
    [[ $existing == "$commit" ]] || { printf 'CLI release tag points to another commit.\n' >&2; exit 1; }
    gh release upload "$tag" "$directory/$tag-linux-x86_64.tar.gz" "$directory/$tag-linux-aarch64.tar.gz" \
        "$directory/checksums.txt" "$directory/install.sh" --clobber
    printf 'CLI GitHub release already exists: %s.\n' "$tag"
else
    gh release create "$tag" "$directory/$tag-linux-x86_64.tar.gz" "$directory/$tag-linux-aarch64.tar.gz" \
        "$directory/checksums.txt" "$directory/install.sh" \
        --target "$commit" --title "Beam CLI ${tag#beam-}" --latest=false \
        --notes "Signed Linux x86_64 and ARM64 CLI release from Filebeam commit $commit. Install with the HTTPS shell installer at https://releases.filebeam.io/cli/install.sh. Full download URLs select their own instance; bare ULIDs default to https://filebeam.io."
fi
