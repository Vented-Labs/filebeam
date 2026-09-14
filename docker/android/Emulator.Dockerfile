FROM filebeam-android-tooling:rust-1.98.0-sdk37
ARG SYSTEM_IMAGE="system-images;android-35;google_apis_ps16k;x86_64"
ENV FILEBEAM_TEST_SYSTEM_IMAGE="${SYSTEM_IMAGE}"
RUN apt-get update && apt-get install --yes --no-install-recommends \
        libasound2 libdbus-1-3 libnss3 libpulse0 libx11-6 libxcb1 libxcomposite1 \
        libxcursor1 libxi6 libxrandr2 libxtst6 \
    && rm -rf /var/lib/apt/lists/* \
    && sdkmanager emulator "$SYSTEM_IMAGE" \
    && chmod -R a+rX "$ANDROID_HOME"
ENV PATH="${ANDROID_HOME}/emulator:${PATH}"
