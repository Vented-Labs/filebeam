#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
if [[ ${FILEBEAM_ANDROID_IN_CONTAINER:-} != 1 ]]; then
    exec bash "$root/scripts/android/run.sh" bash /workspace/scripts/android/check.sh "$@"
fi

for crate in client-core client-ffi; do
    cargo fmt --manifest-path "$root/$crate/Cargo.toml" --check
    cargo clippy --manifest-path "$root/$crate/Cargo.toml" --all-targets --locked -- -D warnings
    cargo test --manifest-path "$root/$crate/Cargo.toml" --locked
done
"$root/mobile/android/gradlew" --project-dir "$root/mobile/android" --no-daemon \
    :app:lintDebug :app:testDebugUnitTest :app:assembleDebug :app:assembleRelease :app:assembleDebugAndroidTest "$@"
python3 "$root/scripts/android/check-native.py" "$root/mobile/android/app/build/outputs/apk/debug/app-debug.apk"
python3 "$root/scripts/android/check-native.py" "$root/mobile/android/app/build/outputs/apk/release/app-release-unsigned.apk"
