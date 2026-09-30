#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-desktop-ci-build.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/target" "$tmp/output"

cat >"$tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == build ]]
target=
while [[ $# -gt 0 ]]; do
    case $1 in
        --target) target=$2; shift 2 ;;
        *) shift ;;
    esac
done
[[ -n $target ]]
name=filebeam
[[ $target == *-windows-* ]] && name+=.exe
mkdir -p "$CARGO_TARGET_DIR/$target/release"
: >"$CARGO_TARGET_DIR/$target/release/$name"
EOF
chmod +x "$tmp/bin/cargo"

PATH="$tmp/bin:$PATH" CARGO_TARGET_DIR="$tmp/target" FILEBEAM_DESKTOP_BUILD_ONLY=1 \
    FILEBEAM_RELEASE_VERSION=1.2.3 FILEBEAM_RELEASE_TAG=v1.2.3 FILEBEAM_RELEASE_SHA=abcdef0 \
    FILEBEAM_BINARY_SUFFIX=.exe bash "$root/scripts/desktop/ci-build.sh" \
    x86_64-pc-windows-msvc "$tmp/output"
test -f "$tmp/output/filebeam.exe"
