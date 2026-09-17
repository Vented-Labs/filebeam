#!/usr/bin/env bash
# Run browser/native WebRTC interoperability against disposable local services.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
run_stamp="$(date -u +%Y%m%dT%H%M%SZ)-$$"
results=$(mktemp -d "/tmp/opencode/filebeam-peer-webrtc-$run_stamp.XXXXXX")
published_results="$root/test-results/android/peers/webrtc-$run_stamp"
environment_file=$(mktemp /tmp/opencode/filebeam-peer-webrtc-env.XXXXXX)
run_id="peer-webrtc-$$"
app="filebeam-$run_id-app"
turn="filebeam-$run_id-turn"
vendor="filebeam-$run_id-vendor"
runtime="filebeam-$run_id-runtime"
storage="filebeam-$run_id-storage"
cache="filebeam-$run_id-cache"
base_url="http://127.0.0.1:8027"
turn_port=34790
relay_min=49200
relay_max=49220
turn_secret=$(openssl rand -hex 32)
host_ip=''
for candidate in $(hostname -I); do
    if [[ $candidate != 127.* && $candidate != ::1 ]]; then
        host_ip=$candidate
        break
    fi
done

cleanup() {
    local status=$?
    mkdir -p "$root/test-results/android/peers"
    cp -a "$results" "$published_results" 2>/dev/null || true
    if [[ ${KEEP_PEER_WEBRTC_ENV:-0} != 1 ]]; then
        docker rm -f "$app" "$turn" >/dev/null 2>&1 || true
        docker volume rm "$vendor" "$runtime" "$storage" "$cache" >/dev/null 2>&1 || true
    fi
    rm -f "$environment_file"
    exit "$status"
}
trap cleanup EXIT

if [[ -z $host_ip ]]; then
    printf '%s\n' 'Could not determine a non-loopback host address for TURN.' >&2
    exit 1
fi

mkdir -p "$results/logs"
chmod 700 "$results" "$results/logs"

printf '%s\n' 'Building browser assets and the native CLI...'
(cd "$root" && npm run build) >"$results/logs/browser-build.log" 2>&1
docker run --rm --mount "type=bind,src=$root,dst=/work" --workdir /work/cli \
    --env CARGO_TARGET_DIR=/work/cli/target filebeam-beam-tooling:rust-1.98.0 \
    cargo build --release >"$results/logs/cli-build.log" 2>&1

for volume in "$vendor" "$runtime" "$storage" "$cache"; do
    docker volume create "$volume" >/dev/null
done
docker run --rm -v "$runtime:/runtime" -v "$storage:/storage" -v "$cache:/cache" \
    --entrypoint sh filebeam-sail-php85/app -c \
    'mkdir -p /runtime /storage/app/public /storage/app/private /storage/framework/cache/data /storage/framework/sessions /storage/framework/testing /storage/framework/views /storage/logs /cache && chmod -R 777 /runtime /storage /cache'

# Dependencies live in a harness-owned volume, never in the source checkout or
# either pre-existing verification environment.
docker run --rm \
    -v "$root:/var/www/html" \
    -v "$vendor:/var/www/html/backend/vendor" \
    --entrypoint composer filebeam-sail-php85/app \
    --working-dir=/var/www/html/backend install --no-interaction --prefer-dist --no-progress \
    >"$results/logs/composer-install.log" 2>&1

docker run -d --rm --name "$turn" --network host coturn/coturn:4.6.2 \
    -n --log-file=stdout --no-cli --no-tls --no-dtls --realm=filebeam-peer-webrtc.local \
    --lt-cred-mech --use-auth-secret --static-auth-secret="$turn_secret" \
    --listening-port="$turn_port" --external-ip="$host_ip" \
    --min-port="$relay_min" --max-port="$relay_max" >"$results/turn-container-id"

