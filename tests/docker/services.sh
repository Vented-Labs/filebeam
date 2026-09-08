#!/usr/bin/env bash
# Extra Docker-only acceptance coverage for the light image and external services.
set -euo pipefail

image=${FILEBEAM_IMAGE:-filebeam/light:local}
keep=false
while (($#)); do
    case "$1" in
        --keep) keep=true; shift ;;
        *) printf 'Usage: %s [--keep]\n' "${BASH_SOURCE[0]}" >&2; exit 2 ;;
    esac
done
image_id=$(docker image inspect --format '{{.Id}}' "$image")
image=$image_id
printf 'Testing image: %s\n' "$image_id" >&2
prefix="filebeam-services-${RANDOM}"
network="$prefix-network"
app="$prefix-app"
tls_app="$prefix-tls"
postgres="$prefix-postgres"
minio="$prefix-minio"
proxy="$prefix-s3-proxy"
data_volume="$prefix-data"
tls_data_volume="$prefix-tls-data"
tls_storage_volume="$prefix-tls-storage"
proxy_config_volume="$prefix-proxy-config"
proxy_data_volume="$prefix-proxy-data"
ca_volume="$prefix-ca"
ini_volume="$prefix-ini"

# Keep this aligned with the already-pinned CI MinIO release. Service images are
# explicit releases so a local acceptance run is reproducible.
minio_image='minio/minio:RELEASE.2025-04-22T22-12-26Z'
mc_image='minio/mc:RELEASE.2025-03-12T17-29-24Z'
postgres_image='postgres:18.6-trixie'
caddy_image='caddy:2.10.2-alpine'

redact() {
    sed -E \
        -e 's/(Filebeam installation token: )[[:graph:]]+/\1[REDACTED]/g' \
        -e 's/(MINIO_ROOT_PASSWORD=|DB_PASSWORD=|REDIS_PASSWORD=|AWS_SECRET_ACCESS_KEY=|X-Installation-Token: )[[:graph:]]+/\1[REDACTED]/g'
}

logs() {
    local name
    for name in "$app" "$tls_app" "$postgres" "$minio" "$proxy"; do
        docker inspect "$name" >/dev/null 2>&1 || continue
        printf '%s\n' "--- $name ---" >&2
        docker logs "$name" 2>&1 | redact >&2 || true
    done
}

cleanup() {
    local status=$?
    if ((status)); then logs; fi
    if [[ $keep == true ]]; then
        printf 'Kept owned objects: network=%s containers=%s,%s,%s,%s,%s volumes=%s,%s,%s,%s,%s,%s,%s\n' "$network" "$app" "$tls_app" "$postgres" "$minio" "$proxy" "$data_volume" "$tls_data_volume" "$tls_storage_volume" "$proxy_config_volume" "$proxy_data_volume" "$ca_volume" "$ini_volume" >&2
    else
        docker rm -f "$app" "$tls_app" "$postgres" "$minio" "$proxy" >/dev/null 2>&1 || true
        docker network rm "$network" >/dev/null 2>&1 || true
        docker volume rm "$data_volume" "$tls_data_volume" "$tls_storage_volume" "$proxy_config_volume" "$proxy_data_volume" "$ca_volume" "$ini_volume" >/dev/null 2>&1 || true
    fi
    exit "$status"
}
trap cleanup EXIT

wait_for() {
    local description=$1 command=$2 deadline=$((SECONDS + 120))
    until eval "$command" >/dev/null 2>&1; do
        ((SECONDS < deadline)) || { printf 'Timed out waiting for %s\n' "$description" >&2; return 1; }
        sleep 2
    done
}

wait_healthy() {
    local name=$1 state='' deadline=$((SECONDS + 120))
    while ((SECONDS < deadline)); do
        state=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$name")
        [[ $state == healthy ]] && return 0
        sleep 2
    done
    printf '%s did not become healthy (last state: %s)\n' "$name" "$state" >&2
    return 1
}

