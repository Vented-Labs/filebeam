#!/usr/bin/env bash
# Verify configurable listeners and two instances sharing one network namespace.
set -euo pipefail

image=$(docker image inspect --format '{{.Id}}' "${FILEBEAM_IMAGE:-filebeam/light:local}")
prefix="filebeam-network-$(date +%s)-$RANDOM"
first="$prefix-first"
second="$prefix-second"
rejected="$prefix-rejected"

cleanup() {
    local status=$? name
    if ((status)); then
        for name in "$first" "$second" "$rejected"; do
            docker logs "$name" 2>&1 | sed -E 's/(Filebeam installation token: ).*/\1[REDACTED]/' >&2 || true
        done
    fi
    docker rm -f "$first" "$second" "$rejected" >/dev/null 2>&1 || true
    exit "$status"
}
trap cleanup EXIT

start() {
    local name=$1
    shift
    docker run -d --name "$name" --label com.filebeam.test="$prefix" --read-only \
        --tmpfs /run:rw,nosuid,nodev,size=64m --tmpfs /tmp:rw,nosuid,nodev,size=64m \
        --tmpfs /data:rw,nosuid,nodev,size=128m --tmpfs /storage:rw,nosuid,nodev,size=16m \
        -e FILEBEAM_SHUTDOWN_TIMEOUT=10 "$@" "$image" >/dev/null
}

wait_healthy() {
    local name=$1 deadline=$((SECONDS + 120)) state=''
    while ((SECONDS < deadline)); do
        state=$(docker inspect --format '{{.State.Health.Status}}' "$name")
        [[ $state == healthy ]] && return 0
        [[ $(docker inspect --format '{{.State.Running}}' "$name") == true ]] || break
        sleep 2
    done
    printf '%s did not become healthy (last state: %s)\n' "$name" "$state" >&2
    return 1
}

# Succeeds when a TCP socket is listening on the port in the container's namespace.
listening() {
    docker exec "$1" sh -ec 'port=$(printf "%04X" "$1"); grep -Eq "^ *[0-9]+: [0-9A-F]+:$port [0-9A-F]+:0000 0A " /proc/net/tcp /proc/net/tcp6' listening "$2"
}

assert_instance() {
    local name=$1 http_port=$2
    listening "$name" "$http_port"
    docker exec "$name" /usr/local/bin/filebeam-healthcheck
    docker exec "$name" curl -fsS --max-time 5 -o /dev/null "http://127.0.0.1:$http_port/up"
    # The control API is a private socket that reports this instance's listener.
    docker exec --user 10001:10001 "$name" curl -fsS --max-time 5 --unix-socket /run/filebeam/caddy/admin.sock http://localhost/config/apps/http/servers \
        | grep -Fq "\":$http_port\""
}

# Stops through the supervisor; the control API drains Caddy without a SIGTERM fallback.
assert_supervised_stop() {
    local name=$1
    docker stop -t 60 "$name" >/dev/null
    [[ $(docker inspect --format '{{.State.ExitCode}}' "$name") == 0 ]] || { printf '%s did not exit cleanly\n' "$name" >&2; return 1; }
    if docker logs "$name" 2>&1 | grep -Ei 'SIGTERM|"level":"error"|PHP Fatal error'; then
        printf '%s did not stop through the control API\n' "$name" >&2
        return 1
    fi
}

start "$rejected" -e FILEBEAM_HTTP_PORT=80
[[ $(docker wait "$rejected") == 64 ]] || { printf '%s\n' 'Privileged HTTP port was accepted' >&2; exit 1; }
docker logs "$rejected" 2>&1 | grep -Fq 'FILEBEAM_HTTP_PORT must be an integer from 1024 to 65535.'

start "$first" -e FILEBEAM_TLS=proxy -e FILEBEAM_HTTP_PORT=18080
wait_healthy "$first"
assert_instance "$first" 18080
for port in 8080 8443 2019; do
    if listening "$first" "$port"; then printf 'Default port %s is bound\n' "$port" >&2; exit 1; fi
done

# The second instance also moves its direct HTTPS listener.
start "$second" --network "container:$first" -e FILEBEAM_TLS=auto -e APP_URL=https://localhost:28443 \
    -e FILEBEAM_HTTP_PORT=28080 -e FILEBEAM_HTTPS_PORT=28443
wait_healthy "$second"
assert_instance "$second" 28080
docker exec "$second" curl -fsS --max-time 5 --cacert /data/caddy/pki/authorities/local/root.crt -o /dev/null https://localhost:28443/up
assert_instance "$first" 18080
for port in 8080 8443 2019; do
    if listening "$first" "$port"; then printf 'Default port %s is bound\n' "$port" >&2; exit 1; fi
done

assert_supervised_stop "$second"
assert_instance "$first" 18080
assert_supervised_stop "$first"
