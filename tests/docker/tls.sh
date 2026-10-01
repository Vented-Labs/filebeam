#!/usr/bin/env bash
# Verify orchestrator bootstrap, supplied certificates, and automatic TLS as a non-root user.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=scripts/docker/lifecycle.sh
source "$root/scripts/docker/lifecycle.sh"
image=$(docker image inspect --format '{{.Id}}' "${FILEBEAM_IMAGE:-filebeam/light:local}")
prefix="filebeam-tls-$(date +%s)-$RANDOM"
app="$prefix-app"
proxy="$prefix-proxy"
network="$prefix-network"
certs="$prefix-certs"
data="$prefix-data"
storage="$prefix-storage"
proxy_data="$prefix-proxy-data"
proxy_config="$prefix-proxy-config"

cleanup() {
    local status=$? name
    if ((status)); then
        for name in "$app" "$proxy"; do
            docker logs "$name" 2>&1 | sed -E 's/(Filebeam installation token: ).*/\1[REDACTED]/' >&2 || true
        done
    fi
    docker rm -f "$app" "$proxy" >/dev/null 2>&1 || true
    docker network rm "$network" >/dev/null 2>&1 || true
    docker volume rm "$certs" "$data" "$storage" "$proxy_data" "$proxy_config" >/dev/null 2>&1 || true
    exit "$status"
}
trap cleanup EXIT

wait_healthy() {
    local deadline=$((SECONDS + 120)) state=''
    while ((SECONDS < deadline)); do
        state=$(docker inspect --format '{{.State.Health.Status}}' "$app")
        [[ $state == healthy ]] && return 0
        [[ $(docker inspect --format '{{.State.Running}}' "$app") == true ]] || break
        sleep 2
    done
    printf 'TLS app did not become healthy (last state: %s)\n' "$state" >&2
    return 1
}

assert_clean_logs() {
    local logs
    logs=$(docker logs "$app" 2>&1 | sed -E 's/(Filebeam installation token: ).*/\1[REDACTED]/')
    if printf '%s\n' "$logs" | grep -Ei '"level":"error"|PHP Fatal error|FATAL:|ERROR:'; then
        printf '%s\n' 'Unexpected error in TLS application logs' >&2
        return 1
    fi
}

acceptance() {
    docker exec -i --user 10001:10001 \
        -e FILEBEAM_ACCEPTANCE_BASE_URL="$base_url" \
        -e FILEBEAM_ACCEPTANCE_CA_FILE="$ca_file" \
        "$app" php /dev/stdin "$@" < "$root/tests/docker/acceptance.php"
}

prepare_storage() {
    docker run --rm --user 0:0 --entrypoint sh -v "$data:/data" -v "$storage:/storage" "$image" \
        -ec 'mkdir -p /storage/primary; chown -R 10001:10001 /data /storage'
}

start_app() {
    docker run -d --name "$app" --label com.filebeam.test="$prefix" \
        --network "$network" --network-alias filebeam.test --user 10001:10001 \
        --read-only --cap-drop ALL --security-opt no-new-privileges \
        --tmpfs /run:rw,nosuid,nodev,uid=10001,gid=10001,mode=0755,size=64m \
        --tmpfs /tmp:rw,nosuid,nodev,uid=10001,gid=10001,mode=1777,size=64m \
        -v "$data:/data" -v "$storage:/storage" -v "$certs:/certs:ro" \
        -e FILEBEAM_BOOTSTRAP_ON_START=true "$@" "$image" >/dev/null
    wait_healthy
}

assert_bootstrap_preserved() {
    local before after
    before=$(docker exec "$app" sha256sum /data/config/.env /data/app/installation/state.json /data/app/installation/runtime.generation)
    docker restart "$app" >/dev/null
    wait_healthy
    after=$(docker exec "$app" sha256sum /data/config/.env /data/app/installation/state.json /data/app/installation/runtime.generation)
    [[ $before == "$after" ]]
}

assert_redirect() {
    local url=$1
    docker exec "$app" sh -ec 'curl -sS -D - -o /dev/null http://localhost:8080/install | grep -Fi "Location: $1"' tls-test "$url" >/dev/null
}

docker network create --label com.filebeam.test="$prefix" "$network" >/dev/null
for volume in "$certs" "$data" "$storage" "$proxy_data" "$proxy_config"; do
    docker volume create --label com.filebeam.test="$prefix" "$volume" >/dev/null
done

# Test-only keys are generated in an owned Docker volume, never stored in the repository.
docker run --rm --user 0:0 --entrypoint sh -v "$certs:/certs" "$image" -ec '
    openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=filebeam.test \
        -addext subjectAltName=DNS:filebeam.test -keyout /certs/tls.key -out /certs/tls.crt >/dev/null 2>&1
    chown 10001:10001 /certs/tls.key
    chmod 0600 /certs/tls.key
    chmod 0644 /certs/tls.crt