wait_unhealthy() {
    local name=$1 state='' deadline=$((SECONDS + 45))
    while ((SECONDS < deadline)); do
        state=$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$name")
        [[ $state == unhealthy ]] && return 0
        sleep 1
    done
    printf '%s did not become unhealthy (last state: %s)\n' "$name" "$state" >&2
    return 1
}

assert_normal_logs() {
    local name=$1
    if docker logs "$name" 2>&1 | redact | grep -Ei '"level":"(warn|error)"|PHP (Warning|Fatal error)|WARNING:|FATAL:|ERROR:'; then
        printf 'Unexpected warning or error in normal %s logs\n' "$name" >&2
        return 1
    fi
}

acceptance() {
    docker exec -i --user 10001:10001 "$app" php /dev/stdin "$@" < "$(dirname "${BASH_SOURCE[0]}")/acceptance.php"
}

tls_acceptance() {
    docker exec -i --user 10001:10001 "$tls_app" php /dev/stdin "$@" < "$(dirname "${BASH_SOURCE[0]}")/acceptance.php"
}

installer_s3() {
    # This uses the real installer HTTP endpoints. Configuration defaults are
    # retained so the container controller, rather than the test, owns managed
    # PostgreSQL and file-cache settings.
    docker exec -i --user 10001:10001 -e INSTALL_BASE=http://localhost:8080 "$app" php /dev/stdin <<'PHP'
<?php
declare(strict_types=1);

$base = getenv('INSTALL_BASE');
function call(string $method, string $path, ?array $payload, int $expected): array {
    global $base;
    $headers = ['Accept: application/json', 'Origin: '.$base];
    $body = $payload === null ? '' : json_encode($payload, JSON_THROW_ON_ERROR);
    if ($payload !== null) $headers[] = 'Content-Type: application/json';
    if ($path !== '/install/bootstrap') {
        require_once '/opt/filebeam/backend/vendor/autoload.php';
        $env = Dotenv\Dotenv::parse((string) file_get_contents('/data/config/.env'));
        $headers[] = 'X-Installation-Token: '.($env['FILEBEAM_INSTALL_TOKEN'] ?? '');
    }
    $context = stream_context_create(['http' => ['method' => $method, 'ignore_errors' => true, 'header' => $headers, 'content' => $body]]);
    $response = @file_get_contents($base.$path, false, $context);
    preg_match('~\s(\d{3})\s~', $http_response_header[0] ?? '', $match);
    $status = (int) ($match[1] ?? 0);
    if ($status !== $expected) fwrite(STDERR, "$method $path returned HTTP $status (expected $expected): ".($response ?: '[empty]')."\n");
    if ($status !== $expected) exit(1);
    return json_decode($response ?: '{}', true, flags: JSON_THROW_ON_ERROR);
}

$local = call('POST', '/install/storage', ['storage' => ['name' => 'Local storage', 'driver' => 'local', 'root' => 'primary']], 422);
if (! str_contains(json_encode($local, JSON_THROW_ON_ERROR), 'filestore mount is unavailable or not writable')) {
    fwrite(STDERR, "Local storage probe did not report the missing mount\n"); exit(1);
}
$configuration = call('POST', '/install/configuration', [], 200);
$defaults = $configuration['defaults'] ?? [];
$database = $defaults['database'] ?? null;
$cache = $defaults['cache'] ?? null;
$chunks = $configuration['chunks'] ?? null;
if (! is_array($database) || ! is_array($cache) || ! is_array($chunks) || ! is_int($chunks['recommended'] ?? null)) exit(1);
$storage = ['name' => 'S3 storage', 'driver' => 's3', 'bucket' => 'filebeam', 'key' => 'filebeam-test', 'secret' => 'filebeam-test-secret', 'region' => 'us-east-1', 'endpoint' => 'https://s3-proxy:9443', 'use_path_style_endpoint' => true];
call('POST', '/install/database', ['database' => $database], 200);
call('POST', '/install/cache', ['cache' => $cache], 200);
call('POST', '/install/storage', ['storage' => $storage], 200);
$size = $chunks['recommended'];
$token = Dotenv\Dotenv::parse((string) file_get_contents('/data/config/.env'))['FILEBEAM_INSTALL_TOKEN'];
$context = stream_context_create(['http' => ['method' => 'PUT', 'ignore_errors' => true, 'header' => ['Accept: application/json', 'Origin: '.$base, 'Content-Type: application/octet-stream', 'Content-Length: '.$size, 'X-Installation-Token: '.$token], 'content' => str_repeat("\0", $size)]]);
@file_get_contents($base.'/install/probe', false, $context);
preg_match('~\s(\d{3})\s~', $http_response_header[0] ?? '', $match);
if ((int) ($match[1] ?? 0) !== 200) exit(1);
$payload = ['database' => $database, 'cache' => $cache, 'instance' => ['name' => 'Filebeam services acceptance', 'url' => 'https://localhost:8443', 'username_domain' => '', 'visibility' => 'public', 'auto_updates_enabled' => false], 'storage' => [$storage], 'placement_mode' => 'replicate', 'admin' => ['name' => 'Acceptance Admin', 'username' => 'acceptance', 'email' => 'acceptance@example.test', 'password' => 'Acceptance-password-123!', 'password_confirmation' => 'Acceptance-password-123!', 'email_ownership_confirmed' => true], 'chunk_max_size' => $size, 'chunk_warning_acknowledged' => true];
call('POST', '/install/complete', $payload, 200);
PHP
}

