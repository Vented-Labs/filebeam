#!/bin/sh
set -eu

base_url=${BEAM_RELEASE_BASE_URL:-https://releases.filebeam.io/cli}
release_public_key='__BEAM_RELEASE_PUBLIC_KEY__'
has_release_key=true
case "$release_public_key" in __*) has_release_key=false ;; esac
if [ -n "${HOME:-}" ]; then install_dir=$HOME/.filebeam; else install_dir=; fi
requested_version=latest

usage() {
    printf '%s\n' 'Usage: install.sh [--dir DIRECTORY] [--version X.Y.Z]'
    exit 64
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dir) [ "$#" -ge 2 ] || usage; install_dir=$2; shift 2 ;;
        --version) [ "$#" -ge 2 ] || usage; requested_version=$2; shift 2 ;;
        --help|-h) usage ;;
        *) usage ;;
    esac
done
[ -n "$install_dir" ] || { printf '%s\n' 'HOME is not set; pass --dir.' >&2; exit 1; }
newline='
'
carriage_return=$(printf '\r')
case "$install_dir" in
    *"$newline"*|*"$carriage_return"*) printf '%s\n' 'The install directory cannot contain line breaks.' >&2; exit 1 ;;
esac

fetch() {
    blocks=$((($3 + 511) / 512))
    if command -v curl >/dev/null 2>&1; then (ulimit -f "$blocks" && curl -fsSL "$1" -o "$2")
    elif command -v wget >/dev/null 2>&1; then (ulimit -f "$blocks" && wget -q -O "$2" "$1")
    else printf '%s\n' 'curl or wget is required.' >&2; exit 1; fi
    size=$(wc -c < "$2")
    [ "$size" -le "$3" ] || { printf '%s\n' 'Download exceeds its allowed size.' >&2; exit 1; }
}
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
    else printf '%s\n' 'sha256sum or shasum is required.' >&2; exit 1; fi
}
decode_base64() {
    if base64 --decode </dev/null >/dev/null 2>&1; then base64 --decode
    else base64 -D; fi
}
verify_catalog() {
    [ "$has_release_key" = true ] || {
        printf '%s\n' 'This installer has no embedded release verification key.' >&2
        exit 1
    }
    command -v openssl >/dev/null 2>&1 || {
        printf '%s\n' 'openssl is required to verify Filebeam releases.' >&2
        exit 1
    }
    signed=$(sed -n 's/.*"signed"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | awk 'NR == 1 { print; exit }')
    signature=$(sed -n 's/.*"signature"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | awk 'NR == 1 { print; exit }')
    [ -n "$signed" ] && [ -n "$signature" ] || {
        printf '%s\n' 'Could not read the signed release catalog.' >&2
        exit 1
    }
    printf '%s' "$signed" | decode_base64 > "$tmp/index.json"
    printf '%s' "$signature" | decode_base64 > "$tmp/index.sig"
    {
        printf '\060\052\060\005\006\003\053\145\160\003\041\000'
        printf '%s' "$release_public_key" | decode_base64
    } > "$tmp/public.der"
    openssl pkey -pubin -inform DER -in "$tmp/public.der" -out "$tmp/public.pem" >/dev/null 2>&1 || {
        printf '%s\n' 'The embedded release verification key is invalid.' >&2
        exit 1
    }
    openssl pkeyutl -verify -pubin -inkey "$tmp/public.pem" -rawin \
        -in "$tmp/index.json" -sigfile "$tmp/index.sig" >/dev/null 2>&1 || {
        printf '%s\n' 'Release catalog signature verification failed.' >&2
        exit 1
    }
}

case "$(uname -s)" in
    Linux) os=linux ;;
    Darwin) os=macos ;;
    *) printf 'Unsupported operating system: %s\n' "$(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64|amd64) architecture=x86_64 ;;
    aarch64|arm64) architecture=aarch64 ;;
    *) printf 'Unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

