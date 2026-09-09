#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-release-test.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
git -C "$temporary" init --quiet
printf 'fixture\n' > "$temporary/source"
git -C "$temporary" add source
git -C "$temporary" -c user.name=Fixture -c user.email=fixture@example.test commit --quiet -m 'fixture'
commit=$(git -C "$temporary" rev-parse HEAD)
git -C "$temporary" update-ref refs/remotes/origin/master "$commit"
git -C "$temporary" tag v0.2.0
git -C "$temporary" tag beam-v0.2.0
for tag in v0.2.0 beam-v0.2.0; do
    [[ $(cd "$temporary" && bash "$root/scripts/release/cli-validate-ref.sh" "$tag") == "$commit" ]]
done
for tag in v00.2.0 v0.2.0-beta branch-name missing; do
    if (cd "$temporary" && bash "$root/scripts/release/cli-validate-ref.sh" "$tag" >/dev/null 2>&1); then
        printf 'Accepted invalid CLI source %s.\n' "$tag" >&2; exit 1
    fi
done
printf 'feature\n' >> "$temporary/source"
git -C "$temporary" add source
git -C "$temporary" -c user.name=Fixture -c user.email=fixture@example.test commit --quiet -m 'feature'
git -C "$temporary" tag v0.3.0
if (cd "$temporary" && bash "$root/scripts/release/cli-validate-ref.sh" v0.3.0 >/dev/null 2>&1); then
    printf 'Accepted a release outside master.\n' >&2; exit 1
fi
printf 'CLI master release tag validation passed.\n'
