#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

grep -Fq 'icons/NOTICE' "$root/scripts/release/package.sh"
grep -Fq 'ICONSAX-NOTICE' "$root/scripts/release/package.sh"
grep -Fq 'Iconsax Free License' "$root/icons/NOTICE"
grep -Fq 'validate-manifest.php' "$root/scripts/release/package.sh"
if grep -Fq '"$root/docs/social-previews.md"' "$root/scripts/release/package.sh"; then
    printf 'Protocol 1 packages must remain installable by the v0.1.0 updater.\n' >&2
    exit 1
fi

stage=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-manifest-test.XXXXXX")
cleanup() { rm -rf "$stage"; }
trap cleanup EXIT
mkdir -p "$stage/updater" "$stage/docs"
cp "$root/update.php" "$stage/update.php"
cp "$root/updater/Updater.php" "$root/updater/ActivityLock.php" "$root/updater/PostgresBackup.php" "$stage/updater/"
printf 'test\n' > "$stage/LICENSE"
printf 'test\n' > "$stage/README.md"
printf 'test\n' > "$stage/SECURITY.md"
printf 'test\n' > "$stage/docs/deployment.md"
php "$root/scripts/release/manifest.php" "$stage" > "$stage/package-files.json"
php "$root/scripts/release/validate-manifest.php" "$stage"

printf 'unexpected\n' > "$stage/unexpected.md"
php "$root/scripts/release/manifest.php" "$stage" > "$stage/package-files.json"
if php "$root/scripts/release/validate-manifest.php" "$stage" 2>/dev/null; then
    printf 'Manifest validator accepted an unapproved package file.\n' >&2
    exit 1
fi
