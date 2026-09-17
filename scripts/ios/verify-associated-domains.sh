#!/usr/bin/env bash
set -euo pipefail

# Apple validates this endpoint independently; fail distribution when it cannot
# authorize the exact signed production application identifier.
: "${IOS_TEAM_ID:?Set IOS_TEAM_ID to the Apple Developer Team ID}"
aasa=$(curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 'https://filebeam.io/.well-known/apple-app-site-association')
python3 -c '
import json, sys
expected = sys.argv[1]
details = json.load(sys.stdin).get("applinks", {}).get("details", [])
if not any(expected in ([detail.get("appID")] + detail.get("appIDs", [])) for detail in details):
    raise SystemExit(f"AASA does not authorize {expected}")
' "$IOS_TEAM_ID.io.filebeam.ios" <<<"$aasa"
