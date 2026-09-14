# syntax=docker/dockerfile:1@sha256:ecfaec9ed6d810b56388c508f4121597bfbba70d41a6dfeee4d8cad5f295fc32
# All service images are the selected, multi-architecture upstream releases.
FROM dunglas/frankenphp:1.12.7-php8.5-trixie@sha256:f92d81eb3fe4fd18b35d3d58192b7cc3acc8943817bbf39f2fcf0be02a3916dc AS php-base

COPY docker/production/apt.conf /etc/apt/apt.conf.d/80-filebeam-network
RUN sed -i 's|http://deb.debian.org|https://deb.debian.org|g' /etc/apt/sources.list.d/debian.sources \
    && install-php-extensions pdo_sqlite pdo_pgsql pdo_mysql redis-6.3.0 sodium pcntl posix intl bcmath zip \
    && rm -rf /var/lib/apt/lists/*

FROM composer:2@sha256:d8f6343d3fae98107426bc49163ccad46ef85aabd4a27d80a74401fab4aba332 AS composer

FROM php-base AS application
COPY --from=composer /usr/bin/composer /usr/local/bin/composer
WORKDIR /opt/filebeam
COPY backend/composer.json backend/composer.lock backend/
COPY updater updater
RUN mkdir -p backend/bootstrap/cache backend/storage/app/public backend/storage/framework/cache/data backend/storage/framework/sessions backend/storage/framework/views backend/storage/logs
RUN --mount=type=cache,target=/root/.cache/composer \
    composer --working-dir=backend install --no-dev --no-interaction --prefer-dist --optimize-autoloader --no-scripts
COPY backend/app backend/app
COPY backend/bootstrap backend/bootstrap
COPY backend/config backend/config
COPY backend/database/migrations backend/database/migrations
COPY backend/database/seeders backend/database/seeders
COPY backend/database/factories backend/database/factories
COPY backend/public backend/public
COPY backend/resources backend/resources
COPY backend/routes backend/routes
COPY backend/artisan backend/package.json backend/vite.config.ts backend/tsconfig.json backend/
RUN composer --working-dir=backend dump-autoload --no-dev --optimize --no-scripts \
    && php backend/artisan package:discover --ansi \
    && php backend/artisan filament:upgrade --no-interaction

FROM rust:1.98.0-trixie@sha256:620dbcd124499c59e2406d3741574b5c5838cf9eb9656f0c3a03948f79b02959 AS wasm
ARG CARGO_BUILD_JOBS=1
RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --version 0.15.0 --locked
WORKDIR /src
COPY encryption encryption
COPY transfer transfer
COPY transfer-wasm transfer-wasm
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/encryption/target \
    --mount=type=cache,target=/src/transfer/target \
    --mount=type=cache,target=/src/transfer-wasm/target \
    wasm-pack build encryption --target web --release --no-opt \
    && wasm-pack build transfer-wasm --target web --release --no-opt

FROM node:24-trixie@sha256:f7d34e58713740f9eef9092c0bd6ff10369d132f7238399a4b270f16d47fa608 AS node

FROM php-base AS frontend-dependencies
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx
WORKDIR /opt/filebeam
COPY package.json package-lock.json ./
COPY backend/package.json backend/package.json
COPY ui/package.json ui/package.json
RUN --mount=type=cache,target=/root/.npm \
    npm ci \
    && npx playwright install --with-deps chromium

FROM frontend-dependencies AS frontend
COPY --from=application /opt/filebeam /opt/filebeam
COPY ui ui
COPY icons icons
COPY scripts/check-icons.mjs scripts/generate-icons.mjs scripts/
COPY scripts/og scripts/og
COPY scripts/prism-gallery scripts/prism-gallery
COPY vite.config.ts ./
COPY --from=wasm /src/encryption/pkg /opt/filebeam/encryption/pkg
COPY --from=wasm /src/transfer-wasm/pkg /opt/filebeam/transfer-wasm/pkg
RUN npm run check:icons \
    && npm run build --workspace=backend

FROM postgres:18.6-trixie@sha256:4ef4dbc939d61acea57712655ddb4b4ab27419c913f94cca0cd57cb3ea3c2280 AS postgres
FROM valkey/valkey:9.1.2-trixie@sha256:c123e3715db63d06d4ad6964884037aa0d5d4d703939b9929954112889708e1d AS valkey

# Keep PostgreSQL's large LLVM and bitcode payload out of the copied runtime layer.
FROM postgres AS postgres-runtime
RUN rm -rf /usr/lib/postgresql/18/lib/bitcode /usr/lib/postgresql/18/lib/llvmjit* \
    && libpq_path="$(dpkg-query -L libpq5 | awk '/\/libpq\.so\.5$/ { print; exit }')" \
    && libpq_dir="$(dirname "$libpq_path")" \
    && mkdir -p "/runtime-libs${libpq_dir}" \
    && cp -a "${libpq_path}"* "/runtime-libs${libpq_dir}/"

FROM php-base AS runtime
ARG FILEBEAM_VARIANT=light
ENV FILEBEAM_CONTAINER=true \
    FILEBEAM_VARIANT=${FILEBEAM_VARIANT} \
    LOG_CHANNEL=stderr \
    APP_ENV=production \
    APP_DEBUG=false \
    FILEBEAM_DATA_DIR=/data \
    FILEBEAM_FILESTORE_ROOT=/storage \
    APP_CONFIG_CACHE=/run/filebeam/cache/config.php \
    APP_EVENTS_CACHE=/run/filebeam/cache/events.php \
    APP_PACKAGES_CACHE=/run/filebeam/cache/packages.php \
    APP_SERVICES_CACHE=/run/filebeam/cache/services.php \
    APP_ROUTES_CACHE=/run/filebeam/cache/routes-v7.php \
    VIEW_COMPILED_PATH=/run/filebeam/views \
    XDG_CONFIG_HOME=/data/caddy \
    XDG_DATA_HOME=/data/caddy

COPY docker/production/php.ini /usr/local/etc/php/conf.d/99-filebeam-production.ini
COPY docker/production/Caddyfile /etc/filebeam/Caddyfile
COPY docker/production/filebeam-init /usr/local/bin/filebeam-init
COPY docker/production/filebeam-supervisor /usr/local/bin/filebeam-supervisor
COPY docker/production/filebeam-healthcheck /usr/local/bin/filebeam-healthcheck

RUN if [ -e /usr/local/lib/libwatcher-c.so.0.14.5 ]; then ln -sf libwatcher-c.so.0.14.5 /usr/local/lib/libwatcher-c.so.0; fi

RUN export DEBIAN_FRONTEND=noninteractive \
    && apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends curl procps tini util-linux \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 filebeam \
    && useradd --uid 10001 --gid filebeam --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin filebeam \
    && groupadd --gid 10002 filebeampg \
    && useradd --uid 10002 --gid filebeampg --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin filebeampg \
    && usermod -aG filebeampg filebeam \
    && groupadd --gid 10003 filebeamvalkey \
    && useradd --uid 10003 --gid filebeamvalkey --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin filebeamvalkey \
    && usermod -aG filebeamvalkey filebeam \
    && mkdir -p /data/app /data/config /data/database /data/caddy /data/postgresql/18/docker /data/valkey /storage /run/filebeam/cache /run/filebeam/views /run/filebeam/postgresql /run/filebeam/valkey /tmp \
    && chmod 1777 /tmp \
    && chmod 755 /usr/local/bin/filebeam-init /usr/local/bin/filebeam-supervisor /usr/local/bin/filebeam-healthcheck \
    && (setcap -r /usr/local/bin/frankenphp 2>/dev/null || true) \
    && build_packages="$(dpkg-query -W -f='${binary:Package}\t${db:Status-Status}\n' 2>/dev/null | awk '$2 == "installed" && $1 !~ /^gcc-[0-9]+-base(:|$)/ && ($1 ~ /^(gcc|g\+\+|cpp|binutils)(-|:|$)/ || $1 ~ /^(autoconf|automake|dpkg-dev|libtool|make|patch|pkg-config|re2c)(:|$)/ || $1 ~ /-dev(:|$)/) { print $1 }')" \
    && if [ -n "$build_packages" ]; then apt-get purge -y $build_packages; fi \
    && rm -rf /usr/include /usr/local/include /usr/src /tmp/pear /var/cache/apt/* \
    && rm -f /usr/lib/*.a /usr/lib/*/*.a /usr/lib/*/*/*.a /usr/local/lib/*.a

COPY --from=application /opt/filebeam /opt/filebeam
COPY --from=frontend /opt/filebeam/backend/public/build /opt/filebeam/backend/public/build
COPY icons/NOTICE /opt/filebeam/ICONSAX-NOTICE
COPY LICENSE SECURITY.md /opt/filebeam/
RUN rm -rf /opt/filebeam/backend/storage /opt/filebeam/backend/bootstrap/cache \
    && ln -s /data/app /opt/filebeam/backend/storage \
    && ln -s /run/filebeam/cache /opt/filebeam/backend/bootstrap/cache

ARG VERSION=0.0.0-local
ARG COMMIT=local-source
ARG BUILD_METADATA=local
LABEL org.opencontainers.image.title="Filebeam" \
    org.opencontainers.image.description="End-to-end encrypted file sharing" \
    org.opencontainers.image.source="https://github.com/Vented-Labs/filebeam" \
    org.opencontainers.image.licenses="MIT" \
    org.opencontainers.image.version="${VERSION}" \
    org.opencontainers.image.revision="${COMMIT}" \
    io.filebeam.build="${BUILD_METADATA}" \
    io.filebeam.postgresql.client-source="docker.io/library/postgres:18.6-trixie@sha256:4ef4dbc939d61acea57712655ddb4b4ab27419c913f94cca0cd57cb3ea3c2280"
RUN VERSION="$VERSION" COMMIT="$COMMIT" BUILD_METADATA="$BUILD_METADATA" php -r 'file_put_contents("/opt/filebeam/backend/config/version.php", "<?php\ndeclare(strict_types=1);\nreturn ".var_export(["version" => getenv("VERSION"), "tag" => getenv("VERSION"), "commit" => getenv("COMMIT"), "distribution" => "container", "build" => getenv("BUILD_METADATA"), "update_public_key" => ""], true).";\n");'

WORKDIR /opt/filebeam/backend
EXPOSE 8080 8443
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=10s --timeout=8s --start-period=60s --retries=3 CMD ["/usr/local/bin/filebeam-healthcheck"]
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/filebeam-init"]
CMD ["/usr/local/bin/filebeam-supervisor"]

FROM runtime AS light
ENV FILEBEAM_VARIANT=light

FROM runtime AS omnibus
# PostgreSQL's Debian payload and Valkey binaries are copied from the pinned
# upstream images. No upstream account or /etc tree is imported.
COPY --from=postgres-runtime /usr/lib/postgresql /usr/lib/postgresql
COPY --from=postgres-runtime /usr/share/postgresql /usr/share/postgresql
# PostgreSQL 18 clients need the matching libpq release in Debian's native path.
COPY --from=postgres-runtime /runtime-libs/usr/lib/ /usr/lib/
COPY --from=valkey /usr/local/bin/valkey-* /usr/local/bin/
ENV FILEBEAM_VARIANT=omnibus \
    PATH=/usr/lib/postgresql/18/bin:$PATH
RUN export DEBIAN_FRONTEND=noninteractive \
    && apt-get update \
    && apt-get install -y --no-install-recommends libldap2 liblz4-1 libzstd1 libxml2 libxslt1.1 libpam0g libreadline8t64 libssl3t64 libicu76 libcurl4t64 liburing2 libedit2 libuuid1 libnuma1 libsystemd0 \
    && rm -rf /var/lib/apt/lists/* \
    && for tool in psql pg_isready pg_dump pg_restore createdb createuser; do ln -s "/usr/lib/postgresql/18/bin/$tool" "/usr/bin/$tool"; done \
    && expected_libpq="$(dirname "$(readlink -f "$(ldconfig -p | awk '$1 == "libc.so.6" { print $NF; exit }')")")/libpq.so.5" \
    && expected_libpq="$(readlink -f "$expected_libpq")" \
    && for binary in /usr/lib/postgresql/18/bin/postgres /usr/lib/postgresql/18/bin/psql /usr/lib/postgresql/18/bin/pg_isready /usr/lib/postgresql/18/bin/pg_dump /usr/lib/postgresql/18/bin/pg_restore /usr/local/bin/valkey-server; do dependencies="$(ldd "$binary")"; printf '%s\n' "$dependencies"; ! printf '%s\n' "$dependencies" | grep -F 'not found'; done \
    && for tool in psql pg_isready pg_dump pg_restore; do resolved_libpq="$(ldd "$(command -v "$tool")" | awk '$1 == "libpq.so.5" { print $3; exit }')"; test "$(readlink -f "$resolved_libpq")" = "$expected_libpq"; grep -aq 'PQfullProtocolVersion' "$resolved_libpq"; done \
    && postgres --version && psql --version && pg_isready --version && pg_dump --version && pg_restore --version && valkey-server --version

# --version and ldd do not exercise libpq symbols first used on connection.
RUN <<'SH'
set -eu
directory=$(mktemp -d /tmp/filebeam-pg-check.XXXXXX)
postgres_pid=''
cleanup() {
    status=$?
    trap - EXIT
    if [ -n "$postgres_pid" ]; then
        kill -INT "$postgres_pid" 2>/dev/null || true
        wait "$postgres_pid" || true
    fi
    if [ "$status" -ne 0 ] && [ -f "$directory/server.log" ]; then cat "$directory/server.log" >&2; fi
    rm -rf "$directory"
    exit "$status"
}
trap cleanup EXIT
chown filebeampg:filebeampg "$directory"
setpriv --reuid=filebeampg --regid=filebeampg --init-groups -- initdb -D "$directory/data" --encoding=UTF8 --locale=C.UTF-8 --auth-local=peer --auth-host=reject --username=filebeampg >/dev/null
setpriv --reuid=filebeampg --regid=filebeampg --init-groups -- postgres -D "$directory/data" -k "$directory" -c listen_addresses='' >"$directory/server.log" 2>&1 &
postgres_pid=$!
attempts=0
until setpriv --reuid=filebeampg --regid=filebeampg --init-groups -- pg_isready -h "$directory" -U filebeampg -d postgres -t 2 -q; do
    attempts=$((attempts + 1))
    kill -0 "$postgres_pid"
    [ "$attempts" -lt 30 ]
    sleep 1
done
setpriv --reuid=filebeampg --regid=filebeampg --init-groups -- psql -X -w -h "$directory" -U filebeampg -d postgres -v ON_ERROR_STOP=1 -Atqc 'SELECT 1'
SH
