#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 1 ]] || { printf 'Usage: %s ARCHIVE\n' "$0" >&2; exit 64; }
archive=$1
[[ -f $archive ]] || { printf 'Archive not found: %s\n' "$archive" >&2; exit 1; }
command -v unzip >/dev/null
command -v php >/dev/null
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-smoke.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

while IFS= read -r path; do
    [[ $path == filebeam/* ]] || { printf 'Archive member outside filebeam/: %s\n' "$path" >&2; exit 1; }
    case $path in
        */.env.example) ;;
        */.env|*/.env.*|*.sqlite|*.sqlite-*|*.db|*.db-*|*/public/hot|*/public/hot/*|*/public/storage|*/public/storage/*)
            printf 'Archive includes mutable or secret path: %s\n' "$path" >&2
            exit 1
            ;;
    esac
done < <(unzip -Z1 "$archive")
unzip -q "$archive" -d "$tmp"
stage="$tmp/filebeam"
[[ -f $stage/package-files.json && -f $stage/backend/artisan && -f $stage/update.php ]] || { printf 'Archive is missing required package files.\n' >&2; exit 1; }
php "$root/scripts/release/manifest.php" "$stage" > "$tmp/actual-manifest.json"
php -r '$expected=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); $actual=json_decode(file_get_contents($argv[2]), true, 512, JSON_THROW_ON_ERROR); if ($expected !== $actual) { throw new RuntimeException("Package manifest does not match archive contents."); }' "$stage/package-files.json" "$tmp/actual-manifest.json"
[[ ! -e $stage/backend/vendor/bin/pest && ! -e $stage/backend/node_modules ]] || { printf 'Archive contains development tooling.\n' >&2; exit 1; }

if [[ ${FILEBEAM_SMOKE_MIGRATE:-true} == true ]]; then
    cp "$stage/backend/.env.example" "$stage/backend/.env"
    database="$stage/backend/database/smoke.sqlite"
    : > "$database"
    (cd "$stage/backend" && DB_CONNECTION=sqlite DB_DATABASE="$database" php artisan migrate --force --no-interaction)
    rm -f "$stage/backend/.env" "$database"
fi
printf 'Archive smoke test passed: %s\n' "$archive"
