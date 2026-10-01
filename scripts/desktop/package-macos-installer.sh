#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 3 ]] || exit 64
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
# Ad-hoc signing needs no account and preserves bundle validation on Apple Silicon.
codesign --force --sign - --identifier io.filebeam.desktop "$app"
codesign --verify --deep --strict "$app"
bash "$root/scripts/desktop/package-update-payload.sh" "$tag" macos "${ARCH:-aarch64}" "$app" "$out"
hdiutil create -ov -format UDZO -srcfolder "$app" "$out/filebeam-desktop-$tag-macos-${ARCH:-aarch64}.dmg"
