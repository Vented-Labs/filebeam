#!/usr/bin/env bash
# Isolated HTTP peer acceptance harness for Android owners. It never manages shared RTC containers.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
results="$root/test-results/android/peers"
fixtures="$results/fixtures"
logs="$results/logs"
state="$results/state/http-v2"
container=filebeam-android-peer-http
port=${FILEBEAM_PEER_HTTP_PORT:-8019}
url="http://127.0.0.1:$port"

usage() {
    printf '%s\n' 'Usage: peer-http.sh {prepare|start|run-smoke|run-large|status}'
}

prepare() {
    mkdir -p "$fixtures" "$logs" "$state/bootstrap-cache" \
        "$state/storage/logs" "$state/storage/framework/cache/data" \
        "$state/storage/framework/sessions" "$state/storage/framework/testing" \
        "$state/storage/framework/views" "$state/filestore" "$state/staging"
    chmod -R a+rwx "$state"
    cp "$root/backend/bootstrap/cache/packages.php" "$root/backend/bootstrap/cache/services.php" "$state/bootstrap-cache/"
    truncate --size 513MiB "$fixtures/peer-513MiB.bin"
    truncate --size 4097MiB "$fixtures/peer-4097MiB.bin"
    printf 'web-to-cli HTTP smoke\n' > "$fixtures/web-to-cli.txt"
    printf 'cli-to-web HTTP smoke\n' > "$fixtures/cli-to-web.txt"
    {
        printf 'revision=%s\n' "$(git -C "$root" rev-parse HEAD)"
        for file in "$fixtures"/*; do
            printf '%s\t%s\t%s\n' "$(basename "$file")" "$(stat -c %s "$file")" "$(sha256sum "$file" | cut -d ' ' -f 1)"
        done
    } > "$results/fixtures.tsv"
}

start() {
    prepare
    if docker inspect "$container" >/dev/null 2>&1; then
        if [ "$(docker inspect --format '{{.State.Running}}' "$container")" = true ]; then
            return
        fi
        docker rm "$container" >/dev/null
    fi
    rm -f "$state/database.sqlite"
    : > "$state/database.sqlite"
    docker run --detach --name "$container" --init \
        --publish "127.0.0.1:$port:8080" \
        --mount "type=bind,src=$root,dst=/var/www/html,readonly" \
        --mount "type=bind,src=$state,dst=/runtime" \
        --mount "type=bind,src=$state/bootstrap-cache,dst=/var/www/html/backend/bootstrap/cache" \
        --mount "type=bind,src=$state/storage,dst=/var/www/html/backend/storage" \
        --workdir /var/www/html/backend \
        --env APP_NAME='Filebeam Android HTTP peer' \
        --env APP_ENV=local --env APP_DEBUG=true --env APP_KEY='base64:9g1g7bGh5A0Hrr9dBYqVgN4bPJ8VxuwYzvVYk8KQp9c=' \
        --env APP_URL="$url" --env DB_CONNECTION=sqlite --env DB_DATABASE=/runtime/database.sqlite \
        --env CACHE_STORE=database --env QUEUE_CONNECTION=database --env SESSION_DRIVER=cookie \
        --env FILESYSTEM_DISK=local --env FILEBEAM_FILESYSTEMS=transfers \
        --env FILEBEAM_FILESTORE_ROOT=/runtime/filestore --env FILEBEAM_STAGING_ROOT=/runtime/staging \
        --env FILEBEAM_ENABLED_TRANSFER_DRIVERS='["http"]' --env FILEBEAM_DEFAULT_TRANSFER_DRIVER=http \
        --env FILEBEAM_DEFAULT_TRANSFER_BYTES=8589934592 --env FILEBEAM_DEFAULT_FILE_COUNT=20 \
        --env FILEBEAM_DEFAULT_NOTE_BYTES=1048576 --env CHUNK_MAX_SIZE=25000000 \
        --env FILEBEAM_CREATIONS_PER_HOUR=300 --env FILEBEAM_WRITES_PER_MINUTE=3000 --env FILEBEAM_READS_PER_MINUTE=3000 \
        filebeam-sail-php85/app > "$logs/container-id.txt"
    docker exec "$container" php artisan migrate --force > "$logs/migrate.log" 2>&1
    until curl --fail --silent "$url/up" >/dev/null; do sleep 1; done
    curl --fail --silent "$url/api/v1/info" > "$logs/info.json"
}

run() {
    local mode=$1
    start
    cargo build --manifest-path "$root/cli/Cargo.toml" --release > "$logs/cli-build.log" 2>&1
    (
        while docker inspect "$container" >/dev/null 2>&1 && [ "$(docker inspect --format '{{.State.Running}}' "$container")" = true ]; do
            printf '%s\t%s\n' "$(date --iso-8601=seconds)" "$(docker stats --no-stream --format '{{.MemUsage}}' "$container")"
            sleep 2
        done
    ) > "$logs/backend-memory.tsv" &
    local sampler=$!
    trap 'kill "$sampler" 2>/dev/null || true' RETURN
    BASE_URL="$url" BROWSER_TEST_CLI_BINARY="$root/target/release/beam" PEER_RESULTS="$results" PEER_LARGE="$mode" \
        npx playwright test scripts/android/peer-http.spec.ts --config "$root/playwright.config.ts" > "$logs/playwright-$mode.log" 2>&1
    kill "$sampler" 2>/dev/null || true
    trap - RETURN
    docker logs "$container" > "$logs/backend-$mode.log" 2>&1
    status
}

status() {
    mkdir -p "$results"
    {
        printf 'host_url=%s\n' "$url"
        printf 'emulator_url=http://10.0.2.2:%s\n' "$port"
        printf 'container=%s\n' "$container"
        printf 'running=%s\n' "$(docker inspect --format '{{.State.Running}}' "$container" 2>/dev/null || printf false)"
        printf 'fixtures=%s\n' "$fixtures"
        printf 'credentials=anonymous disposable instance; transfer capability tokens are emitted only in ignored Playwright logs\n'
    } > "$results/peer-http.md"
    cat "$results/peer-http.md"
}

case ${1:-} in
    prepare|start|status) "$1" ;;
    run-smoke) run smoke ;;
    run-large) run large ;;
    *) usage >&2; exit 64 ;;
esac
