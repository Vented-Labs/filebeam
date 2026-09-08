#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s TAG COMMIT RELEASE_DIRECTORY\n' "$0" >&2
    exit 64
}

[[ $# -eq 3 ]] || usage
tag=$1
commit=$2
release_dir=$3
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
zip="$release_dir/filebeam-$tag.zip"
metadata="$release_dir/release.json"

[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf 'Invalid release tag: %s\n' "$tag" >&2; exit 64; }
[[ $commit =~ ^[0-9a-f]{40}$ ]] || { printf 'Invalid release commit.\n' >&2; exit 64; }
[[ -f $zip && -f $metadata ]] || { printf 'Release ZIP and metadata are required.\n' >&2; exit 1; }
command -v gh >/dev/null
command -v php >/dev/null
[[ ${GH_REPO:-} =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] || { printf 'GH_REPO must be an OWNER/REPOSITORY value.\n' >&2; exit 1; }

php "$root/scripts/release/verify-release.php" "$metadata" "$tag" "$commit"
# The PHP program intentionally contains literal dollar-prefixed variables.
# shellcheck disable=SC2016
read -r expected_sha expected_size < <(php -r '
    $release = json_decode(file_get_contents($argv[1]), true, flags: JSON_THROW_ON_ERROR);
    $package = $release["package"] ?? null;
    $expectedPath = "versions/{$argv[2]}/filebeam-{$argv[2]}.zip";
    if (!is_array($package) || ($package["path"] ?? null) !== $expectedPath || !is_string($package["sha256"] ?? null) || !preg_match("/^[a-f0-9]{64}$/", $package["sha256"]) || !is_int($package["size"] ?? null) || $package["size"] < 1) {
        exit(1);
    }
    printf("%s %d\n", $package["sha256"], $package["size"]);
' "$metadata" "$tag")
[[ $(sha256sum "$zip" | cut -d' ' -f1) == "$expected_sha" ]] || { printf 'Release ZIP digest does not match release.json.\n' >&2; exit 1; }
[[ $(stat --format=%s "$zip") == "$expected_size" ]] || { printf 'Release ZIP size does not match release.json.\n' >&2; exit 1; }
metadata_sha=$(sha256sum "$metadata" | cut -d' ' -f1)
error=$(mktemp "${TMPDIR:-/tmp}/filebeam-github-release.XXXXXX")
trap 'rm -f "$error"' EXIT

remote_tag_commit() {
    local object object_type object_sha depth
    object=$(gh api "repos/$GH_REPO/git/ref/tags/$tag" --jq '.object.type + "\t" + .object.sha' 2>"$error") || {
        cat "$error" >&2
        exit 1
    }
    read -r object_type object_sha <<<"$object"

    for ((depth = 0; depth < 10; depth++)); do
        case $object_type in
            commit)
                printf '%s\n' "$object_sha"
                return
                ;;
            tag)
                object=$(gh api "repos/$GH_REPO/git/tags/$object_sha" --jq '.object.type + "\t" + .object.sha' 2>"$error") || {
                    cat "$error" >&2
                    exit 1
                }
                read -r object_type object_sha <<<"$object"
                ;;
            *)
                printf 'Remote tag %s does not resolve to a commit.\n' "$tag" >&2
                exit 1
                ;;
        esac
    done

    printf 'Remote tag %s has too many nested tag objects.\n' "$tag" >&2
    exit 1
}

assert_remote_tag() {
    local remote_commit
    remote_commit=$(remote_tag_commit)
    [[ $remote_commit == "$commit" ]] || { printf 'Remote tag %s no longer points at the verified commit.\n' "$tag" >&2; exit 1; }
}

assert_remote_tag
if release=$(gh release view "$tag" --json isDraft 2>"$error"); then
    if [[ $release =~ \"isDraft\"[[:space:]]*:[[:space:]]*false ]]; then
        download_dir=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-github-release-asset.XXXXXX")
        trap 'rm -f "$error"; rm -rf "$download_dir"' EXIT
        gh release download "$tag" --pattern "filebeam-$tag.zip" --dir "$download_dir"
        gh release download "$tag" --pattern release.json --dir "$download_dir"
        [[ -f "$download_dir/filebeam-$tag.zip" ]] || { printf 'Published release is missing its ZIP asset.\n' >&2; exit 1; }
        [[ $(sha256sum "$download_dir/filebeam-$tag.zip" | cut -d' ' -f1) == "$expected_sha" ]] || { printf 'Published release ZIP digest differs from the verified artifact.\n' >&2; exit 1; }
        [[ -f "$download_dir/release.json" ]] || { printf 'Published release is missing its metadata asset.\n' >&2; exit 1; }
        [[ $(sha256sum "$download_dir/release.json" | cut -d' ' -f1) == "$metadata_sha" ]] || { printf 'Published release metadata differs from the verified artifact.\n' >&2; exit 1; }
        exit 0
    fi
    [[ $release =~ \"isDraft\"[[:space:]]*:[[:space:]]*true ]] || { printf 'Could not determine GitHub release draft state.\n' >&2; exit 1; }
elif [[ $(<"$error") == *'release not found'* || $(<"$error") == *'HTTP 404'* ]]; then
    gh release create "$tag" --draft --verify-tag --target "$commit" --generate-notes --title "$tag"
else
    cat "$error" >&2
    exit 1
fi

# Only drafts are mutable. A failed upload intentionally leaves this draft unpublished.
gh release upload "$tag" "$zip" "$metadata" --clobber
download_dir=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-github-release-asset.XXXXXX")
trap 'rm -f "$error"; rm -rf "$download_dir"' EXIT
gh release download "$tag" --pattern "filebeam-$tag.zip" --dir "$download_dir"
[[ -f "$download_dir/filebeam-$tag.zip" ]] || { printf 'Draft release is missing its ZIP asset after upload.\n' >&2; exit 1; }
[[ $(sha256sum "$download_dir/filebeam-$tag.zip" | cut -d' ' -f1) == "$expected_sha" ]] || { printf 'Draft release ZIP digest differs from the verified artifact.\n' >&2; exit 1; }
assert_remote_tag
gh release edit "$tag" --draft=false