diagnose_s3() {
    # Do not print request traces: AWS exceptions can include signed URLs. The
    # exception class and message identify TLS, endpoint, and adapter failures.
    docker exec -i --user 10001:10001 "$app" php /dev/stdin <<'PHP' | redact
<?php
declare(strict_types=1);

require '/opt/filebeam/backend/vendor/autoload.php';
$app = require '/opt/filebeam/backend/bootstrap/app.php';
$app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap();
$store = new App\Models\Filestore(['source' => 'database', 'driver' => 's3', 'configuration' => ['key' => 'filebeam-test', 'secret' => 'filebeam-test-secret', 'region' => 'us-east-1', 'bucket' => 'filebeam', 'endpoint' => 'https://s3-proxy:9443', 'use_path_style_endpoint' => true, 'visibility' => 'private']]);
$path = 'services-diagnostic/'.bin2hex(random_bytes(8));
try {
    $disk = app(App\Support\FilestoreRegistry::class)->disk($store);
    $put = $disk->put($path, 'diagnostic');
    $read = $disk->get($path);
    $deleted = $disk->delete($path);
    $exists = $disk->exists($path);
    printf("S3 diagnostic: put=%s read=%s delete=%s exists-after-delete=%s\n", $put ? 'true' : 'false', $read === 'diagnostic' ? 'true' : 'false', $deleted ? 'true' : 'false', $exists ? 'true' : 'false');
    exit($put && $read === 'diagnostic' && $deleted && ! $exists ? 0 : 1);
} catch (Throwable $exception) {
    $message = str_replace(['filebeam-test-secret', 'filebeam-test'], '[REDACTED]', $exception->getMessage());
    fwrite(STDERR, 'S3 diagnostic: '.get_class($exception).': '.$message."\n");
    exit(1);
}
PHP
}

wait_installed_worker() {
    local name=$1 first_pid='' second_pid='' deadline=$((SECONDS + 60))
    while ((SECONDS < deadline)); do
        if docker exec "$name" sh -ec 'curl -fsS --max-time 5 http://127.0.0.1:2019/config/ | grep -Fq "/opt/filebeam/backend/public/frankenphp-worker.php"' 2>/dev/null; then
            first_pid=$(docker exec "$name" pgrep -o -f 'frankenphp.*run')
            sleep 3
            second_pid=$(docker exec "$name" pgrep -o -f 'frankenphp.*run')
            [[ $first_pid == "$second_pid" ]] && wait_healthy "$name" && return 0
        fi
        sleep 1
    done
    printf 'Installed FrankenPHP worker did not become active and stable\n' >&2
    return 1
}

