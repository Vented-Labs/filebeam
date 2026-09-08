#!/usr/bin/env bash
# Docker-only production-image acceptance tests. Secrets stay inside the app container.
set -euo pipefail

usage() { printf '%s\n' 'Usage: scripts/docker/test.sh [--variant all|light|omnibus] [--suite smoke|full] [--keep]'; }
variant=all suite=smoke keep=false
while (($#)); do
    case "$1" in
        --variant) variant=${2:?missing variant}; shift 2 ;;
        --suite) suite=${2:?missing suite}; shift 2 ;;
        --keep) keep=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
case "$variant" in all|light|omnibus) ;; *) usage >&2; exit 2 ;; esac
case "$suite" in smoke|full) ;; *) usage >&2; exit 2 ;; esac
[[ -z ${FILEBEAM_IMAGE:-} || $variant != all ]] || { printf '%s\n' 'FILEBEAM_IMAGE requires one variant'; exit 2; }
command -v docker >/dev/null && docker info >/dev/null

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=scripts/docker/lifecycle.sh
source "$(dirname "${BASH_SOURCE[0]}")/lifecycle.sh"
run_id="filebeam-docker-test-$(date +%s)-$RANDOM"
container='' data_volume='' storage_volume='' runner_volume=''

redacted_logs() {
    docker logs "$container" 2>&1 | sed -E 's/(Filebeam installation token: ).*/\1[REDACTED]/'
}
assert_clean_logs() {
    local logs
    logs=$(redacted_logs)
    if printf '%s\n' "$logs" | grep -Ei '"level":"error"|PHP Fatal error|FATAL:|ERROR:'; then
        printf '%s\n' 'Unexpected error in container lifecycle logs' >&2
        return 1
    fi
}
cleanup() {
    local status=$?
    if ((status)) && [[ -n $container ]]; then redacted_logs >&2 || true; fi
    if [[ $keep == false ]]; then
        [[ -n $container ]] && docker rm -f "$container" >/dev/null 2>&1 || true
        [[ -n $data_volume ]] && docker volume rm "$data_volume" >/dev/null 2>&1 || true
        [[ -n $storage_volume ]] && docker volume rm "$storage_volume" >/dev/null 2>&1 || true
        [[ -n $runner_volume ]] && docker volume rm "$runner_volume" >/dev/null 2>&1 || true
    else
        printf 'Kept owned objects: container=%s data=%s storage=%s runner=%s\n' "$container" "$data_volume" "$storage_volume" "$runner_volume" >&2
    fi
    exit "$status"
}
trap cleanup EXIT

wait_healthy() {
    local deadline=$((SECONDS + 120)) state
    while ((SECONDS < deadline)); do
        state=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$container")
        [[ $state == healthy ]] && return 0
        sleep 2
    done
    printf 'Health check did not become healthy (last: %s)\n' "$state" >&2
    return 1
}

php_in_app() { docker exec --user 10001:10001 "$container" php -d display_errors=0 -r "$1"; }
php_client() { docker exec -i --user 10001:10001 -e FILEBEAM_ACCEPTANCE_VISIBILITY="$test_visibility" "$container" php /dev/stdin "$@" < "$root/tests/docker/acceptance.php"; }
http_status() { php_client status "$1" "$2"; }

create_container() {
    local image=$1 current=$2
    docker run -d --name "$container" --label com.filebeam.test="$run_id" --read-only --tmpfs /run:rw,nosuid,nodev,size=64m --tmpfs /tmp:rw,nosuid,nodev,size=64m -p 127.0.0.1::8080 \
        -e FILEBEAM_CONTAINER=true -e FILEBEAM_VARIANT="$current" -e FILEBEAM_TLS=proxy \
        -v "$data_volume:/data" -v "$storage_volume:/storage" "$image" >/dev/null
    wait_healthy
}

install_local() {
    docker exec "$container" test ! -e /data/config/.env
    http_status /install/ 200
    php_client bootstrap
    # The token is intentionally checked only in the runtime log and never enters shell output.
    local token_deadline=$((SECONDS + 15))
    until docker logs "$container" 2>&1 | grep -q '^Filebeam installation token: [^[:space:]]\+$'; do
        ((SECONDS < token_deadline)) || { printf '%s\n' 'Installer token was not logged' >&2; return 1; }
        sleep 1
    done
    php_client complete
    http_status /install/ 404
    php_client status-protected /.env
    php_client status-protected /config/.env
    php_client status-protected /storage
    docker exec --user 10001:10001 "$container" sh -ec 'test -d /storage/primary; test ! -w /opt/filebeam/backend; touch /storage/primary/.persistence-fixture'
    # shellcheck disable=SC2016 # The PHP client needs literal $ variables.
    php_in_app 'require "/opt/filebeam/backend/vendor/autoload.php"; $app = require "/opt/filebeam/backend/bootstrap/app.php"; $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap(); exit(App\Models\User::query()->where("username", "acceptance")->exists() ? 0 : 1);'
    wait_worker_ready "$container"
    docker exec --user 10001:10001 "$container" sh -ec 'cd /opt/filebeam/backend && php artisan schedule:list --no-interaction >/dev/null'
}

