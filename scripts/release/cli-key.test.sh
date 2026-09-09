#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/beam-key-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

keypair=$(php -r '$pair=sodium_crypto_sign_keypair(); echo base64_encode(sodium_crypto_sign_publickey($pair))." ".base64_encode(sodium_crypto_sign_secretkey($pair));')
read -r canonical_public canonical_secret <<< "$keypair"
other_public=$(php -r 'echo base64_encode(sodium_crypto_sign_publickey(sodium_crypto_sign_keypair()));')
format_key() {
    local key=$1 format=$2
    if [[ $format == unpadded || $format == both ]]; then key=${key%%=*}; fi
    if [[ $format == whitespace || $format == both ]]; then
        printf ' \n\t%s\r\n%s \t\n' "${key:0:12}" "${key:12}"
    else
        printf '%s' "$key"
    fi
}
reject() {
    if "$@" > "$tmp/stdout" 2> "$tmp/stderr"; then
        printf '%s\n' 'Expected key validation to reject the input.' >&2
        exit 1
    fi
    [[ ! -s "$tmp/stdout" ]]
    [[ -s "$tmp/stderr" ]]
    ! grep -Fq "$canonical_secret" "$tmp/stderr"
}

for secret_format in canonical unpadded whitespace both; do
    secret=$(format_key "$canonical_secret" "$secret_format")
    [[ $(RELEASE_SIGNING_KEY=$secret RELEASE_PUBLIC_KEY='' php "$root/scripts/release/cli-public-key.php" derive) == "$canonical_public" ]]
    for public_format in canonical unpadded whitespace both; do
        public=$(format_key "$canonical_public" "$public_format")
        [[ $(RELEASE_SIGNING_KEY=$secret RELEASE_PUBLIC_KEY=$public php "$root/scripts/release/cli-public-key.php" derive) == "$canonical_public" ]]
        [[ $(BEAM_RELEASE_PUBLIC_KEY=$public php "$root/scripts/release/cli-public-key.php" public) == "$canonical_public" ]]
    done
done
reject env RELEASE_SIGNING_KEY="$canonical_secret" RELEASE_PUBLIC_KEY="$other_public" php "$root/scripts/release/cli-public-key.php" derive
grep -Fq 'does not match RELEASE_SIGNING_KEY' "$tmp/stderr"
for invalid in '' 'invalid!' "$canonical_public"; do
    reject env RELEASE_SIGNING_KEY="$invalid" RELEASE_PUBLIC_KEY='' php "$root/scripts/release/cli-public-key.php" derive
done
for invalid in '' 'invalid!' "$canonical_secret" "$(php -r 'echo base64_encode(str_repeat("x", 31));')" "$(php -r 'echo base64_encode(str_repeat("x", 33));')"; do
    reject env BEAM_RELEASE_PUBLIC_KEY="$invalid" php "$root/scripts/release/cli-public-key.php" public
done

mkdir -p "$tmp/bin" "$tmp/package" "$tmp/objects"
cat > "$tmp/bin/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == run ]]
printf '%s\n' "$BEAM_RELEASE_PUBLIC_KEY" > "$DOCKER_PUBLIC"
printf '%s\n' "$@" > "$DOCKER_ARGS"
EOF
chmod +x "$tmp/bin/docker"
export DOCKER_PUBLIC="$tmp/docker-public" DOCKER_ARGS="$tmp/docker-args"
export PATH="$tmp/bin:$PATH"
for format in canonical unpadded whitespace both; do
    BEAM_RELEASE_PUBLIC_KEY=$(format_key "$canonical_public" "$format") BEAM_DOCKER_BUILD=false \
        RELEASE_SIGNING_KEY=$canonical_secret bash "$root/scripts/cli/run.sh" true
    [[ $(<"$DOCKER_PUBLIC") == "$canonical_public" ]]
    grep -Fxq BEAM_RELEASE_PUBLIC_KEY "$DOCKER_ARGS"
    ! grep -Fq RELEASE_SIGNING_KEY "$DOCKER_ARGS"
    ! grep -Fq "$canonical_secret" "$DOCKER_ARGS"
done
rm "$DOCKER_ARGS"
reject env BEAM_RELEASE_PUBLIC_KEY='invalid!' BEAM_DOCKER_BUILD=false bash "$root/scripts/cli/run.sh" true
[[ ! -e "$DOCKER_ARGS" ]]

