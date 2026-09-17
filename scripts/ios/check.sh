#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"

bash "$root/scripts/ios/bootstrap.test.sh"
bash "$root/scripts/ios/build.test.sh"
bash "$root/scripts/ios/toolchain.test.sh"
bash "$root/scripts/ios/bootstrap.sh"
bash "$root/scripts/ios/project.test.sh"
for crate in client-core client-ffi; do
    cargo "+$IOS_RUST_VERSION" fmt --manifest-path "$root/$crate/Cargo.toml" --check
    cargo "+$IOS_RUST_VERSION" clippy --manifest-path "$root/$crate/Cargo.toml" --all-targets --locked -- -D warnings
    cargo "+$IOS_RUST_VERSION" test --manifest-path "$root/$crate/Cargo.toml" --locked
done
swift test --package-path "$ios_root/Packages/FilebeamDomain"
bash "$root/scripts/ios/build.sh" Debug simulator
bash "$root/scripts/ios/build.sh" Release device
bash "$root/scripts/ios/test.sh"
