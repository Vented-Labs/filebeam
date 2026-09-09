FROM rust:1.98.0-bookworm

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        ca-certificates \
        gcc-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        php-cli \
        tar \
    && rustup component add clippy rustfmt \
    && rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_BUILD_JOBS=1 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

WORKDIR /workspace
