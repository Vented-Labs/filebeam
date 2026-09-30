FROM rust:1.98.0-bookworm

RUN rustup component add rustfmt clippy

# Native GPUI builds need both X11/Wayland headers and the Vulkan/Mesa stack.
# No project source or Cargo work runs while this image is built.
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential \
        at-spi2-core \
        ca-certificates \
        clang \
        cmake \
        fontconfig \
        git \
        libdbus-1-dev \
        libegl1-mesa-dev \
        libfontconfig1-dev \
        libfreetype6-dev \
        libgl1-mesa-dev \
        libharfbuzz-dev \
        libnss-wrapper \
        libpango1.0-dev \
        libvulkan-dev \
        libwayland-dev \
        libx11-dev \
        libx11-xcb-dev \
        libxcb-render0-dev \
        libxcb-shape0-dev \
        libxcb-xfixes0-dev \
        libxcb1-dev \
        libxkbcommon-dev \
        libxkbcommon-x11-dev \
        imagemagick \
        mesa-utils \
        mesa-vulkan-drivers \
        openbox \
        php-cli \
        php8.2-common \
        picom \
        python3-minimal \
        python3-pyatspi \
        pkg-config \
        vulkan-tools \
        x11-apps \
        xauth \
        xdotool \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

WORKDIR /workspace
