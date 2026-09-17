#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"

output=${1:?output path is required}
commit=${IOS_RELEASE_COMMIT:?IOS_RELEASE_COMMIT is required}
mkdir -p "$(dirname -- "$output")"
printf '{"commit":"%s","rust":"%s","xcode":"%s","xcodegen":"%s","sdk":"%s","architecture":"%s"}\n' \
    "$commit" "$IOS_RUST_VERSION" "$IOS_XCODE_VERSION" "$IOS_XCODEGEN_VERSION" \
    "$(xcrun --sdk iphoneos --show-sdk-version)" "$(uname -m)" > "$output"
