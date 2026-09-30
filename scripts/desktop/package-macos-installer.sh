#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || exit 64
unsigned=${FILEBEAM_DESKTOP_UNSIGNED:-0}
[[ $unsigned == 1 || ( -n ${APPLE_SIGNING_IDENTITY:-} && -n ${APPLE_NOTARY_PROFILE:-} ) ]] || { printf 'binary tag output plus APPLE_SIGNING_IDENTITY and APPLE_NOTARY_PROFILE are required\n' >&2; exit 1; }
binary=$1 tag=$2 out=$3
[[ $tag =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || exit 64
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
app="$out/Filebeam.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 0755 "$binary" "$app/Contents/MacOS/filebeam"
install -m 0644 "$root/desktop/packaging/icons/filebeam.icns" "$app/Contents/Resources/Filebeam.icns"
cp "$root/desktop/packaging/macos/Info.plist" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string ${tag#v}" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleVersion string ${tag#v}" "$app/Contents/Info.plist"
if [[ $unsigned != 1 ]]; then
    codesign --force --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$app"
    ditto -c -k --keepParent "$app" "$out/notarize.zip"
    xcrun notarytool submit "$out/notarize.zip" --keychain-profile "$APPLE_NOTARY_PROFILE" --wait
    rm "$out/notarize.zip"
    xcrun stapler staple "$app"
fi
bash "$root/scripts/desktop/package-update-payload.sh" "$tag" macos "${ARCH:-aarch64}" "$app" "$out"
hdiutil create -ov -format UDZO -srcfolder "$app" "$out/filebeam-desktop-$tag-macos-${ARCH:-aarch64}.dmg"
if [[ $unsigned != 1 ]]; then
    xcrun notarytool submit "$out/filebeam-desktop-$tag-macos-${ARCH:-aarch64}.dmg" --keychain-profile "$APPLE_NOTARY_PROFILE" --wait
    xcrun stapler staple "$out/filebeam-desktop-$tag-macos-${ARCH:-aarch64}.dmg"
fi