service_scenario() {
    acceptance bootstrap || return
    installer_s3 || return
    wait_installed_worker "$app" || return
    docker exec --user 10001:10001 "$app" php -r 'require "/opt/filebeam/backend/vendor/autoload.php"; $app = require "/opt/filebeam/backend/bootstrap/app.php"; $app->make(Illuminate\Contracts\Console\Kernel::class)->bootstrap(); $pdo = Illuminate\Support\Facades\DB::connection()->getPdo(); exit($pdo->query("select inet_server_addr() is not null")->fetchColumn() ? 0 : 1);' || return
    docker exec --user 10001:10001 "$app" sh -ec '! mountpoint -q /storage; test -f /data/config/.env' || return
    assert_normal_logs "$app" || return

    docker stop "$postgres" >/dev/null || return
    wait_unhealthy "$app" || return
    docker exec "$app" curl -fsS --max-time 5 http://localhost:8080/up >/dev/null || return
    docker start "$postgres" >/dev/null || return
    wait_healthy "$app" || return
    docker stop -t 30 "$app" >/dev/null || return
    [[ $(docker inspect --format '{{.State.ExitCode}}' "$app") == 0 ]] || return
}

tls_scenario() {
    docker run -d --name "$tls_app" --label com.filebeam.test="$prefix" --read-only --tmpfs /run:rw,nosuid,nodev,size=64m --tmpfs /tmp:rw,nosuid,nodev,size=64m -e FILEBEAM_CONTAINER=true -e FILEBEAM_VARIANT=light -e FILEBEAM_TLS=proxy -e APP_URL=https://localhost:8443 -v "$tls_data_volume:/data" -v "$tls_storage_volume:/storage" "$image" >/dev/null || return
    wait_healthy "$tls_app" || return
    tls_acceptance bootstrap || return
    tls_acceptance complete || return
    wait_installed_worker "$tls_app" || return
    docker stop -t 30 "$tls_app" >/dev/null || return
    [[ $(docker inspect --format '{{.State.ExitCode}}' "$tls_app") == 0 ]] || return
    docker rm "$tls_app" >/dev/null || return
    docker run -d --name "$tls_app" --label com.filebeam.test="$prefix" --read-only --tmpfs /run:rw,nosuid,nodev,size=64m --tmpfs /tmp:rw,nosuid,nodev,size=64m -e FILEBEAM_CONTAINER=true -e FILEBEAM_VARIANT=light -e FILEBEAM_TLS=auto -e FILEBEAM_SERVER_NAME=localhost -v "$tls_data_volume:/data" -v "$tls_storage_volume:/storage" "$image" >/dev/null || return
    wait_for 'built-in TLS endpoint' "docker exec '$tls_app' curl -ksSf https://localhost:8443/up" || return
    wait_installed_worker "$tls_app" || return
    docker exec "$tls_app" sh -ec 'curl -ksSf https://localhost:8443/up >/dev/null; test "$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/up)" = 200; test "$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/)" = 308' || return
    certificate_before=$(docker exec "$tls_app" sh -ec 'sha256sum "$(find /data/caddy/certificates -name "*.crt" -print -quit)" | cut -d" " -f1') || return
    [[ -n $certificate_before ]] || return
    docker restart "$tls_app" >/dev/null || return
    wait_for 'restarted built-in TLS endpoint' "docker exec '$tls_app' curl -ksSf https://localhost:8443/up" || return
    certificate_after=$(docker exec "$tls_app" sh -ec 'sha256sum "$(find /data/caddy/certificates -name "*.crt" -print -quit)" | cut -d" " -f1') || return
    [[ $certificate_before == "$certificate_after" ]] || return
    assert_normal_logs "$tls_app" || return
    docker stop -t 30 "$tls_app" >/dev/null || return
    [[ $(docker inspect --format '{{.State.ExitCode}}' "$tls_app") == 0 ]] || return
}

