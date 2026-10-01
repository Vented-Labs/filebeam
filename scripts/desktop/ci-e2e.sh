#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 1 ]] || { printf 'Usage: %s OUTPUT_DIRECTORY\n' "$0" >&2; exit 64; }
out=$1
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
cargo build --manifest-path "$root/desktop/Cargo.toml" --locked --features headless-acceptance --example native_acceptance
cargo build --manifest-path "$root/crates/client-core/Cargo.toml" --locked --example services_acceptance
BEAM_RELEASE_VERSION=${FILEBEAM_RELEASE_VERSION#v} cargo build --manifest-path "$root/crates/cli/Cargo.toml" --locked --release --bin beam
mkdir -p "$out"
target=${CARGO_TARGET_DIR:?CARGO_TARGET_DIR must be set by the bounded runner}
install -m 0755 "$target/debug/examples/native_acceptance" "$out/native_acceptance"
install -m 0755 "$target/debug/examples/services_acceptance" "$out/services_acceptance"
install -m 0755 "$target/release/beam" "$out/beam"
