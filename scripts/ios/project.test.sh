#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"

fixture=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-ios-project.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
# An optional executable lets the same generation check run with Linux XcodeGen.
tool=${1:-$tools_root/xcodegen-$IOS_XCODEGEN_VERSION/bin/xcodegen}
require_xcodegen_version "$tool"
xcodegen() { "$tool" "$@"; }

fixture_ios="$fixture/iOS project"
mkdir -p "$fixture_ios"
cp "$ios_root/project.yml" "$fixture_ios/project.yml"
cp -R "$ios_root/Config" "$fixture_ios/Config"
for directory in App Features Platform ShareExtension/Shared DesignSystem PreviewSupport Resources Tests UITests; do
    mkdir -p "$fixture_ios/$directory"
done
for package in FilebeamCore FilebeamDomain; do
    mkdir -p "$fixture_ios/Packages/$package"
    cp "$ios_root/Packages/$package/Package.swift" "$fixture_ios/Packages/$package/"
done
printf 'import Foundation\n' > "$fixture_ios/App/Fixture.swift"
original=$(shasum -a 256 "$fixture_ios/Config/"*.plist "$fixture_ios/Config/"*.entitlements)
ios_root="$fixture_ios"
generate_project
project="$ios_root/Filebeam.xcodeproj/project.pbxproj"
[[ -s $project ]] || die 'project was not generated at the path used by xcodebuild'
[[ ! -d "$ios_root/Filebeam.xcodeproj/Filebeam.xcodeproj" ]] || die 'project was nested inside another project'
[[ $(shasum -a 256 "$fixture_ios/Config/"*.plist "$fixture_ios/Config/"*.entitlements) == "$original" ]] || die 'project generation overwrote source plists or entitlements'
grep -Fq 'ENABLE_TESTABILITY = YES;' "$project" || die 'XcodeGen debug presets were not applied'
for file in Filebeam-Info.plist ShareExtension-Info.plist Filebeam.entitlements ShareExtension.entitlements; do
    grep -Fq "Config/$file" "$project" || die "generated project does not reference $file"
done
