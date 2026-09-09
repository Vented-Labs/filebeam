#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "$root/.filebeam-catalog-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/dist"
touch "$tmp/dist/beam-v1.2.3-linux-x86_64.tar.gz" "$tmp/dist/beam-v1.2.3-linux-aarch64.tar.gz"
container_tmp=/workspace/${tmp#"$root"/}

"$root/scripts/cli/run.sh" bash -ceu '
keypair=$(php -r '\''$pair=sodium_crypto_sign_keypair(); echo base64_encode(sodium_crypto_sign_publickey($pair))." ".base64_encode(sodium_crypto_sign_secretkey($pair));'\'')
read -r RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY <<< "$keypair"
export RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY
php scripts/release/cli-write-release.php beam-v1.2.3 "$1/dist" "$1/release.json"
php scripts/release/cli-update-index.php /dev/null "$1/release.json" > "$1/index.json"
php scripts/release/sign-index.php < "$1/index.json" > "$1/envelope.json"
php scripts/release/cli-verify-index.php < "$1/envelope.json" > "$1/verified.json"
grep -Fq '\''"version": "1.2.3"'\'' "$1/verified.json"
for length in 0 63 64 65; do
    php -r '\''$envelope=json_decode(file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR); $envelope["signature"]=base64_encode(str_repeat("x", (int) $argv[2])); echo json_encode($envelope, JSON_THROW_ON_ERROR);'\'' "$1/envelope.json" "$length" > "$1/invalid.json"
    if php scripts/release/cli-verify-index.php < "$1/invalid.json" > /dev/null 2> "$1/error.txt"; then
        printf "Invalid %s-byte signature was accepted.\n" "$length" >&2
        exit 1
    fi
    grep -Fq "CLI index signature verification failed." "$1/error.txt"
    if grep -Fq "SodiumException" "$1/error.txt"; then
        printf "Signature length was not checked before calling sodium.\n" >&2
        exit 1
    fi
done
' bash "$container_tmp"
printf 'CLI catalog test passed.\n'
