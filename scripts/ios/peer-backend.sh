#!/usr/bin/env bash
# Disposable loopback Laravel peer for native acceptance. It owns no shared services.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
state=${FILEBEAM_IOS_PEER_STATE:-"$root/.filebeam/ios-peer"}
container=${FILEBEAM_IOS_PEER_CONTAINER:-filebeam-ios-peer}
port=${FILEBEAM_IOS_PEER_PORT:-}
url=

die() { printf 'ios peer: %s\n' "$*" >&2; exit 1; }
allocate_port() {
    if [[ -n $port ]]; then return; fi
    for _ in $(seq 1 40); do
        candidate=$(shuf -i 18080-28080 -n 1)
        if ! ss -ltn "sport = :$candidate" | grep -q LISTEN; then port=$candidate; return; fi
    done
    die 'could not allocate a loopback port'
}
prepare() {
    mkdir -p "$state" "$state/storage/framework/cache/data" "$state/storage/framework/sessions" "$state/storage/framework/views" "$state/storage/logs" "$state/filestore" "$state/staging"
    chmod -R a+rwx "$state"
    : > "$state/database.sqlite"
}
start() {
    allocate_port; url="http://127.0.0.1:$port"; prepare
    if docker inspect "$container" >/dev/null 2>&1; then docker rm -f "$container" >/dev/null; fi
    docker run --detach --name "$container" --init --publish "127.0.0.1:$port:8080" \
        --mount "type=bind,src=$root,dst=/var/www/html,readonly" \
        --mount "type=bind,src=$state,dst=/runtime" \
        --mount "type=bind,src=$state/storage,dst=/var/www/html/backend/storage" \
        --workdir /var/www/html/backend \
        --env APP_ENV=testing --env APP_DEBUG=false --env APP_URL="$url" \
        --env DB_CONNECTION=sqlite --env DB_DATABASE=/runtime/database.sqlite \
        --env CACHE_STORE=database --env QUEUE_CONNECTION=sync --env SESSION_DRIVER=cookie \
        --env FILESYSTEM_DISK=local --env FILEBEAM_FILESTORE_ROOT=/runtime/filestore --env FILEBEAM_STAGING_ROOT=/runtime/staging \
        --env FILEBEAM_ENABLED_TRANSFER_DRIVERS='["http"]' --env FILEBEAM_DEFAULT_TRANSFER_DRIVER=http \
        --env FILEBEAM_CREATIONS_PER_HOUR=300 --env FILEBEAM_WRITES_PER_MINUTE=3000 --env FILEBEAM_READS_PER_MINUTE=3000 \
        filebeam-sail-php85/app:latest >/dev/null
    docker exec "$container" php artisan migrate --force --seed >/dev/null
    # Policy is data, not credentials; no account password is created or logged.
    docker exec -i "$container" php /dev/stdin >/dev/null <<'PHP'
<?php
require '/var/www/html/backend/vendor/autoload.php';
$app = require '/var/www/html/backend/bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
foreach (['anonymous_uploads' => true, 'username_routing' => true, 'registration' => true] as $key => $value) {
    App\Models\InstanceSetting::query()->updateOrCreate(['key' => $key], ['value' => $value]);
}
PHP
    for _ in $(seq 1 60); do
        if curl --fail --silent "$url/up" >/dev/null; then
            printf '%s\n' "$url" > "$state/url"
            printf '%s\n' "$url"
            return
        fi
        sleep 1
    done
    docker logs "$container" >&2 || true
    die 'backend did not become ready'
}
stop() { docker rm -f "$container" >/dev/null 2>&1 || true; rm -rf "$state"; }
case ${1:-} in start) start ;; stop) stop ;; url) cat "$state/url" ;; *) die 'usage: peer-backend.sh {start|stop|url}' ;; esac
