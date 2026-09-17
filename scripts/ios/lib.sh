#!/usr/bin/env bash
set -euo pipefail
export IOS_RUST_VERSION=1.98.0
IOS_XCODEGEN_VERSION=2.41.0
IOS_XCODE_VERSION=${IOS_XCODE_VERSION:-26.3}
if [[ -z ${IOS_DEVELOPER_DIR:-} ]]; then
    if [[ -d /Applications/Xcode_${IOS_XCODE_VERSION}.app/Contents/Developer ]]; then
        IOS_DEVELOPER_DIR="/Applications/Xcode_${IOS_XCODE_VERSION}.app/Contents/Developer"
    else
        IOS_DEVELOPER_DIR=$(xcode-select -p 2>/dev/null || true)
    fi
fi
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
ios_root="$root/mobile/ios"
tools_root="$root/.ios-tools"
die() { printf 'ios: %s\n' "$*" >&2; exit 1; }
require_xcodegen_version() {
    local tool=$1 actual
    if ! actual=$("$tool" --version); then
        die "could not determine XcodeGen version at \"$tool\""
    fi
    [[ $actual == "Version: $IOS_XCODEGEN_VERSION" ]] || die "expected XcodeGen $IOS_XCODEGEN_VERSION at \"$tool\"; got \"$actual\""
}
require_macos() {
    [[ $(uname -s) == Darwin ]] || die 'macOS with Xcode is required; iOS artifacts cannot be built on this host'
    [[ -n $IOS_DEVELOPER_DIR && -d $IOS_DEVELOPER_DIR ]] || die 'select a full Xcode with IOS_DEVELOPER_DIR or xcode-select'
    export DEVELOPER_DIR="$IOS_DEVELOPER_DIR"
    command -v xcodebuild >/dev/null || die 'select a full Xcode with xcode-select'
    command -v xcrun >/dev/null || die 'select a full Xcode with xcode-select'
    local version build
    version=$(xcodebuild -version | awk '/Xcode/{print $2; exit}')
    build=$(xcodebuild -version | awk '/Build version/{print $3; exit}')
    [[ $version == "$IOS_XCODE_VERSION" ]] || die "Xcode $IOS_XCODE_VERSION is required; selected ${version:-unknown} (${build:-unknown})"
}
toolchain_identity() {
    require_macos
    printf 'xcode=%s\nbuild=%s\nsdk=%s\narch=%s\n' \
        "$(xcodebuild -version | awk '/Xcode/{print $2; exit}')" \
        "$(xcodebuild -version | awk '/Build version/{print $3; exit}')" \
        "$(xcrun --sdk iphoneos --show-sdk-version)" "$(uname -m)"
}
input_fingerprint() { { toolchain_identity; git -C "$root" rev-parse HEAD; python3 "$root/scripts/ios/source-fingerprint.py"; } | shasum -a 256 | awk '{print $1}'; }
require_generated() {
    local generated="$ios_root/Packages/FilebeamCore/Generated" artifact="$ios_root/Packages/FilebeamCore/Artifacts/FilebeamCoreFFI.xcframework"
    [[ -f "$generated/FilebeamCore.swift" && -f "$generated/FilebeamCoreFFI.h" && -f "$generated/FilebeamCoreFFI.modulemap" && -d "$artifact" && -f "$generated/input-fingerprint" ]] || die 'generated UniFFI artifacts are absent; run scripts/ios/build-rust.sh'
    [[ $(<"$generated/input-fingerprint") == "$(input_fingerprint)" ]] || die 'generated UniFFI artifacts are stale; run scripts/ios/build-rust.sh'
}
xcodegen() { "$tools_root/xcodegen-$IOS_XCODEGEN_VERSION/bin/xcodegen" "$@"; }
