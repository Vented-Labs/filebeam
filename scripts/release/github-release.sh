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
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
zip="$release_dir/filebeam-$tag.zip"
metadata="$release_dir/release.json"

[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf 'Invalid release tag: %s\n' "$tag" >&2; exit 64; }
[[ $commit =~ ^[0-9a-f]{40}$ ]] || { printf 'Invalid release commit.\n' >&2; exit 64; }
[[ -f $zip && -f $metadata ]] || { printf 'Release ZIP and metadata are required.\n' >&2; exit 1; }
command -v gh >/dev/null
command -v git >/dev/null
command -v php >/dev/null
[[ -n ${GH_REPOSITORY:-} ]] || { printf 'GH_REPOSITORY is required.\n' >&2; exit 1; }

php "$root/scripts/release/verify-release.php" "$metadata" "$tag" "$commit"
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

remote_tag_commit() {
    local direct='' peeled='' object ref
    while IFS=$'\t' read -r object ref; do
        case $ref in
            "refs/tags/$tag") direct=$object ;;
            "refs/tags/$tag^{}") peeled=$object ;;
        esac
    done < <(git ls-remote --tags origin "refs/tags/$tag" "refs/tags/$tag^{}")
    [[ -n $peeled ]] && printf '%s\n' "$peeled" || printf '%s\n' "$direct"
}

assert_remote_tag() {
    local remote_commit
    remote_commit=$(remote_tag_commit)
    [[ $remote_commit == "$commit" ]] || { printf 'Remote tag %s no longer points at the verified commit.\n' "$tag" >&2; exit 1; }
}

assert_remote_tag
error=$(mktemp "${TMPDIR:-/tmp}/filebeam-github-release.XXXXXX")
trap 'rm -f "$error"' EXIT
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
elif [[ $(<"$error") == *'HTTP 404'* ]]; then
    gh release create "$tag" --draft --verify-tag --target "$commit" --generate-notes --title "$tag"
else
    <"$error" >&2
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
