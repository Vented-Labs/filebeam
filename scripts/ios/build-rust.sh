#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
profile=${1:-release}; include_intel=${IOS_INCLUDE_X86_64_SIMULATOR:-1}
[[ $profile == debug || $profile == release ]] || die 'profile must be debug or release'
[[ $include_intel == 0 || $include_intel == 1 ]] || die 'IOS_INCLUDE_X86_64_SIMULATOR must be 0 or 1'
require_macos; bash "$root/scripts/ios/bootstrap.sh"
package="$ios_root/Packages/FilebeamCore" generated="$package/Generated" artifacts="$package/Artifacts" target_dir="${CARGO_TARGET_DIR:-$root/client-ffi/target/ios}"
# Retain Rust DWARF in release artifacts so Xcode archives can create useful dSYMs.
export CARGO_PROFILE_RELEASE_STRIP=none
export CARGO_PROFILE_RELEASE_DEBUG=2
targets=(aarch64-apple-ios aarch64-apple-ios-sim); [[ $include_intel == 0 ]] || targets+=(x86_64-apple-ios)
for target in "${targets[@]}"; do
    if [[ $profile == release ]]; then
        cargo "+$IOS_RUST_VERSION" build --manifest-path "$root/client-ffi/Cargo.toml" --locked --lib --target "$target" --release --target-dir "$target_dir"
    else
        cargo "+$IOS_RUST_VERSION" build --manifest-path "$root/client-ffi/Cargo.toml" --locked --lib --target "$target" --target-dir "$target_dir"
    fi
done
device="$target_dir/aarch64-apple-ios/$profile/libfilebeam_client_ffi.a" sim="$target_dir/aarch64-apple-ios-sim/$profile/libfilebeam_client_ffi.a"
[[ -s $device && -s $sim ]] || die 'Cargo did not produce required static libraries'
staging=$(mktemp -d "$package/.build.XXXXXX"); trap 'rm -rf "$staging"' EXIT
mkdir -p "$staging/bindings" "$staging/headers"
# Bindings, C header, and module map come from the same device library packaged below.
python3 "$root/scripts/ios/generate-bindings.py" --library "$device" --out-dir "$staging/bindings"
python3 "$root/scripts/ios/normalize-bindings.py" "$staging/bindings"
for file in FilebeamCore.swift FilebeamCoreFFI.h FilebeamCoreFFI.modulemap; do [[ -s "$staging/bindings/$file" ]] || die "UniFFI did not generate $file"; done
grep -Eq '^module[[:space:]]+FilebeamCoreFFI[[:space:]]*\{' "$staging/bindings/FilebeamCoreFFI.modulemap" || die 'UniFFI module map does not declare FilebeamCoreFFI'
cp "$staging/bindings/FilebeamCoreFFI.h" "$staging/headers/"
# XCFramework headers require the conventional name, independent of UniFFI's output filename.
cp "$staging/bindings/FilebeamCoreFFI.modulemap" "$staging/headers/module.modulemap"
cp "$device" "$staging/libFilebeamCoreFFI-device.a"
if [[ $include_intel == 1 ]]; then lipo -create "$sim" "$target_dir/x86_64-apple-ios/$profile/libfilebeam_client_ffi.a" -output "$staging/libFilebeamCoreFFI-simulator.a"; else cp "$sim" "$staging/libFilebeamCoreFFI-simulator.a"; fi
xcodebuild -create-xcframework -library "$staging/libFilebeamCoreFFI-device.a" -headers "$staging/headers" -library "$staging/libFilebeamCoreFFI-simulator.a" -headers "$staging/headers" -output "$staging/FilebeamCoreFFI.xcframework"
rm -rf "$generated" "$artifacts"; mkdir -p "$generated" "$artifacts"
cp "$staging/bindings/FilebeamCore.swift" "$staging/bindings/FilebeamCoreFFI.h" "$staging/bindings/FilebeamCoreFFI.modulemap" "$generated/"
cp "$staging/headers/module.modulemap" "$generated/"
input_fingerprint > "$generated/input-fingerprint"; mv "$staging/FilebeamCoreFFI.xcframework" "$artifacts/"