tmp=$(mktemp -d "${TMPDIR:-/tmp}/beam-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
fetch "$base_url/index.json" "$tmp/index-envelope.json" 1048576
verify_catalog "$tmp/index-envelope.json"
expires_at=$(sed -n 's/.*"expires_at"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$tmp/index.json" | awk 'NR == 1 { print; exit }')
expires_epoch=$(date -u -d "$expires_at" +%s 2>/dev/null || date -j -u -f '%Y-%m-%dT%H:%M:%SZ' "$expires_at" +%s 2>/dev/null || true)
[ -n "$expires_epoch" ] && [ "$expires_epoch" -gt "$(date -u +%s)" ] || {
    printf '%s\n' 'The signed release catalog has expired.' >&2
    exit 1
}
if [ "$requested_version" = latest ]; then
    requested_version=$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([0-9][0-9.]*\)".*/\1/p' "$tmp/index.json" | awk 'NR == 1 { print; exit }')
    [ -n "$requested_version" ] || { printf '%s\n' 'The release catalog has no version.' >&2; exit 1; }
fi
case "$requested_version" in [0-9]*.[0-9]*.[0-9]*) ;; *) printf '%s\n' 'Version must be X.Y.Z.' >&2; exit 64 ;; esac
tag="beam-v$requested_version"
archive="$tag-$os-$architecture.tar.gz"
release_url="$base_url/versions/v$requested_version"
asset=$(awk -v wanted_version="$requested_version" -v wanted_os="$os" -v wanted_arch="$architecture" '
        /"version"[[:space:]]*:/ {
            version=$0; sub(/^.*"version"[[:space:]]*:[[:space:]]*"/, "", version); sub(/".*$/, "", version)
        }
        /"architecture"[[:space:]]*:/ {
            os=""
            architecture=$0; sub(/^.*"architecture"[[:space:]]*:[[:space:]]*"/, "", architecture); sub(/".*$/, "", architecture)
        }
        /"os"[[:space:]]*:/ {
            os=$0; sub(/^.*"os"[[:space:]]*:[[:space:]]*"/, "", os); sub(/".*$/, "", os)
        }
        /"path"[[:space:]]*:/ {
            path=$0; sub(/^.*"path"[[:space:]]*:[[:space:]]*"/, "", path); sub(/".*$/, "", path)
        }
        /"sha256"[[:space:]]*:/ {
            checksum=$0; sub(/^.*"sha256"[[:space:]]*:[[:space:]]*"/, "", checksum); sub(/".*$/, "", checksum)
        }
        /"size"[[:space:]]*:/ {
            size=$0; sub(/^.*"size"[[:space:]]*:[[:space:]]*/, "", size); sub(/[^0-9].*$/, "", size)
            if (version == wanted_version && architecture == wanted_arch && (os == wanted_os || (wanted_os == "linux" && os == ""))) { print path "|" checksum "|" size; exit }
        }
    ' "$tmp/index.json")
expected_path=${asset%%|*}
asset=${asset#*|}
expected=${asset%%|*}
expected_size=${asset#*|}
[ "$expected_path" = "versions/v$requested_version/$archive" ] || {
    printf '%s\n' 'The signed catalog does not contain the expected archive.' >&2
    exit 1
}
[ -n "$expected" ] || { printf 'No checksum for %s.\n' "$archive" >&2; exit 1; }
case "$expected_size" in ''|*[!0-9]*) printf '%s\n' 'The signed archive size is invalid.' >&2; exit 1 ;; esac
[ "$expected_size" -gt 0 ] && [ "$expected_size" -le 104857600 ] || {
    printf '%s\n' 'The signed archive size is invalid.' >&2
    exit 1
}
fetch "$release_url/$archive" "$tmp/$archive" "$expected_size"
[ "$(wc -c < "$tmp/$archive")" -eq "$expected_size" ] || { printf '%s\n' 'Archive size mismatch.' >&2; exit 1; }
[ "$(sha256 "$tmp/$archive")" = "$expected" ] || { printf '%s\n' 'Archive checksum mismatch.' >&2; exit 1; }
tar -xOzf "$tmp/$archive" beam/beam > "$tmp/beam"
[ -s "$tmp/beam" ] || { printf '%s\n' 'Archive does not contain beam.' >&2; exit 1; }

mkdir -p "$install_dir/bin" "$install_dir/cache"
[ -f "$install_dir/config.toml" ] || : > "$install_dir/config.toml"
candidate="$install_dir/bin/.beam.$$"
install -m 0755 "$tmp/beam" "$candidate"
mv -f "$candidate" "$install_dir/bin/beam"

add_path() {
    file=$1
    line=$2
    mkdir -p "$(dirname "$file")"
    [ -f "$file" ] || : > "$file"
    grep -Fqx "$line" "$file" 2>/dev/null || printf '\n%s\n' "$line" >> "$file"
}
if [ -n "${HOME:-}" ]; then
    escaped_bin=$(printf '%s' "$install_dir/bin" | sed 's/[\\"$`]/\\&/g')
    add_path "$HOME/.bashrc" "export PATH=\"$escaped_bin:\$PATH\""
    add_path "$HOME/.zshrc" "export PATH=\"$escaped_bin:\$PATH\""
    add_path "$HOME/.config/fish/config.fish" "fish_add_path -m \"$escaped_bin\""
fi
printf 'Installed beam %s to %s/bin/beam\n' "$requested_version" "$install_dir"
