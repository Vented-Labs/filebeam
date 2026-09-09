#!/usr/bin/env bash
set -euo pipefail

for variable in R2_ENDPOINT_URL R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY RELEASE_SIGNING_KEY; do
    [[ -n ${!variable:-} ]] || { printf '%s is required.\n' "$variable" >&2; exit 1; }
done
command -v aws >/dev/null
command -v php >/dev/null

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
RELEASE_PUBLIC_KEY=$(php "$root/scripts/release/cli-public-key.php" derive)
export RELEASE_PUBLIC_KEY
R2_ENDPOINT_URL=$(php "$root/scripts/release/r2-endpoint.php" "$R2_ENDPOINT_URL" "$R2_BUCKET")
tmp=$(mktemp -d "${TMPDIR:-/tmp}/beam-refresh.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
export AWS_DEFAULT_REGION=${AWS_DEFAULT_REGION:-auto}
export AWS_REQUEST_CHECKSUM_CALCULATION=${AWS_REQUEST_CHECKSUM_CALCULATION:-WHEN_REQUIRED}
export AWS_RESPONSE_CHECKSUM_VALIDATION=${AWS_RESPONSE_CHECKSUM_VALIDATION:-WHEN_REQUIRED}
aws_r2=(aws --endpoint-url "$R2_ENDPOINT_URL" s3api)

"${aws_r2[@]}" head-object --bucket "$R2_BUCKET" --key cli/index.json > "$tmp/head.json"
etag=$(php -r '$h=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); echo $h["ETag"];' "$tmp/head.json")
"${aws_r2[@]}" get-object --bucket "$R2_BUCKET" --key cli/index.json "$tmp/current-envelope.json" >/dev/null
php "$root/scripts/release/cli-verify-index.php" < "$tmp/current-envelope.json" > "$tmp/current-index.json"
php "$root/scripts/release/cli-update-index.php" "$tmp/current-index.json" > "$tmp/index-payload.json"
php "$root/scripts/release/sign-index.php" < "$tmp/index-payload.json" > "$tmp/index-envelope.json"
"${aws_r2[@]}" put-object \
    --bucket "$R2_BUCKET" \
    --key cli/index.json \
    --body "$tmp/index-envelope.json" \
    --content-type application/json \
    --cache-control 'no-cache' \
    --if-match "$etag" >/dev/null

printf '%s\n' 'Refreshed signed CLI catalog.'
