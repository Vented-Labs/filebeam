#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-validate-ref.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

remote="$tmp/remote.git"
repo="$tmp/repo"
git init --bare --initial-branch=master "$remote" >/dev/null
git clone "$remote" "$repo" >/dev/null 2>&1
export GIT_AUTHOR_NAME=test GIT_COMMITTER_NAME=test
export GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_EMAIL=test@example.invalid
mkdir -p "$repo/scripts/release"
cp "$root/scripts/release/validate-ref.sh" "$root/scripts/release/semver.php" "$repo/scripts/release/"
chmod +x "$repo/scripts/release/validate-ref.sh"
touch "$repo/initial"
git -C "$repo" add .
git -C "$repo" commit -m initial >/dev/null
git -C "$repo" push -u origin master >/dev/null
git -C "$repo" tag v1.2.3
git -C "$repo" push origin v1.2.3 >/dev/null

tag_commit=$(git -C "$repo" rev-parse HEAD)
[[ $(cd "$repo" && scripts/release/validate-ref.sh v1.2.3) == "$tag_commit" ]]

git -C "$repo" checkout -b unmerged >/dev/null
touch "$repo/unmerged"
git -C "$repo" add unmerged
git -C "$repo" commit -m unmerged >/dev/null
git -C "$repo" branch v1.2.3
[[ $(cd "$repo" && scripts/release/validate-ref.sh v1.2.3) == "$tag_commit" ]]
git -C "$repo" tag v1.2.4
if (cd "$repo" && scripts/release/validate-ref.sh v1.2.4 >/dev/null 2>&1); then
    printf 'Expected an unmerged tag to fail validation.\n' >&2
    exit 1
fi

if (cd "$repo" && scripts/release/validate-ref.sh v01.2.3 >/dev/null 2>&1); then
    printf 'Expected an invalid SemVer tag to fail validation.\n' >&2
    exit 1
fi
