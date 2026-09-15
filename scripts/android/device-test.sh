#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
if [[ ${FILEBEAM_ANDROID_DEVICE_CONTAINER:-} != 1 ]]; then
    devices=()
    accel=off
    if [[ -e /dev/kvm ]]; then
        devices+=(--device /dev/kvm --group-add "$(stat -c %g /dev/kvm)")
        accel=on
    fi
    # Recent emulator modem simulation uses ::1 with AI_ADDRCONFIG. A network
    # with a non-loopback IPv6 address is required even for this local traffic.
    network_args=()
    if [[ ${FILEBEAM_ANDROID_DEVICE_NETWORK:-isolated} == host ]]; then
        network_args+=(--network host)
    else
        network="filebeam-android-test-$$"
        subnet=$(printf 'fd42:fb:%x::/64' "$(( $$ % 65535 ))")
        docker network create --ipv6 --subnet "$subnet" "$network" >/dev/null
        trap 'docker network rm "$network" >/dev/null 2>&1 || true' EXIT
        network_args+=(--network "$network" --sysctl net.ipv6.conf.all.disable_ipv6=0 --sysctl net.ipv6.conf.default.disable_ipv6=0)
    fi
    docker run --rm --init "${devices[@]}" "${network_args[@]}" \
        --memory "${FILEBEAM_ANDROID_MEMORY:-6g}" --cpus "${FILEBEAM_ANDROID_CPUS:-4}" \
        --user "$(id -u):$(id -g)" --env HOME=/tmp/home \
        --env FILEBEAM_ANDROID_DEVICE_CONTAINER=1 --env FILEBEAM_ANDROID_ACCEL="$accel" \
        --volume "$root:/workspace" --workdir /workspace \
        "${FILEBEAM_ANDROID_EMULATOR_IMAGE:-filebeam-android-emulator:api35-16k}" \
        bash /workspace/scripts/android/device-test.sh "$@"
    exit
fi

mkdir -p "$HOME" "$root/test-results/android"
ulimit -c 0
avdmanager create avd --force --name filebeam-test --package "$FILEBEAM_TEST_SYSTEM_IMAGE" --device pixel_6
printf '\ndisk.dataPartition.size=%s\n' "${FILEBEAM_ANDROID_AVD_DISK_SIZE:-8G}" >> "$HOME/.android/avd/filebeam-test.avd/config.ini"
emulator -avd filebeam-test -no-window -no-audio -no-boot-anim -no-snapshot \
    -no-metrics -gpu swangle \
    -accel "$FILEBEAM_ANDROID_ACCEL" -memory 2048 -cores 2 \
    > "$root/test-results/android/emulator.log" 2>&1 &
emulator_pid=$!
cleanup_device() {
    adb logcat -d > "$root/test-results/android/logcat.txt" 2>/dev/null || true
    adb emu kill >/dev/null 2>&1 || true
    kill "$emulator_pid" 2>/dev/null || true
}
trap cleanup_device EXIT
adb start-server
booted=false
for ((attempt=0; attempt<150; attempt++)); do
    kill -0 "$emulator_pid" 2>/dev/null || { printf '%s\n' 'Emulator exited; see test-results/android/emulator.log' >&2; exit 1; }
    if [[ $(timeout 5 adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r') == 1 ]]; then booted=true; break; fi
    sleep 2
done
[[ $booted == true ]] || { printf '%s\n' 'Emulator boot timed out' >&2; exit 1; }
package_ready=false
for ((attempt=0; attempt<30; attempt++)); do
    if [[ $(timeout 5 adb shell pm path android 2>/dev/null) == *package:* ]]; then package_ready=true; break; fi
    sleep 2
done
[[ $package_ready == true ]] || { printf '%s\n' 'Package manager did not become ready' >&2; exit 1; }
page_size=$(adb shell getconf PAGE_SIZE | tr -d '\r')
if [[ $FILEBEAM_TEST_SYSTEM_IMAGE == *ps16k* ]]; then
    [[ $page_size == 16384 ]] || { printf 'Expected 16-KiB pages, got %s\n' "$page_size" >&2; exit 1; }
fi
# Push first rather than holding an install-session pipe open through cold-boot
# dex optimization on the software-rendered emulator.
adb install --no-streaming -r "$root/mobile/android/app/build/outputs/apk/debug/app-debug.apk"
adb install --no-streaming -r "$root/mobile/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
result=$(timeout 180 adb shell am instrument -w "$@" io.filebeam.android.debug.test/androidx.test.runner.AndroidJUnitRunner)
printf '%s\n' "$result" | tee "$root/test-results/android/instrumentation.txt"
[[ $result == *'OK ('* ]] || exit 1
