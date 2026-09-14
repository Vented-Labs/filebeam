#!/usr/bin/env bash
set -euo pipefail

usage() { printf 'Usage: %s TAG COMMIT RELEASE_DIRECTORY [CLI_RELEASE_DIRECTORY]\n' "$0" >&2; exit 64; }
[[ $# -ge 3 && $# -le 4 ]] || usage
tag=$1
commit=$2
release_dir=$3
cli_dir=${4:-}
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf 'Invalid release tag: %s\n' "$tag" >&2; exit 64; }
[[ $commit =~ ^[0-9a-f]{40}$ ]] || { printf 'Invalid release commit.\n' >&2; exit 64; }
[[ ${GH_REPO:-} =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] || { printf 'GH_REPO must be an OWNER/REPOSITORY value.\n' >&2; exit 1; }
command -v gh >/dev/null
command -v php >/dev/null
temporary=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-github-release.XXXXXX")
error=''
download_dir=''
trap 'rm -rf "$temporary" "$download_dir"; rm -f "$error"' EXIT

zip="$release_dir/filebeam-$tag.zip"
metadata="$release_dir/release.json"
[[ -f $zip && -f $metadata ]] || { printf 'Release ZIP and metadata are required.\n' >&2; exit 1; }
php "$root/scripts/release/verify-release.php" "$metadata" "$tag" "$commit"

assets=("$zip" "$metadata")
cli_manifest=''
if [[ -n $cli_dir ]]; then
    cli_tag="beam-$tag"
    cli_assets=(
        "$cli_dir/$cli_tag-linux-x86_64.tar.gz" "$cli_dir/$cli_tag-linux-aarch64.tar.gz"
        "$cli_dir/$cli_tag-macos-x86_64.tar.gz" "$cli_dir/$cli_tag-macos-aarch64.tar.gz"
        "$cli_dir/$cli_tag-windows-x86_64.zip" "$cli_dir/checksums.txt" "$cli_dir/version"
        "$cli_dir/public-key" "$cli_dir/install.sh" "$cli_dir/install.ps1" "$cli_dir/release.json"
    )
    for asset in "${cli_assets[@]}"; do [[ -f $asset ]] || { printf 'Missing CLI release asset: %s\n' "$asset" >&2; exit 1; }; done
    [[ $(<"$cli_dir/version") == "${tag#v}" ]] || { printf 'CLI version does not match %s.\n' "$tag" >&2; exit 1; }
    (cd "$cli_dir" && sha256sum --check --strict checksums.txt)
    # The CLI catalog manifest binds every platform archive to its exact bytes.
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
    ' "$cli_dir/release.json" "$cli_tag" || { printf 'CLI release manifest does not verify every archive.\n' >&2; exit 1; }
    # GitHub release assets share one namespace; retain the catalog filename locally.
    cli_manifest="$temporary/$cli_tag-release.json"
    cp "$cli_dir/release.json" "$cli_manifest"
    assets+=("${cli_assets[@]:0:10}" "$cli_manifest")
fi

error="$temporary/error"
download_dir="$temporary/download"
mkdir "$download_dir"

remote_tag_commit() {
    local object object_type object_sha depth
    object=$(gh api "repos/$GH_REPO/git/ref/tags/$tag" --jq '.object.type + "\t" + .object.sha' 2>"$error") || { cat "$error" >&2; exit 1; }
    read -r object_type object_sha <<<"$object"
    for ((depth = 0; depth < 10; depth++)); do
        case $object_type in
            commit) printf '%s\n' "$object_sha"; return ;;
            tag) object=$(gh api "repos/$GH_REPO/git/tags/$object_sha" --jq '.object.type + "\t" + .object.sha' 2>"$error") || { cat "$error" >&2; exit 1; }; read -r object_type object_sha <<<"$object" ;;
            *) printf 'Remote tag %s does not resolve to a commit.\n' "$tag" >&2; exit 1 ;;
        esac
    done
    printf 'Remote tag %s has too many nested tag objects.\n' "$tag" >&2; exit 1
}
assert_remote_tag() { [[ $(remote_tag_commit) == "$commit" ]] || { printf 'Remote tag %s no longer points at the verified commit.\n' "$tag" >&2; exit 1; }; }
verify_remote_assets() {
    local published=${1:-false} asset name
    for asset in "${assets[@]}"; do
        name=${asset##*/}
        rm -f "$download_dir/$name"
        gh release download "$tag" --pattern "$name" --dir "$download_dir" || {
            if [[ $published == true ]]; then
                printf 'Release is missing immutable asset %s. Recovery requires a new release tag; this script will not upload to, delete, or recreate a published release.\n' "$name" >&2
            else
                printf 'Release asset download failed: %s\n' "$name" >&2
            fi
            return 1
        }
        [[ -f "$download_dir/$name" ]] || { printf 'Release is missing immutable asset %s. Recovery requires a new release tag; this script will not upload to, delete, or recreate a published release.\n' "$name" >&2; return 1; }
        cmp --silent "$asset" "$download_dir/$name" || { printf 'Release asset differs from verified bytes: %s\n' "$name" >&2; return 1; }
    done
}

assert_remote_tag
if release=$(gh release view "$tag" --json isDraft 2>"$error"); then
    if [[ $release =~ \"isDraft\"[[:space:]]*:[[:space:]]*false ]]; then
        verify_remote_assets true
        assert_remote_tag
        exit 0
    fi
    [[ $release =~ \"isDraft\"[[:space:]]*:[[:space:]]*true ]] || { printf 'Could not determine GitHub release draft state.\n' >&2; exit 1; }
elif [[ $(<"$error") == *'release not found'* || $(<"$error") == *'HTTP 404'* ]]; then
    gh release create "$tag" --draft --verify-tag --target "$commit" --generate-notes --title "$tag"
else
    cat "$error" >&2; exit 1
fi

# A draft is the only mutable state. Upload every release asset before any verification or publish.
gh release upload "$tag" "${assets[@]}" --clobber
verify_remote_assets
assert_remote_tag
gh release edit "$tag" --draft=false
