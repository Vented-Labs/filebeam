#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s TAG [output-dir] [--skip-build] [--allow-unsigned-local] [--allow-local-source]\n' "$0" >&2
    exit 64
}

[[ $# -ge 1 ]] || usage
tag=$1
shift
output_dir=dist/release
skip_build=false
allow_unsigned_local=false
allow_local_source=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build) skip_build=true ;;
        --allow-unsigned-local) allow_unsigned_local=true ;;
        --allow-local-source) allow_local_source=true ;;
        *) [[ $output_dir == dist/release ]] || usage; output_dir=$1 ;;
    esac
    shift
done

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
version=$(php "$root/scripts/release/semver.php" "$tag")
release_min_php=${RELEASE_MIN_PHP:-8.5}
if ! php -r 'exit(version_compare($argv[1], "8.5", "<") ? 1 : 0);' "$release_min_php"; then
    printf 'RELEASE_MIN_PHP must be PHP 8.5 or newer.\n' >&2
    exit 64
fi
output_dir=$(mkdir -p "$output_dir" && CDPATH= cd -- "$output_dir" && pwd)
command -v php >/dev/null
command -v composer >/dev/null
command -v zip >/dev/null

if [[ $skip_build == false ]]; then
    (cd "$root" && npm run build)
fi

[[ -f "$root/update.php" && -d "$root/updater" ]] || { printf 'update.php and updater/ must exist before packaging.\n' >&2; exit 1; }
for entry in LICENSE SECURITY.md README.md docs/deployment.md; do
    [[ -f "$root/$entry" ]] || { printf 'Required release file is missing: %s\n' "$entry" >&2; exit 1; }
done
[[ -f "$root/backend/composer.lock" ]] || { printf 'backend/composer.lock is required.\n' >&2; exit 1; }

commit=$(git -C "$root" rev-parse --verify HEAD 2>/dev/null || true)
if [[ -z $commit ]]; then
    [[ $allow_local_source == true ]] || { printf 'A Git commit is required for release packages; use --allow-local-source only for local testing.\n' >&2; exit 1; }
    commit=local-source
fi
built_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
stage_base=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-package.XXXXXX")
stage="$stage_base/filebeam"
cleanup() { rm -rf "$stage_base"; }
trap cleanup EXIT
mkdir -p "$stage/backend"
cp -a "$root/LICENSE" "$root/SECURITY.md" "$root/README.md" "$stage/"
mkdir -p "$stage/docs"
cp -a "$root/docs/deployment.md" "$stage/docs/deployment.md"

copy_entry() {
    local entry=$1
    [[ -e "$root/backend/$entry" ]] || return 0
    cp -a "$root/backend/$entry" "$stage/backend/$entry"
}

# This is the complete distributable backend surface. Development and mutable data
# are never copied; Composer installs production dependencies into the stage.
for entry in app bootstrap config database public resources routes artisan composer.json composer.lock .env.example; do
    copy_entry "$entry"
done
rm -rf "$stage/backend/vendor" "$stage/backend/node_modules" "$stage/backend/tests" "$stage/backend/.agents" "$stage/backend/.zed" "$stage/backend/docker"
rm -rf "$stage/backend/public/storage" "$stage/backend/public/hot" "$stage/backend/storage"
find "$stage/backend/database" -type f \( -name '*.sqlite' -o -name '*.sqlite-*' -o -name '*.db' -o -name '*.db-*' \) -delete 2>/dev/null || true
mkdir -p "$stage/backend/bootstrap/cache" "$stage/backend/storage/app/public" "$stage/backend/storage/framework/cache/data" "$stage/backend/storage/framework/sessions" "$stage/backend/storage/framework/views" "$stage/backend/storage/logs"
find "$stage/backend" -name '.env' -o -name '.env.*' | while IFS= read -r path; do [[ ${path##*/} == .env.example ]] || rm -f "$path"; done
cp -a "$root/update.php" "$stage/update.php"
cp -a "$root/updater" "$stage/updater"

(cd "$stage/backend" && composer install --no-dev --no-interaction --prefer-dist --optimize-autoloader --no-progress && composer check-platform-reqs --no-dev)
if ! platform_blockers=$(cd "$stage/backend" && composer prohibits php "$release_min_php"); then
    printf 'Unable to validate the production lock against PHP %s:\n%s\n' "$release_min_php" "$platform_blockers" >&2
    exit 1
fi
if [[ $platform_blockers == *' requires php '* ]]; then
    printf 'The production lock cannot run on PHP %s:\n%s\n' "$release_min_php" "$platform_blockers" >&2
    exit 1
fi
find "$stage/backend/bootstrap/cache" -mindepth 1 ! -name .gitignore -exec rm -rf {} + 2>/dev/null || true

public_key=${RELEASE_PUBLIC_KEY:-}
if [[ -n $public_key ]]; then
    php "$root/scripts/release/key.php" public "$public_key" >/dev/null
else
    [[ $allow_unsigned_local == true ]] || { printf 'RELEASE_PUBLIC_KEY is required for official signed packages. Use --allow-unsigned-local only for local testing.\n' >&2; exit 1; }
    printf 'Creating explicitly unsigned local test package.\n' >&2
fi
mkdir -p "$stage/backend/config"
php "$root/scripts/release/write-version.php" "$stage/backend/config/version.php" "$version" "$tag" "$commit" "$built_at" "$public_key"
php "$root/scripts/release/manifest.php" "$stage" > "$stage/package-files.json"

archive="$output_dir/filebeam-$tag.zip"
release="$output_dir/release.json"
[[ ! -e $archive && ! -e $release ]] || { printf 'Refusing to overwrite existing release output: %s\n' "$output_dir" >&2; exit 1; }
(cd "$stage_base" && zip -qr "$archive" filebeam)
sha256=$(php -r 'echo hash_file("sha256", $argv[1]);' "$archive")
size=$(php -r 'echo filesize($argv[1]);' "$archive")
php "$root/scripts/release/write-release.php" "$release" "$tag" "$version" "$commit" "$built_at" "versions/$tag/filebeam-$tag.zip" "$sha256" "$size" "$release_min_php"
printf 'Created %s\nCreated %s\n' "$archive" "$release"
