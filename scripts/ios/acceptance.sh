#!/usr/bin/env bash
# Runs isolated Rust HTTP acceptance and optional real HTTPS XCUITest acceptance.
set -euo pipefail

# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
evidence=${IOS_ACCEPTANCE_EVIDENCE_DIR:-$root/test-results/ios/acceptance}
mkdir -p "$evidence"
backend_url=$(bash "$root/scripts/ios/peer-backend.sh" start)
cleanup() { bash "$root/scripts/ios/peer-backend.sh" stop; }
trap cleanup EXIT
native_ran=0
if [[ -f $root/client-ffi/examples/background_acceptance.rs ]]; then
    FILEBEAM_ACCEPTANCE_INSTANCE="$backend_url" bash "$root/scripts/ios/peer-native.sh" > "$evidence/native-background.log" 2>&1
    native_ran=1
else
    printf '%s\n' 'NATIVE NOT RUN: background_acceptance.rs is not present on this worktree.' > "$evidence/native-background.log"
fi
printf 'native_http_instance=%s\n' "$backend_url" > "$evidence/instance.txt"
# A device app rejects HTTP by design. The UI portion therefore needs a genuine
# HTTPS link fixture from an authorized test instance, never the loopback peer.
fixture=${IOS_ACCEPTANCE_HTTPS_LINK_FIXTURE:-}
if [[ -z $fixture ]]; then
    printf '%s\n' 'UI NOT RUN: IOS_ACCEPTANCE_HTTPS_LINK_FIXTURE was not supplied; native HTTP evidence is retained.' | tee "$evidence/ui-status.txt"
    [[ $native_ran == 1 ]] || exit 78
    exit 0
fi
[[ $fixture == https://* ]] || die 'IOS_ACCEPTANCE_HTTPS_LINK_FIXTURE must use HTTPS'
export FILEBEAM_ACCEPTANCE_HTTPS_LINK_FIXTURE="$fixture"
bash "$root/scripts/ios/build-rust.sh" debug
bash "$root/scripts/ios/generate-project.sh"
destination=${IOS_TEST_DESTINATION:-}
if [[ -z $destination ]]; then
    udid=$(xcrun simctl list devices available --json | python3 -c 'import json,sys; d=json.load(sys.stdin)["devices"]; print(next(x["udid"] for r,v in d.items() if "iOS" in r for x in v if x.get("isAvailable") and "iPhone" in x["name"]))')
    destination="platform=iOS Simulator,id=$udid"
fi
result="$evidence/PeerAcceptance.xcresult"
xcodebuild test -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -configuration Debug -destination "$destination" \
    -only-testing:FilebeamUITests/PeerAcceptanceTests -resultBundlePath "$result"
test -d "$result"
summary=$(xcrun xcresulttool get test-results summary --path "$result")
printf '%s' "$summary" > "$evidence/xcresult-summary.json"
python3 - "$evidence/xcresult-summary.json" <<'PY'
import json, sys
value = json.load(open(sys.argv[1]))
def values(node):
    if isinstance(node, dict):
        for key, item in node.items():
            yield key.lower(), item
            yield from values(item)
    elif isinstance(node, list):
        for item in node: yield from values(item)
counts = [(key, item) for key, item in values(value) if isinstance(item, int)]
executed = sum(item for key, item in counts if key in {'testsrun', 'testscount', 'executedtests'})
skipped = sum(item for key, item in counts if 'skip' in key)
if executed <= 0 or executed <= skipped:
    raise SystemExit('xcresult metadata shows no non-skipped selected tests')
PY
