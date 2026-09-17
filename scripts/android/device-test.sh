#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
if [[ ${FILEBEAM_ANDROID_DEVICE_CONTAINER:-} != 1 ]]; then
    report_dir=${FILEBEAM_ANDROID_REPORT_DIR:-"$root/test-results/android"}
    [[ $report_dir == "$root"/* ]] || { printf '%s\n' 'FILEBEAM_ANDROID_REPORT_DIR must be inside the workspace' >&2; exit 2; }
    container_report_dir=/workspace/${report_dir#"$root"/}
    devices=()
    accel=off
    if [[ -e /dev/kvm ]]; then
        devices+=(--device /dev/kvm --group-add "$(stat -c %g /dev/kvm)")
        accel=on
    fi
    # Recent emulator modem simulation uses ::1 with AI_ADDRCONFIG. A network
    # with a non-loopback IPv6 address is required even for this local traffic.
    network_args=()
    network_mode=${FILEBEAM_ANDROID_DEVICE_NETWORK:-isolated}
    # adb reverse targets the adb server's localhost. The container must share
    # the host network for a localhost disposable peer to be reachable there.
    if [[ ${FILEBEAM_ANDROID_HTTP_PROBE:-} == http://127.0.0.1:* || ${FILEBEAM_ANDROID_HTTP_PROBE:-} == http://localhost:* ]]; then
        network_mode=host
    fi
    if [[ $network_mode == host ]]; then
        network_args+=(--network host)
    else
        network="filebeam-android-test-$$"
        subnet=$(printf 'fd42:fb:%x::/64' "$(( $$ % 65535 ))")
        docker network create --ipv6 --subnet "$subnet" "$network" >/dev/null
        trap 'docker network rm "$network" >/dev/null 2>&1 || true' EXIT
        network_args+=(--network "$network" --sysctl net.ipv6.conf.all.disable_ipv6=0 --sysctl net.ipv6.conf.default.disable_ipv6=0)
    fi
    device_status=0
    docker run --rm --init "${devices[@]}" "${network_args[@]}" \
        --memory "${FILEBEAM_ANDROID_MEMORY:-6g}" --cpus "${FILEBEAM_ANDROID_CPUS:-4}" \
        --user "$(id -u):$(id -g)" --env HOME=/tmp/home \
        --env FILEBEAM_ANDROID_DEVICE_CONTAINER=1 --env FILEBEAM_ANDROID_ACCEL="$accel" \
        --env FILEBEAM_ANDROID_AVD_DISK_SIZE="${FILEBEAM_ANDROID_AVD_DISK_SIZE:-8G}" \
        --env FILEBEAM_ANDROID_AVD_MEMORY="${FILEBEAM_ANDROID_AVD_MEMORY:-2048}" \
        --env FILEBEAM_ANDROID_INSTRUMENTATION_TIMEOUT="${FILEBEAM_ANDROID_INSTRUMENTATION_TIMEOUT:-3600}" \
        --env FILEBEAM_ANDROID_HTTP_PROBE="${FILEBEAM_ANDROID_HTTP_PROBE:-}" \
        --env FILEBEAM_ANDROID_LIVE_LOGCAT_TAG="${FILEBEAM_ANDROID_LIVE_LOGCAT_TAG:-}" \
        --env FILEBEAM_ANDROID_PULL_EVIDENCE="${FILEBEAM_ANDROID_PULL_EVIDENCE:-}" \
        --env FILEBEAM_ANDROID_REPORT_DIR="$container_report_dir" \
        --volume "$root:/workspace" --workdir /workspace \
        "${FILEBEAM_ANDROID_EMULATOR_IMAGE:-filebeam-android-emulator:api35-16k}" \
        bash /workspace/scripts/android/device-test.sh "$@" || device_status=$?
    [[ -s $report_dir/emulator.log && -s $report_dir/logcat.txt ]] || {
        printf 'Android device reports are incomplete: %s\n' "$report_dir" >&2
        exit 1
    }
    exit "$device_status"
fi

report_dir=${FILEBEAM_ANDROID_REPORT_DIR:-"$root/test-results/android"}
ensure_report_dir() { mkdir -p "$report_dir"; }
mkdir -p "$HOME"
ensure_report_dir
ulimit -c 0
avdmanager create avd --force --name filebeam-test --package "$FILEBEAM_TEST_SYSTEM_IMAGE" --device pixel_6
printf '\ndisk.dataPartition.size=%s\n' "${FILEBEAM_ANDROID_AVD_DISK_SIZE:-8G}" >> "$HOME/.android/avd/filebeam-test.avd/config.ini"
ensure_report_dir
emulator -avd filebeam-test -no-window -no-audio -no-boot-anim -no-snapshot \
    -no-metrics -gpu swangle \
    -accel "$FILEBEAM_ANDROID_ACCEL" -memory "${FILEBEAM_ANDROID_AVD_MEMORY:-2048}" -cores 2 \
    > "$report_dir/emulator.log" 2>&1 &
emulator_pid=$!
live_logcat_pid=''
cleanup_device() {
    ensure_report_dir
    if [[ -n $live_logcat_pid ]]; then
        kill "$live_logcat_pid" 2>/dev/null || true
    fi
    adb logcat -d > "$report_dir/logcat.txt" 2>/dev/null || true
    adb emu kill >/dev/null 2>&1 || true
    kill "$emulator_pid" 2>/dev/null || true
}
trap cleanup_device EXIT
adb start-server
booted=false
for ((attempt=0; attempt<150; attempt++)); do
    kill -0 "$emulator_pid" 2>/dev/null || { printf '%s\n' "Emulator exited; see $report_dir/emulator.log" >&2; exit 1; }
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
# Probe variables must expand in the Android shell, not on the host.
# shellcheck disable=SC2016
page_size=$(adb shell '
    if command -v getconf >/dev/null 2>&1; then
        getconf PAGE_SIZE
    else
        while read -r key value _; do
            if [ "$key" = KernelPageSize: ]; then printf "%s\n" "$((value * 1024))"; exit 0; fi
        done < /proc/self/smaps
    fi
' | tr -d '\r')
[[ $page_size =~ ^[0-9]+$ ]] || { printf 'Could not determine guest page size: %s\n' "$page_size" >&2; exit 1; }
printf '%s\n' "$page_size" > "$report_dir/page-size.txt"
if [[ $FILEBEAM_TEST_SYSTEM_IMAGE == *ps16k* ]]; then
    [[ $page_size == 16384 ]] || { printf 'Expected 16-KiB pages, got %s\n' "$page_size" >&2; exit 1; }
fi
# Push first rather than holding an install-session pipe open through cold-boot
# dex optimization on the software-rendered emulator.
adb install --no-streaming -r "$root/mobile/android/app/build/outputs/apk/debug/app-debug.apk"
adb install --no-streaming -r "$root/mobile/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
if [[ -n ${FILEBEAM_ANDROID_HTTP_PROBE:-} ]]; then
    ensure_report_dir
    adb shell svc wifi enable >/dev/null 2>&1 || true
    adb shell svc data enable >/dev/null 2>&1 || true
    adb shell ip route > "$report_dir/http-routes.txt" 2>&1 || true
    probe=${FILEBEAM_ANDROID_HTTP_PROBE#http://}
    probe=${probe#https://}
    probe=${probe%%/*}
    probe_host=${probe%:*}
    probe_port=${probe##*:}
    [[ $probe_host != "$probe" && $probe_port =~ ^[0-9]+$ ]] || { printf '%s\n' 'FILEBEAM_ANDROID_HTTP_PROBE must include host:port' >&2; exit 2; }
    # The emulator may lack a usable default route to the Docker host. Reverse
    # forwarding makes a localhost peer explicit and does not depend on 10.0.2.2.
    if [[ $probe_host == 127.0.0.1 || $probe_host == localhost ]]; then
        adb reverse "tcp:$probe_port" "tcp:$probe_port"
        probe_host=127.0.0.1
        printf 'adb reverse tcp:%s tcp:%s\n' "$probe_port" "$probe_port" > "$report_dir/http-forward.txt"
    fi
    adb reverse --list > "$report_dir/http-probe.txt"
fi
ensure_report_dir
adb shell cat /proc/meminfo > "$report_dir/meminfo-before.txt" 2>&1 || true
# Acceptance commands accept a trailing test class. Convert it to AndroidX's
# class filter so the runner remains the only instrumentation component.
instrumentation_args=("$@")
if [[ ${#instrumentation_args[@]} -gt 0 && ${instrumentation_args[-1]} == io.* ]]; then
    test_class=${instrumentation_args[-1]}
    unset 'instrumentation_args[-1]'
    instrumentation_args+=(-e class "$test_class")
fi
if [[ -n ${FILEBEAM_ANDROID_LIVE_LOGCAT_TAG:-} ]]; then
    adb logcat -v brief "${FILEBEAM_ANDROID_LIVE_LOGCAT_TAG}:I" '*:S' &
    live_logcat_pid=$!
fi
if [[ -n ${FILEBEAM_ANDROID_LIVE_LOGCAT_TAG:-} ]]; then
    timeout "${FILEBEAM_ANDROID_INSTRUMENTATION_TIMEOUT:-3600}" adb shell am instrument -w "${instrumentation_args[@]}" io.filebeam.android.debug.test/androidx.test.runner.AndroidJUnitRunner 2>&1 | tee "$report_dir/instrumentation.txt" &
else
    timeout "${FILEBEAM_ANDROID_INSTRUMENTATION_TIMEOUT:-3600}" adb shell am instrument -w "${instrumentation_args[@]}" io.filebeam.android.debug.test/androidx.test.runner.AndroidJUnitRunner > "$report_dir/instrumentation.txt" 2>&1 &
fi
instrumentation_pid=$!
(
    while kill -0 "$instrumentation_pid" 2>/dev/null; do
        printf 'timestamp=%s\n' "$(date --iso-8601=seconds)"
        adb shell cat /proc/meminfo 2>&1 || true
        adb shell dumpsys meminfo io.filebeam.android.debug 2>&1 || true
        sleep 5
    done
) > "$report_dir/rss.txt" &
rss_pid=$!
if wait "$instrumentation_pid"; then instrumentation_status=0; else instrumentation_status=$?; fi
kill "$rss_pid" 2>/dev/null || true
wait "$rss_pid" 2>/dev/null || true
result=$(<"$report_dir/instrumentation.txt")
printf '%s\n' "$result"
if [[ ${FILEBEAM_ANDROID_PULL_EVIDENCE:-} == 1 ]]; then
    adb exec-out run-as io.filebeam.android.debug tar -C files -cf - native-kit-evidence > "$report_dir/evidence.tar"
    mkdir -p "$report_dir/evidence"
    tar -xf "$report_dir/evidence.tar" -C "$report_dir/evidence"
fi
[[ $instrumentation_status == 0 ]] || exit "$instrumentation_status"
[[ $result == *'OK ('* ]] || exit 1
