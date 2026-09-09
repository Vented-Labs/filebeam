#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "$root/.filebeam-install-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/release" "$tmp/package/beam" "$tmp/bin"
printf '#!/bin/sh\nexit 0\n' > "$tmp/package/beam/beam"
chmod +x "$tmp/package/beam/beam"
cp "$root/scripts/cli/install.sh" "$tmp/package/beam/install.sh"
printf '1.2.3\n' > "$tmp/package/beam/version"
tar -C "$tmp/package" -czf "$tmp/release/beam-v1.2.3-linux-x86_64.tar.gz" beam
cp "$tmp/release/beam-v1.2.3-linux-x86_64.tar.gz" "$tmp/release/beam-v1.2.3-linux-aarch64.tar.gz"
(cd "$tmp/release" && sha256sum beam-v1.2.3-linux-x86_64.tar.gz > checksums.txt)
cat > "$tmp/bin/curl" <<'EOF'
#!/bin/sh
set -eu
url=$2
target=$4
case "$url" in
    */index.json) cp "$BEAM_TEST_RELEASE/envelope.json" "$target" ;;
    */checksums.txt) cp "$BEAM_TEST_RELEASE/checksums.txt" "$target" ;;
    */beam-v1.2.3-linux-x86_64.tar.gz) cp "$BEAM_TEST_RELEASE/beam-v1.2.3-linux-x86_64.tar.gz" "$target" ;;
    *) exit 1 ;;
esac
EOF
chmod +x "$tmp/bin/curl"
container_tmp=/workspace/${tmp#"$root"/}

"$root/scripts/cli/run.sh" bash -ceu '
export HOME="$1/home" BEAM_TEST_RELEASE="$1/release" BEAM_RELEASE_BASE_URL=https://example.invalid
export PATH="$1/bin:$PATH"
keypair=$(php -r '\''$pair=sodium_crypto_sign_keypair(); echo base64_encode(sodium_crypto_sign_publickey($pair))." ".base64_encode(sodium_crypto_sign_secretkey($pair));'\'')
read -r RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY <<< "$keypair"
export RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY
php scripts/release/cli-write-release.php beam-v1.2.3 "$1/release" "$1/release.json"
php scripts/release/cli-update-index.php /dev/null "$1/release.json" > "$1/index.json"
php scripts/release/sign-index.php < "$1/index.json" > "$1/release/envelope.json"
if scripts/cli/install.sh --dir "$HOME/unsigned" --version 1.2.3; then
    printf "%s\n" "Installer without an embedded release key unexpectedly succeeded." >&2
    exit 1
fi
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$RELEASE_PUBLIC_KEY|g" scripts/cli/install.sh > "$1/signed-install.sh"
if sh "$1/signed-install.sh" --dir "$HOME/line
break" --version 1.2.3; then
    printf "%s\n" "Installer accepted a directory containing a line break." >&2
    exit 1
fi
sh "$1/signed-install.sh" --dir "$HOME/custom" --version 1.2.3
sh "$1/signed-install.sh" --dir "$HOME/custom" --version 1.2.3
test -x "$HOME/custom/bin/beam"
test -x "$HOME/custom/bin/install.sh"
test -f "$HOME/custom/config.toml"
test -d "$HOME/custom/cache"
test "$(grep -Fc "export PATH=\"$HOME/custom/bin:\$PATH\"" "$HOME/.bashrc")" = 1
test "$(grep -Fc "fish_add_path -m \"$HOME/custom/bin\"" "$HOME/.config/fish/config.fish")" = 1

hostile="$HOME/hostile-backslash\\-\$(touch pwned)-\"quoted\"-\`touch ticked\`"
sh "$1/signed-install.sh" --dir "$hostile" --version 1.2.3
(cd "$HOME" && sh -c '\''. "$1"'\'' sh "$HOME/.bashrc")
test ! -e "$HOME/pwned"
test ! -e "$HOME/ticked"
test -x "$hostile/bin/beam"

sh "$1/signed-install.sh" --dir "$HOME/latest"
test -x "$HOME/latest/bin/beam"
' bash "$container_tmp"
printf 'CLI installer test passed.\n'
