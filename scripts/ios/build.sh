#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
configuration=${1:-Debug}; platform=${2:-simulator}
[[ $configuration == Debug || $configuration == Release ]] || die 'configuration must be Debug or Release'
[[ $platform == simulator || $platform == device ]] || die 'platform must be simulator or device'
if [[ $configuration == Debug ]]; then rust_profile=debug; else rust_profile=release; fi
bash "$root/scripts/ios/build-rust.sh" "$rust_profile"; bash "$root/scripts/ios/generate-project.sh"
if [[ $platform == simulator ]]; then xcodebuild build -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -configuration "$configuration" -destination 'generic/platform=iOS Simulator'; else xcodebuild build -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -configuration "$configuration" -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO; fi
