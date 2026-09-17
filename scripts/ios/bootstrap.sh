#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
require_macos
mkdir -p "$tools_root"
lock="$tools_root/bootstrap.lock"
until mkdir "$lock" 2>/dev/null; do sleep 1; done
trap 'rmdir "$lock"' EXIT
installation="$tools_root/xcodegen-$IOS_XCODEGEN_VERSION"
tool="$installation/bin/xcodegen"
expected_xcodegen_hash=$(awk -F= '/^XCODEGEN_SHA256=/{print $2}' "$root/scripts/ios/toolchain.lock")
if [[ ! -x $tool || ! -f "$installation/share/xcodegen/SettingPresets/base.yml" ]]; then
    archive="$tools_root/xcodegen-$IOS_XCODEGEN_VERSION.zip" staging="$tools_root/xcodegen-$IOS_XCODEGEN_VERSION.staging"
    rm -rf "$staging"
    curl --fail --location --retry 3 --proto '=https' --tlsv1.2 --output "$archive" "https://github.com/yonaskolb/XcodeGen/releases/download/$IOS_XCODEGEN_VERSION/xcodegen.zip"
    [[ $(shasum -a 256 "$archive" | awk '{print $1}') == "$expected_xcodegen_hash" ]] || die 'XcodeGen archive checksum mismatch'
    mkdir -p "$staging"
    ditto -x -k "$archive" "$staging"
    chmod 0755 "$staging/xcodegen/bin/xcodegen"
    rm -rf "$installation"
    mv "$staging/xcodegen" "$installation"
    rm -rf "$staging" "$archive"
fi
require_xcodegen_version "$tool"
rustup toolchain install "$IOS_RUST_VERSION" --profile minimal --component rustfmt --component clippy
[[ $(rustup run "$IOS_RUST_VERSION" rustc --version) == "rustc $IOS_RUST_VERSION"* ]] || die "Rust $IOS_RUST_VERSION installation failed"
rustup target add --toolchain "$IOS_RUST_VERSION" aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
toolchain_identity
