#!/usr/bin/env bash

# Verify the root production build context without building the production image.
# Dockerfile-specific ignore files take precedence over a root .dockerignore;
# production must not add docker/production/Dockerfile.dockerignore unless this
# checker is updated to validate that policy instead.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp_parent="${TMPDIR:-/tmp}"

if [[ ! -d "$tmp_parent" ]]; then
    tmp_parent=/tmp
    unset TMPDIR
fi

if [[ ! -d "$tmp_parent" ]]; then
    printf 'Temporary directory parent does not exist: %s\n' "$tmp_parent" >&2
    exit 1
fi

if ! docker info >/dev/null 2>&1; then
    printf 'Docker daemon is required to check the production build context.\n' >&2
    exit 1
fi

if [[ -e "$root/docker/production/Dockerfile.dockerignore" ]]; then
    printf '%s\n' 'docker/production/Dockerfile.dockerignore overrides the root policy; validate it explicitly before using it.' >&2
    exit 1
fi

if grep -Eq '^[[:space:]]*COPY([[:space:]]+--[^[:space:]]+)*[[:space:]]+backend[[:space:]]+backend([[:space:]]|$)' "$root/docker/production/Dockerfile"; then
    printf '%s\n' 'docker/production/Dockerfile must use explicit backend COPY inputs; COPY backend backend makes BuildKit open ignored root-owned storage.' >&2
    exit 1
fi

tmp="$(mktemp -d "$tmp_parent/filebeam-context.XXXXXX")"
actual_output="$tmp/actual-output"
sentinel_context="$tmp/sentinel-context"
sentinel_output="$tmp/sentinel-output"
container=""

cleanup() {
    [[ -z "$container" ]] || docker rm -f "$container" >/dev/null 2>&1 || true
    rm -rf "$tmp"
}
trap cleanup EXIT

mkdir -p "$actual_output" "$sentinel_context" "$sentinel_output"
cat > "$tmp/Dockerfile.context-check" <<'EOF'
# syntax=docker/dockerfile:1
FROM alpine:3.22 AS copied-context
COPY package.json package-lock.json vite.config.ts LICENSE SECURITY.md /context/
COPY docker/production /context/docker/production
COPY icons /context/icons
COPY scripts/check-icons.mjs scripts/generate-icons.mjs /context/scripts/
COPY scripts/og /context/scripts/og
COPY scripts/prism-gallery /context/scripts/prism-gallery
COPY backend/artisan backend/composer.json backend/composer.lock backend/package.json backend/vite.config.ts backend/tsconfig.json /context/backend/
COPY backend/app /context/backend/app
COPY backend/bootstrap /context/backend/bootstrap
COPY backend/config /context/backend/config
COPY backend/database/migrations /context/backend/database/migrations
COPY backend/database/seeders /context/backend/database/seeders
COPY backend/resources /context/backend/resources
COPY backend/routes /context/backend/routes
COPY backend/public/index.php backend/public/frankenphp-worker.php backend/public/robots.txt backend/public/.htaccess backend/public/favicon.ico backend/public/apple-touch-icon.png /context/backend/public/
COPY backend/public/brand /context/backend/public/brand
COPY backend/public/fonts /context/backend/public/fonts
COPY ui/package.json /context/ui/package.json
COPY ui/src /context/ui/src
COPY encryption /context/encryption
COPY update.php /context/update.php
COPY updater /context/updater
RUN cd /context && find . -mindepth 1 -print | cut -c3- | LC_ALL=C sort > /manifest
FROM scratch
COPY --from=copied-context /manifest /manifest
EOF

# Pass the temporary Dockerfile on stdin so no Dockerfile-specific ignore file
# can override the root policy used for the actual repository context.
docker buildx build --no-cache --progress=plain --file - \
    --output "type=local,dest=$actual_output" "$root" < "$tmp/Dockerfile.context-check" >/dev/null

manifest="$actual_output/manifest"
if [[ ! -f "$manifest" ]]; then
    printf 'BuildKit did not export a context manifest.\n' >&2
    exit 1
fi

