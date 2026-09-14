#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
output=${1:?generated output directory is required}
abis=${2:-arm64-v8a,armeabi-v7a,x86_64}
profile=${3:-release}
[[ $profile == debug || $profile == release ]] || { printf '%s\n' 'rustProfile must be debug or release' >&2; exit 1; }
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-${ANDROID_HOME:?Set ANDROID_HOME}/ndk/28.2.13676358}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/client-ffi/target}
command -v cargo-ndk >/dev/null || { printf '%s\n' 'Install cargo-ndk 4.1.2 (see mobile/android/README.md)' >&2; exit 1; }

targets=()
IFS=',' read -ra selected <<< "$abis"
for abi in "${selected[@]}"; do
    case "$abi" in
        arm64-v8a|armeabi-v7a|x86_64) targets+=(-t "$abi") ;;
        *) printf 'Unsupported Android ABI: %s\n' "$abi" >&2; exit 1 ;;
    esac
done
flags=()
[[ $profile != release ]] || flags+=(--release)
# Both ELF segment alignment and APK ZIP alignment are checked by check-native.py.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384"
(
    # cargo-ndk 4.x discovers its workspace from the current directory even when
    # --manifest-path is supplied. Keep this independent of the Gradle working dir.
    cd "$root/client-ffi"
    cargo ndk "${targets[@]}" -P 26 -o "$output/jniLibs" build --lib --locked "${flags[@]}"
)

# Binding generation runs on the build host; it reads metadata from an Android ELF.
unset RUSTFLAGS
(
    cd "$root/client-ffi"
    # UniFFI discovers the crate's uniffi.toml through Cargo metadata.
    cargo run --locked --features bindgen --bin uniffi-bindgen -- \
        generate --library "$output/jniLibs/${selected[0]}/libfilebeam_client_ffi.so" \
        --language kotlin --out-dir "$output/kotlin" --no-format
)
