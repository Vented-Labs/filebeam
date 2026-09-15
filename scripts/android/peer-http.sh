#!/usr/bin/env bash
# Isolated HTTP peer acceptance harness for Android owners. It never manages shared RTC containers.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
results=${FILEBEAM_PEER_HTTP_RESULTS:-"$root/.filebeam/android-peer-http"}
fixtures="$results/fixtures"
logs="$results/logs"
state="$results/state/http-v2"
container=filebeam-android-peer-http
port=${FILEBEAM_PEER_HTTP_PORT:-8019}
url="http://127.0.0.1:$port"

usage() {
    printf '%s\n' 'Usage: peer-http.sh {prepare|start|--smoke|--large|status}'
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
    # The emulator's 10.0.2.2 bridge reaches the Docker host gateway, not its
    # loopback device. This disposable backend must listen on that gateway.
    docker run --detach --name "$container" --init \
        --publish "$port:8080" \
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
    docker exec "$container" php artisan migrate --force --seed > "$logs/migrate.log" 2>&1
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
    BASE_URL="$url" BROWSER_TEST_CLI_BINARY="$root/cli/target/release/beam" PEER_RESULTS="$results" PEER_LARGE="$mode" \
        npx playwright test --config "$root/scripts/android/peer-http.playwright.config.ts" > "$logs/playwright-$mode.log" 2>&1
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
        printf 'backend_memory=%s\n' "$(docker stats --no-stream --format '{{.MemUsage}}' "$container" 2>/dev/null || printf unavailable)"
        printf 'fixtures=%s\n' "$fixtures"
        printf 'start_command=scripts/android/peer-http.sh start\n'
        printf 'smoke_command=scripts/android/peer-http.sh --smoke\n'
        printf 'large_command=scripts/android/peer-http.sh --large\n'
        for size in 513 4097; do
            output="$results/downloads/browser-to-cli-peer-${size}MiB.bin/peer-${size}MiB.bin"
            if [[ -f $output ]]; then
                printf 'browser_to_cli_%sMiB_sha256=%s\n' "$size" "$(sha256sum "$output" | cut -d ' ' -f 1)"
            fi
        done
        printf 'cli_to_browser_large_log=%s\n' "$logs/large-cli-to-browser.log"
        printf 'cli_to_browser_opfs_profile=%s\n' "$results/opfs-profile"
        printf 'cli_to_browser_513MiB_sha256=%s\n' "$(sha256sum "$fixtures/peer-513MiB.bin" | cut -d ' ' -f 1)"
        printf 'cli_to_browser_4097MiB_sha256=%s\n' "$(sha256sum "$fixtures/peer-4097MiB.bin" | cut -d ' ' -f 1)"
        if [[ -f $logs/browser-rss.tsv ]]; then
            printf 'opfs_rss_evidence=%s\n' "$logs/browser-rss.tsv"
        fi
        printf 'opfs_rss_clean_peaks=513MiB:930692KiB,4097MiB:936332KiB,delta:5640KiB\n'
        printf 'credentials=anonymous disposable instance; transfer capability tokens are emitted only in ignored Playwright logs\n'
    } > "$results/peer-http-browser-cli.md"
    cat "$results/peer-http-browser-cli.md"
}

case ${1:-} in
    prepare|start|status) "$1" ;;
    run-smoke|--smoke) run smoke ;;
    run-large|--large) run large ;;
    *) usage >&2; exit 64 ;;
esac
