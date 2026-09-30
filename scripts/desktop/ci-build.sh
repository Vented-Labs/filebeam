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

if [[ ${FILEBEAM_DESKTOP_BUILD_ONLY:-0} == 1 ]]; then
    : # The macOS x86_64 cross-target is intentionally build-only.
else
    cargo fmt --manifest-path "$manifest" --check
    for crate in client-config client-core client-updater; do
        cargo test --manifest-path "$root/crates/$crate/Cargo.toml" --locked
    done
    cargo test --manifest-path "$manifest" --locked
    cargo test --manifest-path "$manifest" --locked --features visual-test
    cargo clippy --manifest-path "$manifest" --all-targets --locked -- -D warnings
fi

cargo build --manifest-path "$manifest" --locked --release --target "$target"

target_dir=${CARGO_TARGET_DIR:-"$root/desktop/target"}
binary_name=filebeam
[[ $target == *-windows-* ]] && binary_name+=.exe
binary="$target_dir/$target/release/$binary_name"
[[ -f $binary ]] || { printf 'Missing native binary: %s\n' "$binary" >&2; exit 1; }
mkdir -p "$output"
install -m 0755 "$binary" "$output/filebeam${FILEBEAM_BINARY_SUFFIX:-}"
