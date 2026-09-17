#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"

archive=${1:?archive path is required}
app=$(find "$archive/Products/Applications" -maxdepth 1 -name '*.app' -type d -print -quit)
[[ -n $app ]] || die 'archive does not contain an application'
extension=$(find "$app/PlugIns" -maxdepth 1 -name '*.appex' -type d -print -quit)
[[ -n $extension ]] || die 'archive does not contain ShareExtension'
security find-identity -v -p codesigning | grep -q '[0-9])' || die 'no usable code-signing identity in the selected keychain'
codesign --verify --deep --strict --verbose=2 "$app"
codesign --verify --strict --verbose=2 "$extension"
for product in "$app" "$extension"; do
    codesign -d --entitlements :- "$product" 2>/dev/null | plutil -convert xml1 -o - - | grep -Fq 'group.io.filebeam.ios' || die "missing Filebeam app group entitlement: $product"
done
find "$archive/dSYMs" -name '*.dSYM' -type d -print -quit | grep -q . || die 'archive does not contain dSYMs'