docker network create --label com.filebeam.test="$prefix" "$network" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$data_volume" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$tls_data_volume" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$tls_storage_volume" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$proxy_config_volume" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$proxy_data_volume" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$ca_volume" >/dev/null
docker volume create --label com.filebeam.test="$prefix" "$ini_volume" >/dev/null

# Generate the proxy configuration and the PHP CA override inside owned volumes.
docker run --rm --entrypoint sh -v "$proxy_config_volume:/config" "$caddy_image" -ec 'printf "%s\n" "{" "    skip_install_trust" "}" "https://s3-proxy:9443 {" "    tls internal" "    reverse_proxy minio:9000" "}" > /config/Caddyfile'
docker run --rm --entrypoint sh -v "$ini_volume:/ini" "$image" -ec 'printf "%s\n" "curl.cainfo=/test-ca/root.crt" > /ini/zz-services-ca.ini'

docker run -d --name "$postgres" --label com.filebeam.test="$prefix" --network "$network" --network-alias postgres -e POSTGRES_DB=filebeam -e POSTGRES_USER=filebeam -e POSTGRES_PASSWORD=filebeam-test-password "$postgres_image" >/dev/null
docker run -d --name "$minio" --label com.filebeam.test="$prefix" --network "$network" --network-alias minio -e MINIO_ROOT_USER=filebeam-test -e MINIO_ROOT_PASSWORD=filebeam-test-secret "$minio_image" server /data >/dev/null
docker run -d --name "$proxy" --label com.filebeam.test="$prefix" --network "$network" --network-alias s3-proxy -v "$proxy_config_volume:/etc/caddy:ro" -v "$proxy_data_volume:/data" "$caddy_image" caddy run --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null
wait_for 'MinIO bucket creation' "docker run --rm --entrypoint /bin/sh --network '$network' '$mc_image' -ec 'mc alias set local http://minio:9000 filebeam-test filebeam-test-secret >/dev/null && mc mb --ignore-existing local/filebeam >/dev/null'"
wait_for 'S3 proxy CA' "docker run --rm -v '$proxy_data_volume:/data:ro' '$caddy_image' test -f /data/caddy/pki/authorities/local/root.crt"
docker run --rm --entrypoint sh -v "$proxy_data_volume:/source:ro" -v "$ca_volume:/ca" "$caddy_image" -ec 'cp /source/caddy/pki/authorities/local/root.crt /ca/root.crt && chmod 0644 /ca/root.crt'

# The image must remain healthy before setup even when no /storage mount exists.
docker run -d --name "$app" --label com.filebeam.test="$prefix" --network "$network" --read-only --tmpfs /run:rw,nosuid,nodev,size=64m --tmpfs /tmp:rw,nosuid,nodev,size=64m \
    -e FILEBEAM_CONTAINER=true -e FILEBEAM_VARIANT=light -e FILEBEAM_TLS=proxy -e DB_CONNECTION=pgsql -e DB_HOST=postgres -e DB_PORT=5432 -e DB_DATABASE=filebeam -e DB_USERNAME=filebeam -e DB_PASSWORD=filebeam-test-password -e CACHE_STORE=file -e PHP_INI_SCAN_DIR=:/test-ini \
    -v "$data_volume:/data" -v "$ca_volume:/test-ca:ro" -v "$ini_volume:/test-ini:ro" "$image" >/dev/null
wait_healthy "$app"
service_failure=0 tls_failure=0
if ! service_scenario; then
    service_failure=1
    diagnose_s3 || true
else
    printf '%s\n' 'services: passed' >&2
fi
if ! tls_scenario; then
    tls_failure=1
else
    printf '%s\n' 'tls: passed' >&2
fi
((service_failure == 0 && tls_failure == 0))
