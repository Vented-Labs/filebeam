#!/usr/bin/env bash
# Opt-in native-service acceptance. It only uses the disposable Android HTTP peer.
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
results=${FILEBEAM_ACCEPTANCE_RESULTS:-"$root/.filebeam/test-results/native-services-acceptance"}
instance=${FILEBEAM_ACCEPTANCE_INSTANCE:-http://127.0.0.1:8019}
target="$root/.filebeam/cargo-target-1.98"
binary="$target/release/examples/services_acceptance"
mkdir -p "$results/logs" "$target"
"$root/scripts/android/peer-http.sh" start > "$results/logs/peer-start.log" 2>&1
docker exec filebeam-android-peer-http php artisan tinker --execute="App\\Models\\InstanceSetting::query()->updateOrCreate(['key' => 'username_routing'], ['value' => true]);" > "$results/logs/username-routing.log" 2>&1
docker run --rm --network host --mount "type=bind,src=$root,dst=/work" --mount "type=bind,src=$target,dst=/target" --workdir /work/client-core --env CARGO_TARGET_DIR=/target filebeam-beam-tooling:rust-1.98.0 cargo build --release --example services_acceptance > "$results/logs/rust-build.log" 2>&1
FILEBEAM_ACCEPTANCE_INSTANCE="$instance" FILEBEAM_ACCEPTANCE_RESULTS="$results" "$binary" notes | tee "$results/native-notes.log"
account_status=0
FILEBEAM_ACCEPTANCE_INSTANCE="$instance" FILEBEAM_ACCEPTANCE_RESULTS="$results" "$binary" account > "$results/account.log" 2>&1 || account_status=$?
FILEBEAM_ACCEPTANCE_INSTANCE="$instance" FILEBEAM_ACCEPTANCE_RESULTS="$results" NATIVE_ACCEPTANCE_BINARY="$binary" npx playwright test --config "$root/scripts/services-acceptance.playwright.config.ts" | tee "$results/browser.log"
if [ "${FILEBEAM_LIVE_ACCEPTANCE:-0}" = 1 ]; then
    FILEBEAM_ACCEPTANCE_INSTANCE=http://127.0.0.1:8027 FILEBEAM_ACCEPTANCE_RESULTS="$results" "$binary" live | tee "$results/live-native.log"
    FILEBEAM_ACCEPTANCE_INSTANCE=http://127.0.0.1:8027 FILEBEAM_ACCEPTANCE_RESULTS="$results" NATIVE_ACCEPTANCE_BINARY="$binary" npx playwright test --config "$root/scripts/services-live-acceptance.playwright.config.ts" | tee "$results/live-browser.log"
fi
if [ "$account_status" -eq 0 ]; then
    printf 'native services acceptance passed\n' > "$results/summary.txt"
else
    printf 'hosted notes passed; account inbox download blocked; see account.log\n' > "$results/summary.txt"
    exit "$account_status"
fi
