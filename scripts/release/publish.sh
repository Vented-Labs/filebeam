#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s TAG [output-dir] [--metadata-only] [--refresh-index] [--security-release=true|false] [--withdrawn=true|false] [--notes-url=URL]\n' "$0" >&2
    exit 64
}

[[ $# -ge 1 ]] || usage
tag=''
if [[ $1 != --refresh-index ]]; then
    tag=$1
    shift
fi
output_dir=dist/release
metadata_only=false
refresh_index=false
security_release=__preserve__
withdrawn=__preserve__
notes_url=__preserve__
while [[ $# -gt 0 ]]; do
    case $1 in
        --metadata-only) metadata_only=true ;;
        --refresh-index) refresh_index=true; metadata_only=true ;;
        --security-release=preserve) security_release=__preserve__ ;;
        --security-release=true) security_release=true ;;
        --security-release=false) security_release=false ;;
        --withdrawn=preserve) withdrawn=__preserve__ ;;
        --withdrawn=true) withdrawn=true ;;
        --withdrawn=false) withdrawn=false ;;
        --notes-url=*) notes_url=${1#*=}; [[ -n $notes_url ]] || notes_url=__preserve__ ;;
        *) [[ $output_dir == dist/release ]] || usage; output_dir=$1 ;;
    esac
    shift
done

if [[ -n $tag ]]; then
    php "$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)/scripts/release/semver.php" "$tag" >/dev/null
elif [[ $refresh_index == false ]]; then
    usage
fi
for variable in R2_ENDPOINT_URL R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY; do
    [[ -n ${!variable:-} ]] || { printf '%s is required.\n' "$variable" >&2; exit 1; }
done
command -v aws >/dev/null
command -v php >/dev/null
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
R2_ENDPOINT_URL=$(php "$root/scripts/release/r2-endpoint.php" "$R2_ENDPOINT_URL" "$R2_BUCKET")
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-publish.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
aws_r2=(aws --endpoint-url "$R2_ENDPOINT_URL" s3api)
export AWS_DEFAULT_REGION=${AWS_DEFAULT_REGION:-auto}
export AWS_REQUEST_CHECKSUM_CALCULATION=${AWS_REQUEST_CHECKSUM_CALCULATION:-WHEN_REQUIRED}
export AWS_RESPONSE_CHECKSUM_VALIDATION=${AWS_RESPONSE_CHECKSUM_VALIDATION:-WHEN_REQUIRED}

head_object() {
    local key=$1 target=$2 error="$tmp/head-error.txt"
    if "${aws_r2[@]}" head-object --bucket "$R2_BUCKET" --key "$key" > "$target" 2> "$error"; then
        return 0
    fi
    if [[ $(<"$error") == *'Not Found'* || $(<"$error") == *'NoSuchKey'* || $(<"$error") == *'(404)'* ]]; then
        return 1
    fi
    printf '%s\n' "$(<"$error")" >&2
    return 2
}

head_index="$tmp/head-index.json"
current_index="$tmp/current-index.json"
etag=''
if head_object index.json "$head_index"; then
    etag=$(php -r '$head=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); echo $head["ETag"];' "$head_index")
    "${aws_r2[@]}" get-object --bucket "$R2_BUCKET" --key index.json "$tmp/index-envelope.json" >/dev/null
    php "$root/scripts/release/verify-index.php" < "$tmp/index-envelope.json" > "$current_index"
elif [[ $? -eq 1 ]]; then
    printf '{"schema":1,"generation":0,"releases":[]}\n' > "$current_index"
else
    exit 1
fi

release_path=-
if [[ $metadata_only == false ]]; then
    archive="$output_dir/filebeam-$tag.zip"
    release_path="$output_dir/release.json"
    [[ -f $archive && -f $release_path ]] || { printf 'Expected %s and %s.\n' "$archive" "$release_path" >&2; exit 1; }
    php -r '$release=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); if (($release["tag"] ?? null) !== $argv[2] || ($release["package"]["sha256"] ?? null) !== hash_file("sha256", $argv[3]) || (int) ($release["package"]["size"] ?? -1) !== filesize($argv[3])) { throw new RuntimeException("release.json does not match its archive"); }' "$release_path" "$tag" "$archive"
    php "$root/scripts/release/verify-archive-key.php" "$archive" "$RELEASE_PUBLIC_KEY"
    sha256=$(php -r 'echo hash_file("sha256", $argv[1]);' "$archive")
    size=$(php -r 'echo filesize($argv[1]);' "$archive")
    package_key="versions/$tag/filebeam-$tag.zip"
    release_key="versions/$tag/release.json"
    for key in "$package_key" "$release_key"; do
        source=$archive
        [[ $key == "$release_key" ]] && source=$release_path
        object_sha=$(php -r 'echo hash_file("sha256", $argv[1]);' "$source")
        if head_object "$key" "$tmp/head.json"; then
            existing=$(php -r '$head=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); echo $head["Metadata"]["sha256"] ?? "";' "$tmp/head.json")
            [[ $existing == "$object_sha" ]] || { printf 'Immutable object already exists with a different digest: %s\n' "$key" >&2; exit 1; }
        else
            head_status=$?
            if [[ $head_status -ne 1 ]]; then
                exit 1
            fi
            "${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key "$key" --body "$source" --metadata "sha256=$object_sha" --cache-control 'public, max-age=31536000, immutable' --if-none-match '*' >/dev/null
        fi
    done
    [[ $(php -r 'echo filesize($argv[1]);' "$archive") == "$size" ]]
fi

index_tag=$tag
[[ $refresh_index == true ]] && index_tag=__refresh__
php "$root/scripts/release/update-index.php" "$current_index" "$release_path" "$security_release" "$withdrawn" "$notes_url" "$index_tag" > "$tmp/index-payload.json"
php "$root/scripts/release/sign-index.php" < "$tmp/index-payload.json" > "$tmp/index-envelope.json"
if [[ -n $etag ]]; then
    "${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key index.json --body "$tmp/index-envelope.json" --content-type application/json --cache-control 'no-cache, max-age=300' --if-match "$etag" >/dev/null
else
    "${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key index.json --body "$tmp/index-envelope.json" --content-type application/json --cache-control 'no-cache, max-age=300' --if-none-match '*' >/dev/null
fi
printf 'Published signed index generation %s for %s\n' "$(php -r '$i=json_decode(file_get_contents($argv[1]), true); echo $i["generation"];' "$tmp/index-payload.json")" "$tag"
