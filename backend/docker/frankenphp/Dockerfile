FROM node:24-bookworm-slim AS node
FROM composer:2 AS composer
FROM rust:1.98.0-slim-bookworm AS rust

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-pack --version 0.15.0 --locked

FROM dunglas/frankenphp:1-php8.5-bookworm

ARG WWWUSER=1000
ARG WWWGROUP=1000

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git gosu supervisor unzip build-essential pkg-config libssl-dev \
    && install-php-extensions pdo_pgsql pdo_sqlite redis pcntl bcmath intl zip \
    && rm -rf /var/lib/apt/lists/*

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
COPY --from=composer /usr/bin/composer /usr/local/bin/composer
COPY --from=rust /usr/local/cargo /usr/local/cargo
COPY --from=rust /usr/local/rustup /usr/local/rustup

ENV CARGO_HOME=/home/sail/.cargo \
    FILEBEAM_RUST_IN_CONTAINER=1 \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH

RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
    && groupadd --gid "${WWWGROUP}" sail \
    && useradd --uid "${WWWUSER}" --gid sail --create-home --shell /bin/bash sail \
    && mkdir -p /home/sail/.cargo /config/caddy /data/caddy \
    && chown -R sail:sail /home/sail /config/caddy /data/caddy

COPY Caddyfile /etc/caddy/Caddyfile
COPY php.ini /usr/local/etc/php/conf.d/99-filebeam.ini
COPY supervisord.conf /etc/supervisor/conf.d/filebeam.conf
COPY --chmod=755 start-container /usr/local/bin/start-container

WORKDIR /var/www/html/backend
EXPOSE 8080 5173

ENTRYPOINT ["start-container"]
CMD ["/usr/bin/supervisord", "-n", "-c", "/etc/supervisor/conf.d/filebeam.conf"]