# Sentinels are created only in a minimal temporary context. The real context
# above is read directly by BuildKit, avoiding host storage traversal or copies.
cp "$root/.dockerignore" "$sentinel_context/.dockerignore"
mkdir -p "$sentinel_context/backend/app" "$sentinel_context/backend/database" \
    "$sentinel_context/backend/public/build" "$sentinel_context/backend/public/js/filament" \
    "$sentinel_context/backend/resources/js/actions" "$sentinel_context/backend/resources/js/routes" \
    "$sentinel_context/backend/resources/js/wayfinder" "$sentinel_context/backend/storage/logs" \
    "$sentinel_context/encryption/pkg" "$sentinel_context/encryption/target" \
    "$sentinel_context/node_modules" "$sentinel_context/.ai" "$sentinel_context/dist" \
    "$sentinel_context/tests" "$sentinel_context/mobile"
touch "$sentinel_context/backend/app/.context-allowed.php" \
    "$sentinel_context/backend/.env.context-sentinel" \
    "$sentinel_context/backend/database/context-sentinel.sqlite" \
    "$sentinel_context/backend/public/build/context-sentinel.js" \
    "$sentinel_context/backend/public/js/filament/context-sentinel.js" \
    "$sentinel_context/backend/resources/js/actions/context-sentinel.ts" \
    "$sentinel_context/backend/resources/js/routes/context-sentinel.ts" \
    "$sentinel_context/backend/resources/js/wayfinder/context-sentinel.ts" \
    "$sentinel_context/backend/storage/logs/context-sentinel.log" \
    "$sentinel_context/encryption/pkg/context-sentinel.wasm" \
    "$sentinel_context/encryption/target/context-sentinel" \
    "$sentinel_context/node_modules/context-sentinel.js" \
    "$sentinel_context/.ai/context-sentinel" "$sentinel_context/dist/context-sentinel" \
    "$sentinel_context/tests/context-sentinel" "$sentinel_context/mobile/context-sentinel"

cat > "$tmp/Dockerfile.sentinel-check" <<'EOF'
# syntax=docker/dockerfile:1
FROM alpine:3.22 AS copied-context
COPY . /context/
RUN cd /context && find . -mindepth 1 -print | cut -c3- | LC_ALL=C sort > /manifest
FROM scratch
COPY --from=copied-context /manifest /manifest
EOF

docker buildx build --no-cache --progress=plain --file - \
    --output "type=local,dest=$sentinel_output" "$sentinel_context" < "$tmp/Dockerfile.sentinel-check" >/dev/null
sentinel_manifest="$sentinel_output/manifest"

require_present() {
    if ! grep -Fqx "$1" "$manifest"; then
        printf 'Expected production context input is missing: %s\n' "$1" >&2
        exit 1
    fi
}

require_prefix() {
    if ! grep -Fq "${1}/" "$manifest"; then
        printf 'Expected production context input prefix is missing: %s/\n' "$1" >&2
        exit 1
    fi
}

require_absent() {
    if grep -Fqx "$1" "$sentinel_manifest"; then
        printf 'Excluded context input leaked into BuildKit: %s\n' "$1" >&2
        exit 1
    fi
}

require_actual_absent() {
    if grep -Fqx "$1" "$manifest"; then
        printf 'Excluded repository input leaked into BuildKit: %s\n' "$1" >&2
        exit 1
    fi
}

require_actual_prefix_absent() {
    if grep -Fq "${1}/" "$manifest"; then
        printf 'Excluded repository input prefix leaked into BuildKit: %s/\n' "$1" >&2
        exit 1
    fi
}

require_sentinel_present() {
    if ! grep -Fqx "$1" "$sentinel_manifest"; then
        printf 'Allowed sentinel is missing from BuildKit: %s\n' "$1" >&2
        exit 1
    fi
}

