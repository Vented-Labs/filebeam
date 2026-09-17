#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
require_macos
version=${IOS_TEST_RUNTIME_VERSION:?set IOS_TEST_RUNTIME_VERSION to the required simulator release}

runtime_identifier() {
    xcrun simctl list runtimes --json | python3 -c '
import json, sys
version = sys.argv[1]
for runtime in json.load(sys.stdin)["runtimes"]:
    if runtime.get("isAvailable") and runtime.get("version") == version and runtime["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS-"):
        print(runtime["identifier"])
        break
' "$version"
}

runtime=$(runtime_identifier)
if [[ -z $runtime ]]; then
    download=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-ios-runtime.XXXXXX")
    trap 'rm -rf "$download"' EXIT
    xcodebuild -downloadPlatform iOS -buildVersion "$version" -exportPath "$download"
    imported=false
    for image in "$download"/*.dmg; do
        [[ -f $image ]] || continue
        xcodebuild -importPlatform "$image"
        imported=true
    done
    [[ $imported == true ]] || die "download did not produce an iOS $version simulator image"
    for _ in {1..30}; do
        runtime=$(runtime_identifier)
        [[ -z $runtime ]] || break
        sleep 2
    done
    [[ -n $runtime ]] || die "iOS $version simulator runtime is unavailable after installation"
fi

device=$(xcrun simctl list devices available --json | python3 -c '
import json, sys
for device in json.load(sys.stdin)["devices"].get(sys.argv[1], []):
    if device.get("isAvailable") and "iPhone" in device.get("name", ""):
        print(device["udid"])
        break
' "$runtime")
if [[ -z $device ]]; then
    device=$(xcrun simctl create "Filebeam iOS $version" com.apple.CoreSimulator.SimDeviceType.iPhone-15 "$runtime")
fi
printf 'Prepared iOS %s simulator: %s\n' "$version" "$device"
