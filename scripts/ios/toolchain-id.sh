#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
printf 'IOS_TOOLCHAIN_ID=%s\n' "$(toolchain_identity | shasum -a 256 | awk '{print $1}')"