require_present 'package.json'
require_present 'package-lock.json'
require_present 'LICENSE'
require_present 'SECURITY.md'
require_present 'icons/NOTICE'
require_present 'icons/approved.json'
require_present 'icons/dependency-inventory.json'
require_present 'scripts/check-icons.mjs'
require_present 'scripts/generate-icons.mjs'
require_prefix 'scripts/og'
require_present 'scripts/prism-gallery/check-icons.mjs'
require_present 'scripts/prism-gallery/Gallery.vue'
require_present 'backend/artisan'
require_present 'backend/composer.json'
require_present 'backend/composer.lock'
require_present 'backend/package.json'
require_present 'backend/vite.config.ts'
require_present 'backend/tsconfig.json'
require_present 'encryption/Cargo.toml'
require_present 'update.php'
require_present 'updater/ActivityLock.php'
require_present 'updater/PostgresBackup.php'
require_present 'updater/Updater.php'
require_present 'backend/public/index.php'
require_present 'backend/public/.htaccess'
require_present 'backend/public/frankenphp-worker.php'
if [[ -f "$root/encryption/Cargo.lock" ]]; then
    require_present 'encryption/Cargo.lock'
fi
require_prefix 'backend/app'
require_prefix 'backend/bootstrap'
require_prefix 'backend/config'
require_prefix 'backend/database/migrations'
require_prefix 'backend/database/seeders'
require_prefix 'backend/resources'
require_prefix 'backend/routes'
require_prefix 'backend/public/brand'
require_prefix 'backend/public/fonts'
require_prefix 'ui/src'
require_prefix 'encryption/src'
require_prefix 'updater'
require_sentinel_present 'backend/app/.context-allowed.php'

require_actual_absent 'backend/database/database.sqlite'
require_actual_absent 'backend/public/build/manifest.json'
require_actual_prefix_absent 'backend/bootstrap/cache'
require_actual_prefix_absent 'backend/resources/js/actions'
require_actual_prefix_absent 'backend/resources/js/routes'
require_actual_prefix_absent 'backend/resources/js/wayfinder'

for excluded in \
    'backend/.env.context-sentinel' \
    'backend/database/context-sentinel.sqlite' \
    'backend/public/build/context-sentinel.js' \
    'backend/public/js/filament/context-sentinel.js' \
    'backend/resources/js/actions/context-sentinel.ts' \
    'backend/resources/js/routes/context-sentinel.ts' \
    'backend/resources/js/wayfinder/context-sentinel.ts' \
    'backend/storage/logs/context-sentinel.log' \
    'encryption/pkg/context-sentinel.wasm' \
    'encryption/target/context-sentinel' \
    'node_modules/context-sentinel.js' \
    '.ai/context-sentinel' \
    'dist/context-sentinel' \
    'tests/context-sentinel' \
    'mobile/context-sentinel'; do
    require_absent "$excluded"
done

if [[ $# -ne 0 && ( $# -ne 2 || "$1" != '--image' ) ]]; then
    printf 'Usage: %s [--image IMAGE]\n' "$0" >&2
    exit 64
fi

if [[ $# -eq 2 ]]; then
    container="$(docker create "$2")"
    docker export "$container" > "$tmp/image.tar"
    tar -tf "$tmp/image.tar" > "$tmp/image-manifest"
    license_present=false
    security_present=false

    while IFS= read -r path; do
        path="${path#./}"
        [[ "$path" == opt/filebeam/* ]] || continue
        if [[ "$path" == opt/filebeam/scripts/prism-gallery/* ]]; then
            printf 'Build-only gallery source leaked into the runtime image: /%s\n' "$path" >&2
            exit 1
        fi
        [[ "$path" != 'opt/filebeam/LICENSE' ]] || license_present=true
        [[ "$path" != 'opt/filebeam/SECURITY.md' ]] || security_present=true
        name="${path##*/}"
        case "$name" in
            .env|.env.*)
                # A documented template is not a runtime credential file.
                [[ "$name" == '.env.example' ]] || {
                    printf 'Sensitive environment file leaked into application: /%s\n' "$path" >&2
                    exit 1
                }
                ;;
            *.key|*.pem|*.p12|*.pfx|*.sqlite|*.sqlite-*|*.sql|*.dump|*.bak)
                printf 'Sensitive database, backup, or key file leaked into application: /%s\n' "$path" >&2
                exit 1
                ;;
        esac
    done < "$tmp/image-manifest"

    if [[ "$license_present" != true || "$security_present" != true ]]; then
        printf '%s\n' 'Application image is missing /opt/filebeam/LICENSE or /opt/filebeam/SECURITY.md.' >&2
        exit 1
    fi

    printf 'Image filesystem sensitive-file policy passed: %s\n' "$2"
fi

printf 'Production Docker build context policy passed.\n'
