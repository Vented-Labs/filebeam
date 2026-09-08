#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s TAG\n' "$0" >&2
    exit 64
}

[[ $# -eq 1 ]] || usage
tag=$1
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)

# This helper is run from the trusted master checkout, so it owns SemVer parsing.
php "$root/scripts/release/semver.php" "$tag" >/dev/null

tag_ref="refs/tags/$tag"
git rev-parse --verify --quiet "$tag_ref" >/dev/null || {
    printf 'Tag does not exist: %s\n' "$tag" >&2
    exit 1
}
commit=$(git rev-parse --verify --quiet "${tag_ref}^{commit}") || {
    printf 'Tag does not resolve to a commit: %s\n' "$tag" >&2
    exit 1
}
git rev-parse --verify --quiet refs/remotes/origin/master >/dev/null || {
    printf 'origin/master is required.\n' >&2
    exit 1
}
git merge-base --is-ancestor "$commit" refs/remotes/origin/master || {
    printf 'Tag %s commit is not an ancestor of origin/master.\n' "$tag" >&2
    exit 1
}

printf '%s\n' "$commit"
