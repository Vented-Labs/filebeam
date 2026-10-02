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
        -e FILEBEAM_BOOTSTRAP_ON_START=true -e FILEBEAM_SHUTDOWN_TIMEOUT=5 "$@" "$image" >/dev/null
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

assert_no_http3() {
    docker exec "$app" sh -ec '
        url=$1 ca=$2
        shift 2
        headers=$(curl --fail --silent --show-error ${ca:+--cacert "$ca"} "$@" -D - -o /dev/null "$url/up")
        if printf "%s\n" "$headers" | grep -qi "^alt-svc:"; then
            printf "%s\n" "Unexpected HTTP/3 advertisement" >&2
            exit 1
        fi
        ! grep -qi ":20FB " /proc/net/udp /proc/net/udp6
    ' tls-test "$base_url" "$ca_file" "$@"
}

public_port() {
    # Reserve a free TCP/UDP pair on the host while selecting a high public port.
    docker run --rm --network host --entrypoint php "$image" -r '
        $tcp = stream_socket_server("tcp://127.0.0.1:0", $errno, $error);
        if ($tcp === false) { exit(1); }
        $address = stream_socket_get_name($tcp, false);
        $udp = stream_socket_server("udp://".$address, $errno, $error, STREAM_SERVER_BIND);
        if ($udp === false) { exit(1); }
        echo substr(strrchr($address, ":"), 1);
    '
}

assert_public_http3() {
    local host=$1 port=$2 install_status=$3
    # A separate client crosses Docker's published ports, rather than connecting
    # to the internal listener. Use the image's HTTP/3-capable Debian curl.
    docker run --rm --network host --entrypoint sh \
        -v "$certs:/certs:ro" -v "$data:/data:ro" "$image" -ec '
        ca=$1 host=$2 port=$3 expected=$4
        url="https://$host:$port"
        request() {
            curl --silent --show-error --noproxy "*" --connect-timeout 5 --max-time 20 \
                --cacert "$ca" --resolve "$host:$port:127.0.0.1" "$@"
        }
        test "$(request --http1.1 --fail -o /dev/null -w "%{http_version}" "$url/up")" = 1.1
        test "$(request --http2 --fail --alt-svc /tmp/alt-svc -D /tmp/headers \
            -o /dev/null -w "%{http_version}" "$url/up")" = 2
        test "$(grep -ic "^alt-svc:" /tmp/headers)" = 1
        tr -d "\r" </tmp/headers | grep -Fxi "alt-svc: h3=\":$port\"; ma=86400"
        test "$(request --http3-only --fail -o /dev/null -w "%{http_version}" "$url/up")" = 3
        test "$(request --http3-only -o /dev/null -w "%{http_code}" "$url/install/")" = "$expected"
        # Follow the real advertisement, without forcing HTTP/3 on this request.
        test "$(request --alt-svc /tmp/alt-svc --fail -o /dev/null -w "%{http_version}" "$url/up")" = 3
    ' tls-test "$ca_file" "$host" "$port" "$install_status"
}

docker network create --label com.filebeam.test="$prefix" "$network" >/dev/null
for volume in "$certs" "$data" "$storage" "$proxy_data" "$proxy_config"; do
    docker volume create --label com.filebeam.test="$prefix" "$volume" >/dev/null
done

if docker run --rm -e FILEBEAM_HTTP3=invalid "$image" true; then
    printf '%s\n' 'An invalid HTTP/3 setting was accepted' >&2
    exit 1
fi

# Test-only keys are generated in an owned Docker volume, never stored in the repository.
docker run --rm --user 0:0 --entrypoint sh -v "$certs:/certs" "$image" -ec '
    openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=filebeam.test \
        -addext subjectAltName=DNS:filebeam.test,IP:192.0.2.10 -keyout /certs/tls.key -out /certs/tls.crt >/dev/null 2>&1
    chown 10001:10001 /certs/tls.key
    chmod 0600 /certs/tls.key
    chmod 0644 /certs/tls.crt
'
prepare_storage
base_url=https://filebeam.test:8443 ca_file=/certs/tls.crt
start_app -e FILEBEAM_TLS=certificate -e APP_URL=https://filebeam.test:30443 \
    -e FILEBEAM_TLS_CERT_FILE=/certs/tls.crt -e FILEBEAM_TLS_KEY_FILE=/certs/tls.key
assert_redirect https://filebeam.test:30443/install
assert_no_http3
acceptance status /install/ 200
assert_bootstrap_preserved
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null
port=$(public_port)
start_app -e FILEBEAM_TLS=certificate -e FILEBEAM_HTTP3=true -e APP_URL="https://filebeam.test:$port" \
    -e FILEBEAM_TLS_CERT_FILE=/certs/tls.crt -e FILEBEAM_TLS_KEY_FILE=/certs/tls.key \
    -p "127.0.0.1:$port:8443/tcp" -p "127.0.0.1:$port:8443/udp"
assert_redirect "https://filebeam.test:$port/install"
assert_public_http3 filebeam.test "$port" 200
acceptance complete
wait_worker_ready "$app"
acceptance status /install/ 404
assert_public_http3 filebeam.test "$port" 404
assert_bootstrap_preserved

