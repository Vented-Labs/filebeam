#!/usr/bin/env bash
# Runs the real Laravel device API behind a locally trusted TLS endpoint.
set -euo pipefail

source "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/common.sh"
device_init
device_require php
device_require openssl
device_require socat
device_require curl
device_require python3

backend="$device_root/backend"
runtime="$device_scratch/production-backend"
mkdir -p "$runtime/config" "$runtime/app" "$runtime/database"
chmod 700 "$runtime" "$runtime/config" "$runtime/app" "$runtime/database"

http_port=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
tls_port=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
print(s.getsockname()[1])
s.close()
PY
)
instance="https://localhost:$tls_port"
certificate="$runtime/localhost.pem"
key="$runtime/localhost.key"
database="$runtime/database/filebeam.sqlite"
touch "$database"
chmod 600 "$database"
openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 1 -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost' -addext 'basicConstraints=critical,CA:FALSE' -addext 'extendedKeyUsage=serverAuth' -keyout "$key" -out "$certificate" >/dev/null 2>&1
chmod 600 "$key" "$certificate"

printf '%s\n' \
  'APP_ENV=testing' \
  "APP_KEY=base64:$(openssl rand -base64 32)" \
  "APP_URL=$instance" \
  'APP_NAME="Filebeam Device Harness"' \
  'DB_CONNECTION=sqlite' \
  "DB_DATABASE=$database" \
  'CACHE_STORE=database' \
  'QUEUE_CONNECTION=sync' \
  'SESSION_DRIVER=file' \
  'FILEBEAM_DEVICE_RENDEZVOUS_ENABLED=true' \
  'FILEBEAM_ENABLED_TRANSFER_DRIVERS=["webrtc"]' \
  'FILEBEAM_DEFAULT_TRANSFER_DRIVER=webrtc' \
  'FILEBEAM_WEBRTC_ICE_SERVERS=[]' \
  'FILEBEAM_WEBRTC_TURN_URLS=' \
  > "$runtime/config/.env"

php_server= proxy_server=
cleanup() {
  [[ -z ${proxy_server:-} ]] || kill "$proxy_server" 2>/dev/null || true
  [[ -z ${php_server:-} ]] || kill "$php_server" 2>/dev/null || true
  rm -rf "$device_scratch"
}
trap cleanup EXIT

export FILEBEAM_CONTAINER=1 FILEBEAM_DATA_DIR="$runtime" FILEBEAM_VARIANT=light
# Container mode defaults generated caches and file storage to /run and /storage.
mkdir -p "$runtime/cache" "$runtime/views" "$runtime/storage" "$runtime/app/framework/sessions" "$runtime/app/framework/cache" "$runtime/app/logs"
export APP_CONFIG_CACHE="$runtime/cache/config.php" APP_ROUTES_CACHE="$runtime/cache/routes.php" \
  APP_EVENTS_CACHE="$runtime/cache/events.php" APP_SERVICES_CACHE="$runtime/cache/services.php" \
  APP_PACKAGES_CACHE="$runtime/cache/packages.php" VIEW_COMPILED_PATH="$runtime/views" \
  FILEBEAM_FILESTORE_ROOT="$runtime/storage"
php "$backend/artisan" migrate:fresh --force --no-interaction >/dev/null
php "$backend/artisan" tinker --execute="App\\Models\\User::query()->create(['name' => 'Device Harness', 'username' => 'device_harness', 'normalized_username' => 'device_harness', 'email' => 'device-harness@example.test', 'password' => Illuminate\\Support\\Facades\\Hash::make('correct horse battery staple')]);" >/dev/null
(cd "$backend/public" && PHP_CLI_SERVER_WORKERS=4 exec php -S "127.0.0.1:$http_port" "$backend/vendor/laravel/framework/src/Illuminate/Foundation/resources/server.php") >"$runtime/http.log" 2>&1 &
php_server=$!
socat "OPENSSL-LISTEN:$tls_port,reuseaddr,fork,cert=$certificate,key=$key,verify=0" "TCP:127.0.0.1:$http_port" >/dev/null 2>&1 &
proxy_server=$!
for _ in {1..100}; do
  curl --silent --fail --cacert "$certificate" "$instance/up" >/dev/null && break
  sleep 0.05
done
curl --silent --fail --cacert "$certificate" "$instance/up" >/dev/null

FILEBEAM_DEVICE_INSTANCE="$instance" FILEBEAM_DEVICE_ROOT="$runtime" FILEBEAM_DEVICE_CA="$certificate" \
  cargo run --release --locked --manifest-path "$device_root/tests/device/production-backend/Cargo.toml"
php "$backend/artisan" tinker --execute="if (Illuminate\\Support\\Facades\\DB::table('transfers')->exists() || Illuminate\\Support\\Facades\\DB::table('transfer_items')->exists() || Illuminate\\Support\\Facades\\DB::table('transfer_chunks')->exists() || Illuminate\\Support\\Facades\\DB::table('transfer_chunk_locations')->exists() || Illuminate\\Support\\Facades\\DB::table('transfer_chunk_uploads')->exists() || Illuminate\\Support\\Facades\\DB::table('transfer_key_envelopes')->exists() || Illuminate\\Support\\Facades\\DB::table('filestores')->exists()) { throw new RuntimeException('device transfer created a public transfer or filestore record'); }" >/dev/null
# The harness writes this text into a hidden file that must travel only between the daemons.
python3 - "$database" "$runtime/http.log" "$runtime/app/logs" <<'PY'
import pathlib
import sys

needle = b'private hidden canary'
paths = [path for arg in sys.argv[1:] for path in (pathlib.Path(arg).rglob('*') if pathlib.Path(arg).is_dir() else [pathlib.Path(arg)])]
leaked = [str(path) for path in paths if path.is_file() and needle in path.read_bytes()]
if leaked:
    print('device private metadata escaped into: ' + ', '.join(leaked), file=sys.stderr)
    raise SystemExit(1)
PY
printf '%s\n' 'Device harness passed: TLS registration and approval, pinned daemons, private tree delivery with a verified receipt, no server-side transfer records, and web revoke during an active transfer.'
