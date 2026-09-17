#!/usr/bin/env bash
# Linux-only binding shape check. It does not claim to validate Apple linking or Xcode builds.
set -euo pipefail

"$(dirname -- "$0")/typecheck-native.sh"
