#!/usr/bin/env bash
set -euo pipefail
root=$(git rev-parse --show-toplevel)
mkdir -p "$root/.filebeam"
state=$(mktemp -d "$root/.filebeam/theme-tests.XXXXXX")
server=''
cleanup() {
    if [[ -n "$server" ]]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; fi
}
trap cleanup EXIT
mkdir -p "$state/config" "$state/cache" "$state/app/framework/views"
touch "$state/config/.env" "$state/database.sqlite"
printf 'post_max_size=64M\nupload_max_filesize=64M\n' > "$state/server.ini"
export PHP_INI_SCAN_DIR="${PHP_INI_SCAN_DIR:-}:$state"
export APP_ENV=local APP_DEBUG=false APP_URL="http://127.0.0.1:${THEME_PORT:-8033}"
export APP_NAME="filebeam-theme-$(php -r 'echo bin2hex(random_bytes(12));')"
export THEME_TEST_INSTANCE="$APP_NAME"
export FILEBEAM_NAME=Filebeam FILEBEAM_LOGO_URL='' FILEBEAM_FAVICON_URL='' FILEBEAM_OG_IMAGE_URL=''
export FILEBEAM_COPYRIGHT_YEAR=2026
export APP_KEY="base64:$(php -r 'echo base64_encode(random_bytes(32));')"
export FILEBEAM_CONTAINER=true FILEBEAM_VARIANT=light FILEBEAM_DATA_DIR="$state"
export DB_CONNECTION=sqlite DB_DATABASE="$state/database.sqlite" DB_URL=''
export CACHE_STORE=file SESSION_DRIVER=cookie QUEUE_CONNECTION=sync MAIL_MAILER=log
export APP_CONFIG_CACHE="$state/cache/config.php" APP_ROUTES_CACHE="$state/cache/routes.php"
export APP_EVENTS_CACHE="$state/cache/events.php" APP_SERVICES_CACHE="$state/cache/services.php"
export APP_PACKAGES_CACHE="$state/cache/packages.php" VIEW_COMPILED_PATH="$state/app/framework/views"
export FILEBEAM_PRIMARY_COLOR='' FILEBEAM_CREATIONS_PER_HOUR=600
export FILEBEAM_FILESTORE_ROOT="$state/filestore" FILEBEAM_FILESYSTEMS=transfers
if ! php "$root/backend/artisan" migrate --seed --force --no-interaction > "$state/setup.log" 2>&1; then
    printf 'Theme fixture setup failed: %s\n' "$state/setup.log" >&2
    exit 1
fi
php "$root/backend/artisan" serve --host=127.0.0.1 --port="${THEME_PORT:-8033}" --tries=1 --no-reload > "$state/server.log" 2>&1 &
server=$!
for _ in {1..40}; do
    kill -0 "$server"
    if curl --fail --silent --max-time 1 "$APP_URL/up" >/dev/null; then break; fi
    sleep .25
done
curl --fail --silent --max-time 5 "$APP_URL/" | php -r '$html = stream_get_contents(STDIN); if (!str_contains($html, "\"name\":\"".getenv("THEME_TEST_INSTANCE")."\"")) { fwrite(STDERR, "Refusing to test an application not owned by this fixture.\n"); exit(1); }'
THEME_BASE_URL="$APP_URL" npx playwright test --config "$root/playwright.theme.config.ts" "$@"
