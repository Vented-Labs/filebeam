#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
require_macos

bash "$root/scripts/ios/bootstrap.test.sh"
bash "$root/scripts/ios/build.test.sh"
bash "$root/scripts/ios/toolchain.test.sh"
bash "$root/scripts/ios/bootstrap.sh"
bash "$root/scripts/ios/project.test.sh"
for crate in client-core client-ffi; do
    cargo "+$IOS_RUST_VERSION" fmt --manifest-path "$root/crates/$crate/Cargo.toml" --check
    cargo "+$IOS_RUST_VERSION" clippy --manifest-path "$root/crates/$crate/Cargo.toml" --all-targets --locked -- -D warnings
    cargo "+$IOS_RUST_VERSION" test --manifest-path "$root/crates/$crate/Cargo.toml" --locked
done
swift test --package-path "$ios_root/Packages/FilebeamDomain"
swift test --package-path "$ios_root/Packages/FilebeamTheme"
# Compiler and test failures are repeated as workflow annotations so a failed check is
# readable from the pull request without opening the full log.
annotate_failures() {
    grep -E "^/.+:[0-9]+:[0-9]+: error: " "$1" | sort -u | head -20 | sed -E "s|^$root/||; s|^([^:]+):([0-9]+):[0-9]+: error: (.*)$|::error file=\\1,line=\\2::\\3|"
    grep -E "error: |\\*\\* (BUILD|TEST) FAILED|Test Case .* failed" "$1" | grep -vE "^/.+:[0-9]+:[0-9]+: error: " | sort -u | head -10 | sed "s/^/::error ::/"
}
checked() {
    local log status
    log=$(mktemp)
    set +e
    "$@" 2>&1 | tee "$log"
    status=${PIPESTATUS[0]}
    set -e
    [[ $status -eq 0 ]] || annotate_failures "$log"
    rm -f "$log"
    return "$status"
}
checked bash "$root/scripts/ios/build.sh" Debug simulator
checked bash "$root/scripts/ios/test.sh"
checked bash "$root/scripts/ios/build.sh" Release device
