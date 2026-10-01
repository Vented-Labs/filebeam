#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 || ( $# -eq 1 && $1 == --refresh-index ) ]] || { printf 'Usage: %s vX.Y.Z DIRECTORY|--refresh-index\n' "$0" >&2; exit 64; }
refresh=false; [[ $1 == --refresh-index ]] && refresh=true
tag=${1:-} out=${2:-}; root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
[[ $refresh == true || $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 64
for v in R2_ENDPOINT_URL R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY; do [[ -n ${!v:-} ]] || { printf '%s is required\n' "$v" >&2; exit 1; }; done
command -v aws >/dev/null; endpoint=$(php "$root/scripts/release/r2-endpoint.php" "$R2_ENDPOINT_URL" "$R2_BUCKET"); api=(aws --endpoint-url "$endpoint" s3api); tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
head() { "${api[@]}" head-object --bucket "$R2_BUCKET" --key "$1" >"$2" 2>"$tmp/error"; }
put() { local key=$1 file=$2 sha; sha=$(sha256sum "$file"|cut -d' ' -f1); if head "$key" "$tmp/head"; then [[ $(php -r '$x=json_decode(file_get_contents($argv[1]),true);echo $x["Metadata"]["sha256"]??"";' "$tmp/head") == "$sha" ]] || { printf 'Immutable object differs: %s\n' "$key" >&2; exit 1; }; else "${api[@]}" put-object --bucket "$R2_BUCKET" --key "$key" --body "$file" --metadata "sha256=$sha" --cache-control 'public, max-age=31536000, immutable' --if-none-match '*' >/dev/null; fi; }
if [[ $refresh == true ]]; then
    head desktop/index.json "$tmp/head-index" || { printf 'Desktop index does not exist.\n' >&2; exit 1; }
    etag=$(php -r '$x=json_decode(file_get_contents($argv[1]),true);echo $x["ETag"];' "$tmp/head-index")
    "${api[@]}" get-object --bucket "$R2_BUCKET" --key desktop/index.json "$tmp/envelope" >/dev/null
    php "$root/scripts/release/cli-verify-index.php" <"$tmp/envelope" >"$tmp/current.json"
    php "$root/scripts/release/desktop-update-index.php" "$tmp/current.json" --refresh >"$tmp/index.json"
    php "$root/scripts/release/sign-index.php" <"$tmp/index.json" >"$tmp/envelope"
    "${api[@]}" put-object --bucket "$R2_BUCKET" --key desktop/index.json --body "$tmp/envelope" --content-type application/json --cache-control no-cache --if-match "$etag" >/dev/null
    exit 0
fi
SOURCE_DATE_EPOCH=$(git -C "$root" show -s --format=%ct HEAD) php "$root/scripts/release/desktop-write-release.php" "$tag" "$out" "$tmp/release.json"
mapfile -t asset_paths < <(php -r '$release=json_decode(file_get_contents($argv[1]),true,flags:JSON_THROW_ON_ERROR); foreach ($release["assets"] as $asset) echo $asset["path"]."\n";' "$tmp/release.json")
for path in "${asset_paths[@]}"; do put "desktop/$path" "$out/${path##*/}"; done
put "desktop/versions/$tag/release.json" "$tmp/release.json"
etag=''; if head desktop/index.json "$tmp/head-index"; then etag=$(php -r '$x=json_decode(file_get_contents($argv[1]),true);echo $x["ETag"];' "$tmp/head-index"); "${api[@]}" get-object --bucket "$R2_BUCKET" --key desktop/index.json "$tmp/envelope" >/dev/null; php "$root/scripts/release/cli-verify-index.php" <"$tmp/envelope" >"$tmp/current.json"; else printf '{"schema":1,"product":"desktop","generation":0,"releases":[]}\n' >"$tmp/current.json"; fi
php "$root/scripts/release/desktop-update-index.php" "$tmp/current.json" "$tmp/release.json" >"$tmp/index.json"; php "$root/scripts/release/sign-index.php" <"$tmp/index.json" >"$tmp/envelope"
if [[ -n $etag ]]; then "${api[@]}" put-object --bucket "$R2_BUCKET" --key desktop/index.json --body "$tmp/envelope" --content-type application/json --cache-control no-cache --if-match "$etag" >/dev/null; else "${api[@]}" put-object --bucket "$R2_BUCKET" --key desktop/index.json --body "$tmp/envelope" --content-type application/json --cache-control no-cache --if-none-match '*' >/dev/null; fi
