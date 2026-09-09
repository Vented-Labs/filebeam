#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-release-smoke.XXXXXX")
server_pid=''
cleanup() {
    if [[ -n $server_pid ]]; then kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true; fi
    rm -rf "$temporary"
}
trap cleanup EXIT

# Test-only signing keys; production signing material is supplied exclusively by CI.
keypair=$(php -r '$pair=sodium_crypto_sign_keypair(); echo base64_encode(sodium_crypto_sign_publickey($pair))." ".base64_encode(sodium_crypto_sign_secretkey($pair));')
read -r RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY <<< "$keypair"
export RELEASE_PUBLIC_KEY RELEASE_SIGNING_KEY
BEAM_RELEASE_PUBLIC_KEY=$RELEASE_PUBLIC_KEY "$root/scripts/cli/package.sh" beam-v0.2.0 "$temporary/package"
bash "$root/scripts/cli/smoke-package.sh" beam-v0.2.0 "$temporary/package"

mkdir -p "$temporary/public/cli/versions/v0.2.0"
cp "$temporary/package/"*.tar.gz "$temporary/public/cli/versions/v0.2.0/"
cp "$temporary/package/install.sh" "$temporary/public/cli/install.sh"
php "$root/scripts/release/cli-write-release.php" beam-v0.2.0 "$temporary/package" "$temporary/release.json"
php "$root/scripts/release/cli-update-index.php" /dev/null "$temporary/release.json" > "$temporary/index.json"
php "$root/scripts/release/sign-index.php" < "$temporary/index.json" > "$temporary/public/cli/index.json"

python3 -u -c '
import functools, http.server, sys
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=sys.argv[1]))
print(server.server_port, flush=True)
server.serve_forever()
' "$temporary/public" > "$temporary/port" 2> "$temporary/http.log" &
server_pid=$!
for _ in {1..50}; do [[ -s "$temporary/port" ]] && break; sleep 0.1; done
[[ -s "$temporary/port" ]]
export BEAM_RELEASE_BASE_URL="http://127.0.0.1:$(<"$temporary/port")/cli"
bash "$root/scripts/cli/smoke-install.sh" beam-v0.2.0
HOME="$temporary/home" sh "$temporary/public/cli/install.sh" --dir "$temporary/latest"
[[ $("$temporary/latest/bin/beam" --version) == 'beam 0.2.0' ]]

printf '{}\n' > "$temporary/public/cli/index.json"
if HOME="$temporary/home" sh "$temporary/public/cli/install.sh" --dir "$temporary/rejected" > "$temporary/rejected.log" 2>&1; then
    printf 'Installer accepted an unsigned catalog.\n' >&2; exit 1
fi
[[ ! -e "$temporary/rejected/bin/beam" ]]
printf 'Signed CLI package, HTTP installer, installed binary, latest selection, and tamper rejection passed.\n'
