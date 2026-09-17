#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
: "${IOS_EXPORT_OPTIONS_PLIST:?Set IOS_EXPORT_OPTIONS_PLIST to an external ExportOptions.plist}"
archive=${IOS_ARCHIVE_PATH:-$root/dist/ios/Filebeam.xcarchive}; output=${IOS_EXPORT_PATH:-$root/dist/ios/export}
[[ -d $archive && -f $IOS_EXPORT_OPTIONS_PLIST ]] || die 'archive or export options are absent'
rm -rf "$output"; xcodebuild -exportArchive -archivePath "$archive" -exportPath "$output" -exportOptionsPlist "$IOS_EXPORT_OPTIONS_PLIST"
for candidate in "$output"/*.ipa; do [[ -s $candidate ]] && { ipa=$candidate; break; }; done
[[ -n ${ipa:-} ]] || die 'export did not produce an IPA'