verify_recreate_and_shutdown() {
    stop_after_runtime_log_check 120 "$container"
    docker rm "$container" >/dev/null
    create_container "$current_image" "$current_variant"
    http_status /install/ 404
    docker exec --user 10001:10001 "$container" test -f /storage/primary/.persistence-fixture
    # shellcheck disable=SC2016 # The PHP client needs literal $ variables.
    php_in_app 'require "/opt/filebeam/backend/vendor/autoload.php"; $app = require "/opt/filebeam/backend/bootstrap/app.php"; $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap(); exit(App\Models\User::query()->where("username", "acceptance")->exists() ? 0 : 1);'
}

verify_omnibus() {
    [[ $current_variant == omnibus ]] || return 0
    docker exec "$container" sh -ec 'test -S /run/filebeam/postgresql/.s.PGSQL.5432; test -S /run/filebeam/valkey/valkey.sock; ! grep -Eq ":1538[[:space:]]+.*[[:space:]]0A[[:space:]]" /proc/net/tcp; ! grep -Eq ":18EB[[:space:]]+.*[[:space:]]0A[[:space:]]" /proc/net/tcp'
    # shellcheck disable=SC2016
    php_in_app '$pdo = new PDO("pgsql:host=/run/filebeam/postgresql;port=5432;dbname=filebeam", "filebeam"); exit($pdo->query("select inet_server_addr() is null")->fetchColumn() ? 0 : 1);'
    docker exec --user 10002:10002 "$container" sh -ec 'test "$(psql -X -w -h /run/filebeam/postgresql -U filebeampg -d filebeam -Atqc "SHOW server_encoding")" = UTF8'
    php_client env-equals REDIS_HOST /run/filebeam/valkey/valkey.sock
    php_client env-equals REDIS_PORT 0
    docker exec "$container" sh -ec 'kill -TERM "$(pgrep -o postgres)"'
    local deadline=$((SECONDS + 30))
    while ((SECONDS < deadline)); do
        [[ $(docker inspect --format '{{.State.Running}}' "$container") == false ]] && return 0
        sleep 1
    done
    return 1
}

run_browser() {
    runner_volume="$run_id-browser"
    docker volume create --label com.filebeam.test="$run_id" "$runner_volume" >/dev/null
    docker run --rm -v "$root:/source:ro" -v "$runner_volume:/work" node:24-trixie sh -ec 'mkdir -p /work/backend /work/ui /work/tests/browser; cp /source/package.json /source/package-lock.json /source/playwright.config.ts /work/; cp /source/backend/package.json /work/backend/; cp /source/ui/package.json /work/ui/; cp -a /source/tests/browser/. /work/tests/browser/; cd /work && npm ci'
    docker run --rm --network "container:$container" --ipc=host -v "$runner_volume:/work" -w /work -e BASE_URL=http://localhost:8080 node:24-trixie sh -ec 'npx playwright install --with-deps chromium && npx playwright test --workers=1 tests/browser/transfers.spec.ts'
    local before_fd after_fd before_rss after_rss
    before_fd=$(docker exec "$container" sh -ec 'for p in $(pgrep -f "frankenphp.*run"); do ls "/proc/$p/fd"; done | wc -l')
    before_rss=$(docker exec "$container" sh -ec 'ps -eo rss,args | awk "/frankenphp.*run/ {sum += \$1} END {print sum + 0}"')
    php_client soak
    after_fd=$(docker exec "$container" sh -ec 'for p in $(pgrep -f "frankenphp.*run"); do ls "/proc/$p/fd"; done | wc -l')
    after_rss=$(docker exec "$container" sh -ec 'ps -eo rss,args | awk "/frankenphp.*run/ {sum += \$1} END {print sum + 0}"')
    ((after_fd <= before_fd + 16)) || { printf 'FD soak growth: %s -> %s\n' "$before_fd" "$after_fd" >&2; return 1; }
    ((after_rss <= before_rss + 131072)) || { printf 'RSS soak growth: %s -> %s KiB\n' "$before_rss" "$after_rss" >&2; return 1; }
}

run_variant() {
    current_variant=$1 test_visibility=private
    current_image=$(docker image inspect --format '{{.Id}}' "${FILEBEAM_IMAGE:-filebeam/$1:local}")
    [[ $suite == full ]] && test_visibility=public
    container="$run_id-$current_variant" data_volume="$run_id-$current_variant-data" storage_volume="$run_id-$current_variant-storage"
    docker volume create --label com.filebeam.test="$run_id" "$data_volume" >/dev/null
    docker volume create --label com.filebeam.test="$run_id" "$storage_volume" >/dev/null
    create_container "$current_image" "$current_variant"
    install_local
    verify_recreate_and_shutdown
    if [[ $suite == full ]]; then
        run_browser
        FILEBEAM_IMAGE="$current_image" FILEBEAM_TEST_VARIANT="$current_variant" bash "$root/tests/docker/worker.sh"
        if [[ $current_variant == light ]]; then
            FILEBEAM_IMAGE="$current_image" bash "$root/tests/docker/services.sh"
        fi
    fi
    verify_omnibus
    if [[ $current_variant == light ]]; then
        stop_after_runtime_log_check 120 "$container"
    fi
    docker rm -f "$container" >/dev/null 2>&1 || true
    docker volume rm "$data_volume" "$storage_volume" >/dev/null
    container='' data_volume='' storage_volume=''
}

if [[ $variant == all ]]; then run_variant light; run_variant omnibus; else run_variant "$variant"; fi
