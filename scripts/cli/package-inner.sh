#!/usr/bin/env bash
set -euo pipefail

tag=${1:?tag is required}
output_dir=${2:?output directory is required}
[[ $tag =~ ^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || { printf 'Tag must be beam-vX.Y.Z.\n' >&2; exit 64; }
version=${tag#beam-v}
canonical_public_key=$(php scripts/release/cli-public-key.php public)
[[ "$canonical_public_key" == "${BEAM_RELEASE_PUBLIC_KEY:-}" ]] || {
    printf '%s\n' 'BEAM_RELEASE_PUBLIC_KEY must be canonical Base64 of a 32-byte Ed25519 public key.' >&2
    exit 1
}
export BEAM_RELEASE_VERSION="$version"
source_date_epoch=$(git show -s --format=%ct HEAD)
mkdir -p "$output_dir"

for architecture in x86_64 aarch64; do
    case "$architecture" in
        x86_64) target=x86_64-unknown-linux-gnu ;;
        aarch64) target=aarch64-unknown-linux-gnu ;;
    esac
    cargo build --manifest-path cli/Cargo.toml --release --locked --target "$target"
    binary="cli/target/$target/release/beam"
    [[ -x $binary ]] || { printf 'Expected executable %s.\n' "$binary" >&2; exit 1; }
    stage=$(mktemp -d)
    trap 'rm -rf "$stage"' EXIT
    mkdir -p "$stage/beam"
    cp "$binary" "$stage/beam/beam"
    cp scripts/cli/install.sh LICENSE "$stage/beam/"
    sed -i "s|__BEAM_RELEASE_PUBLIC_KEY__|$BEAM_RELEASE_PUBLIC_KEY|g" "$stage/beam/install.sh"
    printf '%s\n' "$version" > "$stage/beam/version"
    (cd "$stage/beam" && sha256sum beam install.sh LICENSE version > checksums.txt)
    archive="$output_dir/$tag-linux-$architecture.tar.gz"
    [[ ! -e $archive ]] || { printf 'Refusing to overwrite %s.\n' "$archive" >&2; exit 1; }
    tar --sort=name --mtime="@$source_date_epoch" --owner=0 --group=0 --numeric-owner -C "$stage" -czf "$archive" beam
    rm -rf "$stage"
    trap - EXIT
done

(cd "$output_dir" && sha256sum "$tag"-linux-*.tar.gz > checksums.txt)
printf '%s\n' "$version" > "$output_dir/version"
printf '%s\n' "$BEAM_RELEASE_PUBLIC_KEY" > "$output_dir/public-key"
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$BEAM_RELEASE_PUBLIC_KEY|g" scripts/cli/install.sh > "$output_dir/install.sh"
