#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd); tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
for t in linux-aarch64.tar.gz macos-x86_64.tar.gz macos-aarch64.tar.gz windows-x86_64.zip; do printf x >"$tmp/filebeam-desktop-v1.2.3-$t"; done
bash "$root/scripts/desktop/package-update-payload.sh" v1.2.3 linux x86_64 /bin/true "$tmp"
php "$root/scripts/release/desktop-write-release.php" v1.2.3 "$tmp" "$tmp/release.json"; php "$root/scripts/release/desktop-update-index.php" /dev/null "$tmp/release.json" >"$tmp/index.json"; php "$root/scripts/release/desktop-update-index.php" "$tmp/index.json" --refresh >"$tmp/refreshed-index.json"
grep -Fq '"product": "desktop"' "$tmp/index.json"; grep -Fq '"kind": "tar-gz"' "$tmp/release.json"; grep -Fq '"kind": "app-tar-gz"' "$tmp/release.json"; grep -Fq '"kind": "zip-exe"' "$tmp/release.json"; tar -tzf "$tmp/filebeam-desktop-v1.2.3-linux-x86_64.tar.gz" | grep -Fx filebeam/filebeam
