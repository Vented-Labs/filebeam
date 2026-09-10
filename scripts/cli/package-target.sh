#!/usr/bin/env bash
set -euo pipefail

usage() { printf 'Usage: %s beam-vX.Y.Z OS ARCHITECTURE OUTPUT_DIR\n' "$0" >&2; exit 64; }
[[ $# -eq 4 ]] || usage
tag=$1
os=$2
architecture=$3
output_dir=$4
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || usage
version=${tag#beam-v}
case "$os-$architecture" in
    linux-x86_64) target=x86_64-unknown-linux-gnu; executable=beam; format=tar.gz ;;
    linux-aarch64) target=aarch64-unknown-linux-gnu; executable=beam; format=tar.gz ;;
    macos-x86_64) target=x86_64-apple-darwin; executable=beam; format=tar.gz ;;
    macos-aarch64) target=aarch64-apple-darwin; executable=beam; format=tar.gz ;;
    windows-x86_64) target=x86_64-pc-windows-msvc; executable=beam.exe; format=zip ;;
    *) usage ;;
esac
[[ -n ${BEAM_RELEASE_PUBLIC_KEY:-} ]] || { printf '%s\n' 'BEAM_RELEASE_PUBLIC_KEY is required.' >&2; exit 1; }
export BEAM_RELEASE_VERSION="$version"
source_date_epoch=$(git show -s --format=%ct HEAD)
mkdir -p "$output_dir"
cargo build --manifest-path cli/Cargo.toml --release --locked --target "$target"
binary="cli/target/$target/release/$executable"
[[ -f $binary ]] || { printf 'Expected executable %s.\n' "$binary" >&2; exit 1; }
stage=$(mktemp -d "${TMPDIR:-/tmp}/beam-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/beam"
cp "$binary" "$stage/beam/$executable"
cp LICENSE "$stage/beam/"
printf '%s\n' "$version" > "$stage/beam/version"
archive="$output_dir/$tag-$os-$architecture.$format"
python3 scripts/cli/archive.py "$format" "$stage" "$archive" "$source_date_epoch"