# Replacing a mounted certificate must be picked up after a restart.
old_cert=$(docker exec "$app" sha256sum /certs/tls.crt)
docker run --rm --user 0:0 --entrypoint sh -v "$certs:/certs" "$image" -ec '
    openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=filebeam.test \
        -addext subjectAltName=DNS:filebeam.test,IP:192.0.2.10 -keyout /certs/tls.key -out /certs/tls.crt >/dev/null 2>&1
    chown 10001:10001 /certs/tls.key
    chmod 0600 /certs/tls.key
'
docker restart "$app" >/dev/null
wait_healthy
[[ $(docker exec "$app" sha256sum /certs/tls.crt) != "$old_cert" ]]
acceptance status /install/ 404
assert_public_http3 filebeam.test "$port" 404
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null

# Even an opted-in deployment without UDP forwarding must still serve TCP.
start_app -e FILEBEAM_TLS=certificate -e FILEBEAM_HTTP3=true -e APP_URL="https://filebeam.test:$port" \
    -e FILEBEAM_TLS_CERT_FILE=/certs/tls.crt -e FILEBEAM_TLS_KEY_FILE=/certs/tls.key \
    -p "127.0.0.1:$port:8443/tcp"
docker run --rm --network host --entrypoint sh -v "$certs:/certs:ro" "$image" -ec '
    test "$(curl --http3 --fail --silent --show-error --noproxy "*" --max-time 15 \
        --cacert /certs/tls.crt --resolve "filebeam.test:$1:127.0.0.1" \
        -o /dev/null -w "%{http_version}" "https://filebeam.test:$1/up")" = 2
' tls-test "$port"
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null

# IP clients omit SNI, and Docker's listener address differs from the public IP.
start_app -e FILEBEAM_TLS=certificate -e FILEBEAM_HTTP3=false -e APP_URL=https://192.0.2.10:30443 \
    -e FILEBEAM_TLS_CERT_FILE=/certs/tls.crt -e FILEBEAM_TLS_KEY_FILE=/certs/tls.key
base_url=https://192.0.2.10:8443
assert_no_http3 --resolve 192.0.2.10:8443:127.0.0.1
# A stopped queue worker cannot process SIGTERM; shutdown must still honor its budget.
queue_pid=$(docker exec "$app" pgrep -o -f 'artisan queue:work')
docker exec "$app" kill -STOP "$queue_pid"
stop_started=$SECONDS
stop_after_runtime_log_check 120 "$app"
((SECONDS - stop_started < 15))
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
assert_no_http3
acceptance status /install/ 200
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null
# The implicit public port comes from the HTTPS origin, not the 8443 listener.
start_app -e FILEBEAM_TLS=auto -e FILEBEAM_HTTP3=true -e FILEBEAM_SERVER_NAME=localhost
docker exec "$app" sh -ec 'curl --fail --silent --show-error --cacert "$1" -D - -o /dev/null "$2/up" | grep -F "h3=\":443\"; ma=86400"' tls-test "$ca_file" "$base_url"
stop_after_runtime_log_check 120 "$app"
docker rm "$app" >/dev/null
port=$(public_port)
start_app -e FILEBEAM_TLS=auto -e FILEBEAM_HTTP3=true -e APP_URL="https://localhost:$port" \
    -p "127.0.0.1:$port:8443/tcp" -p "127.0.0.1:$port:8443/udp"
assert_redirect "https://localhost:$port/install"
assert_public_http3 localhost "$port" 200
acceptance complete
wait_worker_ready "$app"
assert_public_http3 localhost "$port" 404
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
start_app -e FILEBEAM_TLS=proxy -e FILEBEAM_HTTP3=true -e APP_URL=https://proxy.test:9443 -e FILEBEAM_TRUSTED_PROXIES="$proxy_ip"
base_url=http://localhost:8080 ca_file=''
assert_no_http3
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
# Configured first-run credentials must not appear in supervisor logs or survive in runtime env.
docker rm "$app" >/dev/null
docker volume rm "$data" "$storage" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$data" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$storage" >/dev/null
prepare_storage
docker run --rm --user 0:0 --entrypoint sh -v "$certs:/certs" "$image" -ec '
    php -r '\''file_put_contents("/certs/setup-token", bin2hex(random_bytes(32)));'\''
    chown 10001:10001 /certs/setup-token
    chmod 0600 /certs/setup-token
'
start_app -e FILEBEAM_TLS=proxy -e FILEBEAM_SETUP_TOKEN_FILE=/certs/setup-token
base_url=http://localhost:8080 ca_file=''
assert_no_http3
docker exec --user 10001:10001 "$app" php -r '
    require "/opt/filebeam/backend/vendor/autoload.php";
    $env = Dotenv\Dotenv::parse(file_get_contents("/data/config/.env"));
    exit(hash_equals(file_get_contents("/certs/setup-token"), $env["FILEBEAM_INSTALL_TOKEN"] ?? "") ? 0 : 1);
'
if docker logs "$app" 2>&1 | grep -q '^Filebeam installation token:'; then
    printf '%s\n' 'Configured setup token was logged' >&2
    exit 1
fi
supervisor_pid=$(docker exec "$app" pgrep -o -f '^/bin/sh /usr/local/bin/filebeam-supervisor')
docker exec "$app" php -r '
    $env = file_get_contents("/proc/".$argv[1]."/environ");
    exit(str_contains($env, "FILEBEAM_SETUP_TOKEN=") || str_contains($env, "FILEBEAM_SETUP_TOKEN_FILE=") ? 1 : 0);
' "$supervisor_pid"
stop_after_runtime_log_check 120 "$app"
printf '%s\n' 'rootless TLS and bootstrap: passed'
