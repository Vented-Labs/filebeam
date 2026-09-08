#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-publish-test.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

account=0123456789abcdef0123456789abcdef
bucket=filebeam-releases
endpoint="https://$account.r2.cloudflarestorage.com"

[[ $(php "$root/scripts/release/r2-endpoint.php" "$endpoint/$bucket/" "$bucket") == "$endpoint" ]]
[[ $(php "$root/scripts/release/r2-endpoint.php" "${endpoint}/filebeam%2Dreleases" "$bucket") == "$endpoint" ]]
for invalid_endpoint in "$endpoint/not-the-bucket" "$endpoint/$bucket/extra" "$endpoint?query=value" "https://user@$account.r2.cloudflarestorage.com"; do
    if php "$root/scripts/release/r2-endpoint.php" "$invalid_endpoint" "$bucket" >/dev/null 2>&1; then
        printf 'Expected invalid R2 endpoint to be rejected: %s\n' "$invalid_endpoint" >&2
        exit 1
    fi
done

mkdir -p "$tmp/bin" "$tmp/release/filebeam/backend/config"
log="$tmp/aws.log"
cat > "$tmp/bin/aws" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >> "$AWS_LOG"
if [[ " $* " == *' head-object '* ]]; then
    printf 'Not Found\n' >&2
    exit 255
fi
EOF
chmod +x "$tmp/bin/aws"

keypair=$(php -r '$pair=sodium_crypto_sign_keypair(); echo base64_encode(sodium_crypto_sign_publickey($pair))." ".base64_encode(sodium_crypto_sign_secretkey($pair));')
read -r RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY <<< "$keypair"
export RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY
printf "<?php return ['update_public_key' => '%s'];\n" "$RELEASE_PUBLIC_KEY" > "$tmp/release/filebeam/backend/config/version.php"
(cd "$tmp/release" && zip -qr "filebeam-v0.1.0.zip" filebeam)
archive="$tmp/release/filebeam-v0.1.0.zip"
php "$root/scripts/release/write-release.php" "$tmp/release/release.json" v0.1.0 0.1.0 test 2026-01-01T00:00:00Z versions/v0.1.0/filebeam-v0.1.0.zip "$(php -r 'echo hash_file("sha256", $argv[1]);' "$archive")" "$(php -r 'echo filesize($argv[1]);' "$archive")" 8.5

AWS_LOG=$log PATH="$tmp/bin:$PATH" R2_ENDPOINT_URL="$endpoint/$bucket" R2_BUCKET="$bucket" AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test "$root/scripts/release/publish.sh" v0.1.0 "$tmp/release" >/dev/null

php -r '
    $lines = file($argv[1], FILE_IGNORE_NEW_LINES);
    foreach (["--endpoint-url" => 6, $argv[2] => 6, $argv[3] => 6, "index.json" => 2, "versions/v0.1.0/filebeam-v0.1.0.zip" => 2, "versions/v0.1.0/release.json" => 2] as $value => $expected) {
        if (count(array_keys($lines, $value, true)) !== $expected) {
            throw new RuntimeException("Unexpected AWS argument count for {$value}.");
        }
    }
    foreach ($lines as $line) {
        if (str_starts_with($line, $argv[3]."/")) {
            throw new RuntimeException("S3 object keys must not include R2_BUCKET.");
        }
    }
' "$log" "$endpoint" "$bucket"

printf 'Publish endpoint and key test passed.\n'
