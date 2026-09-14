FROM eclipse-temurin:21-jdk-jammy AS java
FROM rust:1.98.0-bookworm

COPY --from=java /opt/java/openjdk /opt/java/openjdk
ENV JAVA_HOME=/opt/java/openjdk \
    ANDROID_HOME=/opt/android-sdk \
    ANDROID_NDK_HOME=/opt/android-sdk/ndk/28.2.13676358 \
    FILEBEAM_ANDROID_IN_CONTAINER=1 \
    FILEBEAM_RUST_IN_CONTAINER=1
ENV PATH="${JAVA_HOME}/bin:${ANDROID_HOME}/cmdline-tools/latest/bin:${ANDROID_HOME}/platform-tools:/opt/gradle/gradle-9.7.1/bin:${PATH}"

RUN apt-get update && apt-get install --yes --no-install-recommends ca-certificates curl unzip python3 \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add clippy rustfmt \
    && rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android \
    && cargo install cargo-ndk --version 4.1.2 --locked

RUN curl --fail --location --retry 3 https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip --output /tmp/android-tools.zip \
    && printf '%s  %s\n' e025545c62a8e64c7559119566a569fb1dec5f60 /tmp/android-tools.zip | sha1sum --check \
    && mkdir -p "$ANDROID_HOME/cmdline-tools" \
    && unzip -q /tmp/android-tools.zip -d "$ANDROID_HOME/cmdline-tools" \
    && mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest" \
    && rm /tmp/android-tools.zip \
    && (yes | sdkmanager --licenses > /dev/null) \
    && sdkmanager 'platform-tools' 'platforms;android-37.0' 'build-tools;36.0.0' 'ndk;28.2.13676358'

RUN curl --fail --location --retry 3 https://services.gradle.org/distributions/gradle-9.7.1-bin.zip --output /tmp/gradle.zip \
    && printf '%s  %s\n' acd53f1edaf02f1a8ff99879f8a34b302661a057d9b063ae9e35b552f804d20a /tmp/gradle.zip | sha256sum --check \
    && unzip -q /tmp/gradle.zip -d /opt/gradle \
    && rm /tmp/gradle.zip

# SDK archives retain owner-only execute bits; local/CI builds run as the caller.
RUN chmod -R a+rX "$ANDROID_HOME"

WORKDIR /workspace/mobile/android
