#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
bash "$root/scripts/ios/build-rust.sh" debug; bash "$root/scripts/ios/generate-project.sh"; mkdir -p "$root/test-results/ios"
destination=${IOS_TEST_DESTINATION:-}
test_os=${IOS_TEST_OS:-}
only_testing=${IOS_TEST_ONLY:-}
if [[ -n ${IOS_ACCEPTANCE_HTTPS_LINK_FIXTURE:-} ]]; then
    # xcodebuild forwards TEST_RUNNER_ variables to its test runner, stripping
    # the prefix. Keep the capability out of command-line build settings.
    export TEST_RUNNER_FILEBEAM_ACCEPTANCE_HTTPS_LINK_FIXTURE="$IOS_ACCEPTANCE_HTTPS_LINK_FIXTURE"
fi
if [[ -z $destination ]]; then
    udid=$(xcrun simctl list devices available --json | python3 -c '
import json, sys
requested = sys.argv[1]
matches = []
for runtime, devices in json.load(sys.stdin)["devices"].items():
    if not runtime.startswith("com.apple.CoreSimulator.SimRuntime.iOS-"): continue
    version = runtime.removeprefix("com.apple.CoreSimulator.SimRuntime.iOS-").replace("-", ".")
    if requested and not (version == requested or version.startswith(requested + ".")): continue
    matches.extend((version, device["name"], device["udid"]) for device in devices if device.get("isAvailable") and "iPhone" in device.get("name", ""))
if not matches:
    suffix = " for iOS " + requested if requested else ""
    raise SystemExit("no available iPhone simulator" + suffix + "; install that iOS runtime or set IOS_TEST_DESTINATION")
print(sorted(matches, reverse=True)[0][2])
' "$test_os")
    destination="platform=iOS Simulator,id=$udid"
fi
if [[ -n $only_testing ]]; then
    xcodebuild test -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -configuration Debug -destination "$destination" "-only-testing:$only_testing" -resultBundlePath "$root/test-results/ios/Filebeam.xcresult"
else
    xcodebuild test -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -configuration Debug -destination "$destination" -resultBundlePath "$root/test-results/ios/Filebeam.xcresult"
fi
