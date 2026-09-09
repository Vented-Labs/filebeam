#!/usr/bin/env bash
set -euo pipefail

usage() { printf 'Usage: %s beam-vX.Y.Z [output-dir]\n' "$0" >&2; exit 64; }
[[ $# -ge 1 && $# -le 2 ]] || usage
tag=$1
output_dir=${2:-dist/beam}
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || usage
for variable in R2_ENDPOINT_URL R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY RELEASE_SIGNING_KEY; do
    [[ -n ${!variable:-} ]] || { printf '%s is required.\n' "$variable" >&2; exit 1; }
done
command -v aws >/dev/null
command -v php >/dev/null
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
RELEASE_PUBLIC_KEY=$(php "$root/scripts/release/cli-public-key.php" derive)
export RELEASE_PUBLIC_KEY
[[ -f "$output_dir/checksums.txt" && -f "$output_dir/version" && -f "$output_dir/public-key" && -f "$output_dir/install.sh" ]] || { printf 'Expected CLI package output in %s.\n' "$output_dir" >&2; exit 1; }
[[ $(<"$output_dir/public-key") == "$RELEASE_PUBLIC_KEY" ]] || { printf 'CLI package public key does not match RELEASE_SIGNING_KEY.\n' >&2; exit 1; }
R2_ENDPOINT_URL=$(php "$root/scripts/release/r2-endpoint.php" "$R2_ENDPOINT_URL" "$R2_BUCKET")
tmp=$(mktemp -d "${TMPDIR:-/tmp}/beam-publish.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$RELEASE_PUBLIC_KEY|g" "$root/scripts/cli/install.sh" > "$tmp/install.sh"
cmp --silent "$output_dir/install.sh" "$tmp/install.sh" || { printf 'CLI package installer does not match the canonical release public key.\n' >&2; exit 1; }
aws_r2=(aws --endpoint-url "$R2_ENDPOINT_URL" s3api)
export AWS_DEFAULT_REGION=${AWS_DEFAULT_REGION:-auto}
export AWS_REQUEST_CHECKSUM_CALCULATION=${AWS_REQUEST_CHECKSUM_CALCULATION:-WHEN_REQUIRED}
export AWS_RESPONSE_CHECKSUM_VALIDATION=${AWS_RESPONSE_CHECKSUM_VALIDATION:-WHEN_REQUIRED}

head_object() {
    local key=$1 target=$2 error="$tmp/head-error.txt"
    if "${aws_r2[@]}" head-object --bucket "$R2_BUCKET" --key "$key" > "$target" 2> "$error"; then return 0; fi
    if [[ $(<"$error") == *'Not Found'* || $(<"$error") == *'NoSuchKey'* || $(<"$error") == *'(404)'* ]]; then return 1; fi
    printf '%s\n' "$(<"$error")" >&2; return 2
}
put_immutable() {
    local key=$1 source=$2 digest
    digest=$(sha256sum "$source" | awk '{print $1}')
    if head_object "$key" "$tmp/head.json"; then
        existing=$(php -r '$h=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); echo $h["Metadata"]["sha256"] ?? "";' "$tmp/head.json")
        [[ $existing == "$digest" ]] || { printf 'Immutable object already exists with a different digest: %s\n' "$key" >&2; exit 1; }
    elif [[ $? -eq 1 ]]; then
        "${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key "$key" --body "$source" --metadata "sha256=$digest" --cache-control 'public, max-age=31536000, immutable' --if-none-match '*' >/dev/null
    else exit 1; fi
}

version=${tag#beam-v}
release_dir="cli/versions/v$version"
for architecture in x86_64 aarch64; do
    archive="$tag-linux-$architecture.tar.gz"
    [[ -f "$output_dir/$archive" ]] || { printf 'Missing %s.\n' "$output_dir/$archive" >&2; exit 1; }
    put_immutable "$release_dir/$archive" "$output_dir/$archive"
done
put_immutable "$release_dir/checksums.txt" "$output_dir/checksums.txt"
put_immutable "$release_dir/version" "$output_dir/version"
# Immutable metadata must reproduce exactly when a failed publish job is retried.
SOURCE_DATE_EPOCH=$(git -C "$root" show -s --format=%ct HEAD) \
    php "$root/scripts/release/cli-write-release.php" "$tag" "$output_dir" "$tmp/release.json"
put_immutable "$release_dir/release.json" "$tmp/release.json"

etag=''
if head_object cli/index.json "$tmp/head-index.json"; then
    etag=$(php -r '$h=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); echo $h["ETag"];' "$tmp/head-index.json")
    "${aws_r2[@]}" get-object --bucket "$R2_BUCKET" --key cli/index.json "$tmp/current-envelope.json" >/dev/null
    php "$root/scripts/release/cli-verify-index.php" < "$tmp/current-envelope.json" > "$tmp/current-index.json"
else
    [[ $? -eq 1 ]] || exit 1
    printf '{"schema":1,"generation":0,"releases":[]}\n' > "$tmp/current-index.json"
fi
php "$root/scripts/release/cli-update-index.php" "$tmp/current-index.json" "$tmp/release.json" > "$tmp/index-payload.json"
php "$root/scripts/release/sign-index.php" < "$tmp/index-payload.json" > "$tmp/index-envelope.json"
if [[ -n $etag ]]; then
    "${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key cli/index.json --body "$tmp/index-envelope.json" --content-type application/json --cache-control 'no-cache' --if-match "$etag" >/dev/null
else
    "${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key cli/index.json --body "$tmp/index-envelope.json" --content-type application/json --cache-control 'no-cache' --if-none-match '*' >/dev/null
fi
"${aws_r2[@]}" put-object --bucket "$R2_BUCKET" --key cli/install.sh --body "$tmp/install.sh" --content-type text/x-shellscript --cache-control 'no-cache' >/dev/null
printf 'Published %s.\n' "$tag"
