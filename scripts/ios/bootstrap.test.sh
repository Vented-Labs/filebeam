#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/ios/lib.sh
source "$(dirname -- "$0")/lib.sh"

fixture=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-ios-bootstrap.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
tool="$fixture/xcodegen tool/xcodegen"
mkdir -p "$(dirname -- "$tool")"
cat >"$tool" <<'EOF'
#!/usr/bin/env bash
[[ $1 == --version ]]
case ${XCODEGEN_TEST_MODE:?} in
    actual) printf 'Version: 2.41.0\n' ;;
    mismatch) printf 'Version: 2.41.1\n' ;;
    failure) exit 23 ;;
esac
EOF
chmod +x "$tool"

XCODEGEN_TEST_MODE=actual
export XCODEGEN_TEST_MODE
require_xcodegen_version "$tool"

for mode in mismatch failure; do
    XCODEGEN_TEST_MODE=$mode
    if (require_xcodegen_version "$tool") >"$fixture/$mode.log" 2>&1; then
        printf 'expected %s version check to fail\n' "$mode" >&2
        exit 1
    fi
    [[ $(<"$fixture/$mode.log") == *"\"$tool\""* ]] || {
        printf 'missing quoted tool path in %s diagnostic\n' "$mode" >&2
        exit 1
    }
done
