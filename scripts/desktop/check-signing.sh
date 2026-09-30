#!/usr/bin/env bash
set -euo pipefail
for name in RELEASE_SIGNING_KEY RELEASE_PUBLIC_KEY APPLE_CERTIFICATE_BASE64 APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_NOTARY_KEY_BASE64 APPLE_NOTARY_KEY_ID APPLE_NOTARY_ISSUER WINDOWS_CERTIFICATE_BASE64 WINDOWS_SIGNING_PASSWORD; do
    [[ -n ${!name:-} ]] || { printf 'Desktop release requires %s\n' "$name" >&2; exit 1; }
done
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
php "$root/scripts/release/cli-public-key.php" derive >/dev/null