printf '%s\n' \
    'APP_NAME="Filebeam Peer WebRTC"' 'APP_ENV=local' 'APP_DEBUG=true' \
    'APP_KEY=base64:9g1g7bGh5A0Hrr9dBYqVgN4bPJ8VxuwYzvVYk8KQp9c=' "APP_URL=$base_url" \
    'DB_CONNECTION=sqlite' 'DB_DATABASE=/runtime/filebeam.sqlite' 'CACHE_STORE=database' \
    'QUEUE_CONNECTION=database' 'SESSION_DRIVER=cookie' 'FILESYSTEM_DISK=local' \
    'FILEBEAM_FILESYSTEMS=transfers' 'FILEBEAM_FILESTORE_ROOT=/runtime/filestore' \
    'FILEBEAM_STAGING_ROOT=/runtime/staging' 'FILEBEAM_ENABLED_TRANSFER_DRIVERS=["http","webrtc"]' \
    'FILEBEAM_DEFAULT_TRANSFER_DRIVER=webrtc' 'FILEBEAM_WEBRTC_ICE_SERVERS=[]' \
    "FILEBEAM_WEBRTC_TURN_URLS=turn:$host_ip:$turn_port?transport=udp" \
    "FILEBEAM_WEBRTC_TURN_SECRET=$turn_secret" 'FILEBEAM_WEBRTC_TURN_TTL_SECONDS=120' \
    'FILEBEAM_WEBRTC_SESSION_IDLE_SECONDS=120' 'FILEBEAM_DEFAULT_TRANSFER_BYTES=10737418240' \
    'CHUNK_MAX_SIZE=25000000' 'FILEBEAM_CREATIONS_PER_HOUR=300' >"$environment_file"
chmod 600 "$environment_file"

docker run -d --rm --name "$app" -p "127.0.0.1:8027:8080" \
    -v "$root:/var/www/html" -v "$vendor:/var/www/html/backend/vendor" \
    -v "$runtime:/runtime" -v "$storage:/var/www/html/backend/storage" \
    -v "$cache:/var/www/html/backend/bootstrap/cache" \
    -v "$environment_file:/var/www/html/backend/.env:ro" \
    filebeam-sail-php85/app >"$results/app-container-id"

for _ in {1..30}; do
    if docker exec "$app" php artisan migrate --force --seed >"$results/logs/migrate.log" 2>&1; then
        break
    fi
    sleep 1
done
docker exec "$app" php artisan migrate:status >"$results/logs/migrate-status.log" 2>&1
# Migrations run through docker exec as root while the supervised app runs as
# sail, so make the disposable SQLite database and its WAL sidecars writable.
docker exec "$app" sh -c 'chmod -R 777 /runtime'

for _ in {1..30}; do
    if curl --fail --silent "$base_url/up" >"$results/up.json"; then
        break
    fi
    sleep 1
done
curl --fail --silent "$base_url/up" >"$results/up.json"

printf '%s\n' \
    "base_url=$base_url" \
    "turn_url=turn:$host_ip:$turn_port?transport=udp" \
    "relay_ports=$relay_min-$relay_max/udp" \
    "lifecycle=resources are removed on exit; set KEEP_PEER_WEBRTC_ENV=1 to inspect, then docker rm -f $app $turn and docker volume rm $vendor $runtime $storage $cache" \
    >"$results/environment.txt"
chmod 600 "$results/environment.txt"

if [[ ${PEER_WEBRTC_SETUP_ONLY:-0} == 1 ]]; then
    printf 'Peer WebRTC environment: %s\n' "$published_results"
    exit 0
fi

BASE_URL="$base_url" TURN_URL="turn:$host_ip:$turn_port?transport=udp" RESULTS_DIR="$results" PEER_WEBRTC_CASES="${PEER_WEBRTC_CASES:-}" \
    node "$root/scripts/android/peer-webrtc.mjs" | tee "$results/summary.json"
docker logs "$turn" >"$results/logs/coturn.log" 2>&1
docker logs "$app" >"$results/logs/backend.log" 2>&1
chmod 600 "$results"/logs/* "$results"/*.json "$results"/*.txt
printf 'Peer WebRTC evidence: %s\n' "$published_results"
