#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
: "${IOS_EXPORT_OPTIONS_PLIST:?Set IOS_EXPORT_OPTIONS_PLIST to an external ExportOptions.plist}"
archive=${IOS_ARCHIVE_PATH:-$root/dist/ios/Filebeam.xcarchive}; output=${IOS_EXPORT_PATH:-$root/dist/ios/export}
[[ -d $archive && -f $IOS_EXPORT_OPTIONS_PLIST ]] || die 'archive or export options are absent'
rm -rf "$output"; xcodebuild -exportArchive -archivePath "$archive" -exportPath "$output" -exportOptionsPlist "$IOS_EXPORT_OPTIONS_PLIST"
find "$output" -maxdepth 1 -name '*.ipa' -type f -size +0c -print -quit | grep -q . || die 'export did not produce an IPA'