'
prepare_storage
base_url=https://filebeam.test:8443 ca_file=/certs/tls.crt
start_app -e FILEBEAM_TLS=certificate -e APP_URL=https://filebeam.test:30443 \
    -e FILEBEAM_TLS_CERT_FILE=/certs/tls.crt -e FILEBEAM_TLS_KEY_FILE=/certs/tls.key
assert_redirect https://filebeam.test:30443/install
acceptance status /install/ 200
assert_bootstrap_preserved
acceptance complete
wait_worker_ready "$app"
acceptance status /install/ 404
assert_bootstrap_preserved

# Replacing a mounted certificate must be picked up after a restart.
old_cert=$(docker exec "$app" sha256sum /certs/tls.crt)
docker run --rm --user 0:0 --entrypoint sh -v "$certs:/certs" "$image" -ec '
    openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=filebeam.test \
        -addext subjectAltName=DNS:filebeam.test -keyout /certs/tls.key -out /certs/tls.crt >/dev/null 2>&1
    chown 10001:10001 /certs/tls.key
    chmod 0600 /certs/tls.key
'
docker restart "$app" >/dev/null
wait_healthy
[[ $(docker exec "$app" sha256sum /certs/tls.crt) != "$old_cert" ]]
acceptance status /install/ 404
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null

# A key that only root can read must fail before the server is started.
docker run --rm --user 0:0 --entrypoint sh -v "$certs:/certs" "$image" -ec 'chown 0:0 /certs/tls.key'
if docker run --rm --user 10001:10001 --entrypoint /usr/local/bin/filebeam-init \
    -v "$certs:/certs:ro" -e FILEBEAM_TLS=certificate -e APP_URL=https://filebeam.test \
    -e FILEBEAM_TLS_CERT_FILE=/certs/tls.crt -e FILEBEAM_TLS_KEY_FILE=/certs/tls.key "$image" true; then
    printf '%s\n' 'An unreadable TLS key was accepted' >&2
    exit 1
fi

# Automatic TLS preserves the externally advertised port without publishing its HTTP listener.
docker volume rm "$data" "$storage" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$data" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$storage" >/dev/null
prepare_storage
base_url=https://localhost:8443 ca_file=/data/caddy/pki/authorities/local/root.crt
start_app -e FILEBEAM_TLS=auto -e APP_URL=https://localhost:30444
assert_redirect https://localhost:30444/install
acceptance status /install/ 200
acceptance complete
wait_worker_ready "$app"
assert_bootstrap_preserved
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null

# The first browser request through an explicitly trusted HTTPS proxy requires no shell bootstrap.
docker volume rm "$data" "$storage" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$data" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$storage" >/dev/null
prepare_storage
docker run --rm --entrypoint sh -v "$proxy_config:/config" caddy:2.10.2-alpine -ec '
    printf "%s\n" "{" "    skip_install_trust" "}" \
        "https://proxy.test:9443 {" "    tls internal" "    reverse_proxy filebeam.test:8080" "}" > /config/Caddyfile
'
docker run -d --name "$proxy" --label com.filebeam.test="$prefix" --network "$network" --network-alias proxy.test \
    -v "$proxy_config:/etc/caddy:ro" -v "$proxy_data:/data" caddy:2.10.2-alpine \
    caddy run --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null
proxy_ip=$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$proxy")
start_app -e FILEBEAM_TLS=proxy -e APP_URL=https://proxy.test:9443 -e FILEBEAM_TRUSTED_PROXIES="$proxy_ip"
deadline=$((SECONDS + 60))
until docker exec "$proxy" test -f /data/caddy/pki/authorities/local/root.crt; do
    ((SECONDS < deadline)) || exit 1
    sleep 1
done
docker run --rm --entrypoint sh -v "$proxy_data:/source:ro" -v "$certs:/certs" caddy:2.10.2-alpine \
    -ec 'cp /source/caddy/pki/authorities/local/root.crt /certs/proxy-ca.crt && chmod 0644 /certs/proxy-ca.crt'
base_url=https://proxy.test:9443 ca_file=/certs/proxy-ca.crt
deadline=$((SECONDS + 60))
until docker exec "$app" curl --fail --silent --show-error --cacert "$ca_file" "$base_url/up" >/dev/null 2>&1; do
    ((SECONDS < deadline)) || exit 1
    sleep 1
done
acceptance status /install/ 200
acceptance complete
wait_worker_ready "$app"
acceptance status /install/ 404
stop_after_runtime_log_check 120 "$app"
printf '%s\n' 'rootless TLS and bootstrap: passed'
