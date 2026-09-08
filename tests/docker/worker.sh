#!/usr/bin/env bash
# Exercises real long-lived FrankenPHP worker request isolation in a fresh light image.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
run_id="filebeam-worker-$(date +%s)-${RANDOM}"
container="$run_id-app"
data_volume="$run_id-data"
storage_volume="$run_id-storage"
keep=false
current_variant=${FILEBEAM_TEST_VARIANT:-light}

usage() { printf '%s\n' "Usage: tests/docker/worker.sh [--keep]"; }
while (($#)); do
    case "$1" in
        --keep) keep=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done

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
    if ((status)); then
        redacted_logs >&2 || true
        printf 'Kept owned objects after failure: container=%s data=%s storage=%s\n' "$container" "$data_volume" "$storage_volume" >&2
        exit "$status"
    fi
    if [[ $keep == true ]]; then
        printf 'Kept owned objects: container=%s data=%s storage=%s\n' "$container" "$data_volume" "$storage_volume" >&2
    else
        if ! docker stop -t 120 "$container" >/dev/null; then
            status=1
        elif [[ $(docker inspect --format '{{.State.ExitCode}}' "$container") != 0 ]]; then
            printf '%s\n' 'Container did not exit cleanly' >&2
            status=1
        elif ! assert_clean_logs; then
            status=1
        fi
        if ((status)); then
            redacted_logs >&2 || true
            printf 'Kept owned objects after cleanup failure: container=%s data=%s storage=%s\n' "$container" "$data_volume" "$storage_volume" >&2
            exit "$status"
        fi
        docker rm "$container" >/dev/null
        docker volume rm "$data_volume" "$storage_volume" >/dev/null
    fi
    exit "$status"
}
trap cleanup EXIT

wait_healthy() {
    local deadline=$((SECONDS + 120)) state=''
    while ((SECONDS < deadline)); do
        state=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$container")
        [[ $state == healthy ]] && return 0
        sleep 2
    done
    printf 'Health check did not become healthy (last: %s)\n' "$state" >&2
    return 1
}

wait_worker_mode() {
    local deadline=$((SECONDS + 60)) active_config=''
    while ((SECONDS < deadline)); do
        active_config=$(docker exec "$container" curl --silent --show-error http://127.0.0.1:2019/config/ 2>/dev/null || true)
        [[ $active_config == *'"file_name":"/opt/filebeam/backend/public/frankenphp-worker.php","num":1'* ]] && return 0
        sleep 1
    done
    printf '%s\n' 'Active Caddy configuration did not enter one-worker PHP mode' >&2
    return 1
}

acceptance() {
    docker exec -i --user 10001:10001 "$container" php /dev/stdin "$@" < "$root/tests/docker/acceptance.php"
}

worker_client() {
    docker exec -i --user 10001:10001 "$container" php /dev/stdin "$@" < "$root/tests/docker/worker.php"
}

command -v docker >/dev/null
docker info >/dev/null
image=${FILEBEAM_IMAGE:-filebeam/light:local}
image_id=$(docker image inspect --format '{{.Id}}' "$image")

docker volume create --label com.filebeam.test="$run_id" "$data_volume" >/dev/null
docker volume create --label com.filebeam.test="$run_id" "$storage_volume" >/dev/null
docker run -d --name "$container" --label com.filebeam.test="$run_id" --read-only \
    --tmpfs /run:rw,nosuid,nodev,size=64m --tmpfs /tmp:rw,nosuid,nodev,size=64m -p 127.0.0.1::8080 \
    -e FILEBEAM_CONTAINER=true -e FILEBEAM_VARIANT="$current_variant" -e FILEBEAM_TLS=proxy \
    -e FILEBEAM_WORKERS=1 -e FILEBEAM_THREADS=2 -e MAX_REQUESTS=100000 \
    -v "$data_volume:/data" -v "$storage_volume:/storage" "$image_id" >/dev/null
wait_healthy

# Use the production installer only through its HTTP contract, with localhost as its instance URL.
docker exec "$container" test ! -e /data/config/.env
acceptance bootstrap
acceptance complete
acceptance status /install/ 404
wait_healthy
wait_worker_mode
docker exec --user 10001:10001 "$container" sh -ec 'test -d /storage/primary; test ! -w /opt/filebeam/backend; touch /storage/primary/.worker-fixture'
worker_client seed
worker_client runtime

docker exec "$container" sh -ec 'test "$FILEBEAM_THREADS" = 2; test "$MAX_REQUESTS" = 100000'
worker_pid=$(docker exec "$container" sh -ec 'pgrep -x frankenphp | tr "\n" " "')
[[ $worker_pid =~ ^[0-9]+\ $ ]] || { printf 'Expected exactly one frankenphp process, found: %s\n' "$worker_pid" >&2; exit 1; }
worker_pid=${worker_pid% }

worker_client warm >/dev/null
before_fd=$(docker exec "$container" sh -ec "ls /proc/$worker_pid/fd | wc -l")
before_rss=$(docker exec "$container" sh -ec "ps -o rss= -p $worker_pid | tr -d ' '")
result=$(worker_client isolation)
after_fd=$(docker exec "$container" sh -ec "ls /proc/$worker_pid/fd | wc -l")
after_rss=$(docker exec "$container" sh -ec "ps -o rss= -p $worker_pid | tr -d ' '")
docker exec "$container" sh -ec "kill -0 $worker_pid"
[[ $(docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$container" | grep -cx 'MAX_REQUESTS=100000') -eq 1 ]]

((after_fd <= before_fd + 16)) || { printf 'FD growth exceeded bound: %s -> %s\n' "$before_fd" "$after_fd" >&2; exit 1; }
((after_rss <= before_rss + 131072)) || { printf 'RSS growth exceeded bound: %s -> %s KiB\n' "$before_rss" "$after_rss" >&2; exit 1; }
printf '%s\n' "$result"
printf 'worker-isolation: image=%s worker_pid=%s workers=1 threads=2 max_requests=100000 fd=%s->%s rss_kib=%s->%s\n' "$image_id" "$worker_pid" "$before_fd" "$after_fd" "$before_rss" "$after_rss"
