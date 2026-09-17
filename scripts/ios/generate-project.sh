#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"
require_macos; bash "$root/scripts/ios/bootstrap.sh"; require_generated
generate_project
xcodebuild -resolvePackageDependencies -project "$ios_root/Filebeam.xcodeproj" -scheme Filebeam -clonedSourcePackagesDirPath "$ios_root/.swiftpm"
