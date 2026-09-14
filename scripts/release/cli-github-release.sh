#!/usr/bin/env bash
set -euo pipefail

usage() { printf 'Usage: %s CLI_TAG COMMIT DIRECTORY\n' "$0" >&2; exit 64; }
[[ $# -eq 3 ]] || usage
tag=$1
commit=$2
directory=$3
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ && $commit =~ ^[a-f0-9]{40}$ ]] || exit 64
command -v gh >/dev/null
assets=(
    "$directory/$tag-linux-x86_64.tar.gz" "$directory/$tag-linux-aarch64.tar.gz"
    "$directory/$tag-macos-x86_64.tar.gz" "$directory/$tag-macos-aarch64.tar.gz"
    "$directory/$tag-windows-x86_64.zip" "$directory/checksums.txt" "$directory/version"
    "$directory/public-key" "$directory/install.sh" "$directory/install.ps1" "$directory/release.json"
)
for asset in "${assets[@]}"; do [[ -f $asset ]] || { printf 'Missing CLI release asset: %s\n' "$asset" >&2; exit 1; }; done
[[ $(<"$directory/version") == "${tag#beam-v}" ]] || { printf 'CLI version does not match %s.\n' "$tag" >&2; exit 1; }
(cd "$directory" && sha256sum --check --strict checksums.txt)
php -r '
    $release = json_decode(file_get_contents($argv[1]), true, flags: JSON_THROW_ON_ERROR);
    $tag = $argv[2];
    if (!is_array($release) || ($release["tag"] ?? null) !== $tag || ($release["version"] ?? null) !== substr($tag, 6) || !is_array($release["assets"] ?? null) || count($release["assets"]) !== 5) exit(1);
    $seen = [];
    foreach ($release["assets"] as $asset) {
        if (!is_array($asset) || !is_string($asset["os"] ?? null) || !is_string($asset["architecture"] ?? null) || !is_string($asset["path"] ?? null) || !is_string($asset["sha256"] ?? null) || !preg_match("/^[a-f0-9]{64}$/", $asset["sha256"]) || !is_int($asset["size"] ?? null) || $asset["size"] < 1) exit(1);
        $extension = $asset["os"] === "windows" ? "zip" : "tar.gz";
        $name = "$tag-{$asset["os"]}-{$asset["architecture"]}.$extension";
        if (isset($seen[$name]) || $asset["path"] !== "versions/v".substr($tag, 6)."/$name" || !is_file(dirname($argv[1])."/$name") || hash_file("sha256", dirname($argv[1])."/$name") !== $asset["sha256"] || filesize(dirname($argv[1])."/$name") !== $asset["size"]) exit(1);
        $seen[$name] = true;
    }
    if (count($seen) !== 5) exit(1);
' "$directory/release.json" "$tag" || { printf 'CLI release manifest does not verify every archive.\n' >&2; exit 1; }

temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-github-release.XXXXXX")
error=$(mktemp "${TMPDIR:-/tmp}/beam-github-release-error.XXXXXX")
trap 'rm -rf "$temporary"; rm -f "$error"' EXIT
assert_remote_tag() {
    local actual
    actual=$(gh api "repos/{owner}/{repo}/commits/$tag" --jq .sha 2>"$error") || { cat "$error" >&2; exit 1; }
    [[ $actual == "$commit" ]] || { printf 'Release tag points to another commit.\n' >&2; exit 1; }
}
verify_remote_assets() {
    local published=${1:-false} asset name
    for asset in "${assets[@]}"; do
        name=${asset##*/}; rm -f "$temporary/$name"
        gh release download "$tag" --pattern "$name" --dir "$temporary" || {
            if [[ $published == true ]]; then
                printf 'CLI release is missing immutable asset %s. Recovery requires a new release tag; this script will not upload to, delete, or recreate a published release.\n' "$name" >&2
            else
                printf 'CLI release asset download failed: %s\n' "$name" >&2
            fi
            return 1
        }
        [[ -f "$temporary/$name" ]] || { printf 'CLI release is missing immutable asset %s. Recovery requires a new release tag; this script will not upload to, delete, or recreate a published release.\n' "$name" >&2; return 1; }
        cmp --silent "$asset" "$temporary/$name" || { printf 'CLI release asset differs from verified bytes: %s\n' "$name" >&2; return 1; }
    done
}

assert_remote_tag
if release=$(gh release view "$tag" --json isDraft 2>"$error"); then
    if [[ $release =~ \"isDraft\"[[:space:]]*:[[:space:]]*false ]]; then verify_remote_assets true; assert_remote_tag; exit 0; fi
    [[ $release =~ \"isDraft\"[[:space:]]*:[[:space:]]*true ]] || { printf 'Could not determine GitHub release draft state.\n' >&2; exit 1; }
elif [[ $(<"$error") == *'release not found'* || $(<"$error") == *'HTTP 404'* ]]; then
    gh release create "$tag" --draft --verify-tag --target "$commit" --title "Beam CLI ${tag#beam-}" --latest=false --notes "Signed Linux, macOS, and Windows CLI release from Filebeam commit $commit."
else
    cat "$error" >&2; exit 1
fi
gh release upload "$tag" "${assets[@]}" --clobber
verify_remote_assets
assert_remote_tag
gh release edit "$tag" --draft=false
