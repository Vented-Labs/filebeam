#!/usr/bin/env bash
set -euo pipefail

tag=${1:?Usage: cli-validate-ref.sh vX.Y.Z-or-beam-vX.Y.Z}
[[ $tag =~ ^(beam-)?v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || {
    printf 'A strict release tag is required.\n' >&2; exit 64;
}
commit=$(git rev-parse --verify "refs/tags/$tag^{commit}")
git merge-base --is-ancestor "$commit" refs/remotes/origin/master || {
    printf 'CLI releases must come from master.\n' >&2; exit 1;
}
printf '%s\n' "$commit"