cat > "$tmp/bin/aws" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $RELEASE_PUBLIC_KEY == "$CANONICAL_PUBLIC" ]]
printf '%s\n' "$*" >> "$AWS_LOG"
shift 3
operation=$1
shift
key='' body='' target=''
while (($#)); do
    case "$1" in
        --key) key=$2; shift 2 ;;
        --body) body=$2; shift 2 ;;
        --*) shift 2 ;;
        *) target=$1; shift ;;
    esac
done
object="$AWS_OBJECTS/$key"
case "$operation" in
    head-object)
        [[ -f "$object" ]] || { printf 'Not Found\n' >&2; exit 255; }
        digest=$(sha256sum "$object"); digest=${digest%% *}
        printf '{"ETag":"fixture","Metadata":{"sha256":"%s"}}\n' "$digest"
        ;;
    get-object) cp "$object" "$target" ;;
    put-object) mkdir -p "$(dirname "$object")"; cp "$body" "$object" ;;
    *) exit 1 ;;
esac
EOF
chmod +x "$tmp/bin/aws"
export AWS_LOG="$tmp/aws.log" AWS_OBJECTS="$tmp/objects" CANONICAL_PUBLIC="$canonical_public"
export R2_ENDPOINT_URL=https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com
export R2_BUCKET=filebeam-releases AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test
printf '%s\n' "$canonical_public" > "$tmp/package/public-key"
printf '0.2.0\n' > "$tmp/package/version"
for architecture in x86_64 aarch64; do
    printf 'archive fixture\n' > "$tmp/package/beam-v0.2.0-linux-$architecture.tar.gz"
done
(cd "$tmp/package" && sha256sum beam-v0.2.0-linux-*.tar.gz > checksums.txt)
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$canonical_public|g" "$root/scripts/cli/install.sh" > "$tmp/package/install.sh"
for format in canonical unpadded whitespace both; do
    export RELEASE_SIGNING_KEY=$(format_key "$canonical_secret" "$format")
    export RELEASE_PUBLIC_KEY=$(format_key "$canonical_public" "$format")
    bash "$root/scripts/release/cli-publish.sh" beam-v0.2.0 "$tmp/package" >/dev/null
    cmp "$tmp/package/install.sh" "$AWS_OBJECTS/cli/install.sh"
    RELEASE_PUBLIC_KEY=$canonical_public php "$root/scripts/release/cli-verify-index.php" < "$AWS_OBJECTS/cli/index.json" > /dev/null
done
# Derivation also supports repositories without a separate public-key secret.
RELEASE_PUBLIC_KEY='' bash "$root/scripts/release/cli-publish.sh" beam-v0.2.0 "$tmp/package" >/dev/null
RELEASE_PUBLIC_KEY='' bash "$root/scripts/release/cli-refresh-index.sh" >/dev/null
RELEASE_PUBLIC_KEY=$canonical_public php "$root/scripts/release/cli-verify-index.php" < "$AWS_OBJECTS/cli/index.json" > /dev/null

: > "$AWS_LOG"
reject env RELEASE_PUBLIC_KEY="$other_public" bash "$root/scripts/release/cli-publish.sh" beam-v0.2.0 "$tmp/package"
[[ ! -s "$AWS_LOG" ]]
reject env RELEASE_PUBLIC_KEY="$other_public" bash "$root/scripts/release/cli-refresh-index.sh"
[[ ! -s "$AWS_LOG" ]]
printf '%s\n' "$other_public" > "$tmp/package/public-key"
reject bash "$root/scripts/release/cli-publish.sh" beam-v0.2.0 "$tmp/package"
grep -Fq 'CLI package public key does not match' "$tmp/stderr"
[[ ! -s "$AWS_LOG" ]]
printf '%s\n' "$canonical_public" > "$tmp/package/public-key"
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$other_public|g" "$root/scripts/cli/install.sh" > "$tmp/package/install.sh"
reject bash "$root/scripts/release/cli-publish.sh" beam-v0.2.0 "$tmp/package"
grep -Fq 'CLI package installer does not match' "$tmp/stderr"
[[ ! -s "$AWS_LOG" ]]
printf 'CLI key derivation, canonical Docker/installer keys, signed catalog, and mismatch rejection passed.\n'
