#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 2 ]] || { printf 'Usage: %s TARGET OUTPUT_DIRECTORY\n' "$0" >&2; exit 64; }
target=$1
output=$2
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
manifest="$root/desktop/Cargo.toml"

for variable in FILEBEAM_RELEASE_VERSION FILEBEAM_RELEASE_TAG FILEBEAM_RELEASE_SHA; do
    [[ -n ${!variable:-} ]] || { printf '%s is required\n' "$variable" >&2; exit 1; }
done
FILEBEAM_RELEASE_VERSION=${FILEBEAM_RELEASE_VERSION#v}
export FILEBEAM_RELEASE_VERSION
if [[ -n ${FILEBEAM_RELEASE_PUBLIC_KEY:-} ]]; then
    FILEBEAM_RELEASE_PUBLIC_KEY=$(python3 -c 'import base64, os; key = base64.b64decode("".join(os.environ["FILEBEAM_RELEASE_PUBLIC_KEY"].split()) + "==="); assert len(key) == 32, "release public key must be 32 bytes"; print(base64.b64encode(key).decode())')
    export FILEBEAM_RELEASE_PUBLIC_KEY
fi

if [[ ${FILEBEAM_DESKTOP_BUILD_ONLY:-0} == 1 ]]; then
    : # The macOS x86_64 cross-target is intentionally build-only.
else
    cargo fmt --manifest-path "$manifest" --check
    cargo test --manifest-path "$manifest" --locked --features visual-test \
        -p filebeam-client-config -p filebeam-client-core -p filebeam-client-updater -p filebeam-desktop
    cargo clippy --manifest-path "$manifest" --all-targets --locked -- -D warnings
fi

target_dir=${CARGO_TARGET_DIR:-"$root/desktop/target"}
host=$(rustc -vV | sed -n 's/^host: //p')
build_args=()
binary_dir="$target_dir/release"
if [[ $target != "$host" ]]; then
    build_args+=(--target "$target")
    binary_dir="$target_dir/$target/release"
fi
cargo build --manifest-path "$manifest" --locked --release "${build_args[@]}"

binary_name=filebeam
[[ $target == *-windows-* ]] && binary_name+=.exe
binary="$binary_dir/$binary_name"
[[ -f $binary ]] || { printf 'Missing native binary: %s\n' "$binary" >&2; exit 1; }
mkdir -p "$output"
install -m 0755 "$binary" "$output/filebeam${FILEBEAM_BINARY_SUFFIX:-}"
