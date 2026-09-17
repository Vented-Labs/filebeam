#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
: "${IOS_SIGNING_XCCONFIG:?Set IOS_SIGNING_XCCONFIG to an external signing xcconfig}"; [[ -f $IOS_SIGNING_XCCONFIG ]] || die 'signing xcconfig does not exist'
bash "$root/scripts/ios/build-rust.sh" release; bash "$root/scripts/ios/generate-project.sh"
archive=${IOS_ARCHIVE_PATH:-$root/dist/ios/Filebeam.xcarchive}; mkdir -p "$(dirname -- "$archive")"
version=${IOS_MARKETING_VERSION:-1.0.0}; version=${version#v}
build_number=${IOS_BUILD_NUMBER:-1}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && $build_number =~ ^[1-9][0-9]*$ ]] || die 'release requires a numeric three-component version and a positive build number'
xcodebuild archive -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -configuration Release -destination 'generic/platform=iOS' -archivePath "$archive" -xcconfig "$IOS_SIGNING_XCCONFIG" "MARKETING_VERSION=$version" "CURRENT_PROJECT_VERSION=$build_number"
bash "$root/scripts/ios/verify-archive.sh" "$archive"
bash "$root/scripts/ios/verify-associated-domains.sh"
