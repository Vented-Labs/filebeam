#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-desktop-ci-build.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/target" "$tmp/output"

cat >"$tmp/bin/rustc" <<'EOF'
#!/usr/bin/env bash
printf 'host: %s\n' "${MOCK_RUST_HOST:-x86_64-unknown-linux-gnu}"
EOF
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
binary_dir="$CARGO_TARGET_DIR/release"
if [[ -n $target ]]; then binary_dir="$CARGO_TARGET_DIR/$target/release"; fi
name=filebeam
[[ ${target:-${MOCK_RUST_HOST:-}} == *-windows-* ]] && name+=.exe
mkdir -p "$binary_dir"
: >"$binary_dir/$name"
EOF
chmod +x "$tmp/bin/cargo" "$tmp/bin/rustc"

PATH="$tmp/bin:$PATH" CARGO_TARGET_DIR="$tmp/target" FILEBEAM_DESKTOP_BUILD_ONLY=1 \
    FILEBEAM_RELEASE_VERSION=1.2.3 FILEBEAM_RELEASE_TAG=v1.2.3 FILEBEAM_RELEASE_SHA=abcdef0 \
    FILEBEAM_BINARY_SUFFIX=.exe bash "$root/scripts/desktop/ci-build.sh" \
    x86_64-pc-windows-msvc "$tmp/output"
test -f "$tmp/output/filebeam.exe"

PATH="$tmp/bin:$PATH" CARGO_TARGET_DIR="$tmp/native-target" FILEBEAM_DESKTOP_BUILD_ONLY=1 \
    FILEBEAM_RELEASE_VERSION=1.2.3 FILEBEAM_RELEASE_TAG=v1.2.3 FILEBEAM_RELEASE_SHA=abcdef0 \
    bash "$root/scripts/desktop/ci-build.sh" x86_64-unknown-linux-gnu "$tmp/native-output"
test -f "$tmp/native-output/filebeam"
test -f "$tmp/native-target/release/filebeam"
