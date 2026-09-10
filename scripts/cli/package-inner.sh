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
mkdir -p "$output_dir"

for architecture in x86_64 aarch64; do
    scripts/cli/package-target.sh "$tag" linux "$architecture" "$output_dir"
done

(cd "$output_dir" && sha256sum "$tag"-linux-*.tar.gz > checksums.txt)
printf '%s\n' "$version" > "$output_dir/version"
printf '%s\n' "$BEAM_RELEASE_PUBLIC_KEY" > "$output_dir/public-key"
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$BEAM_RELEASE_PUBLIC_KEY|g" scripts/cli/install.sh > "$output_dir/install.sh"
sed "s|__BEAM_RELEASE_PUBLIC_KEY__|$BEAM_RELEASE_PUBLIC_KEY|g" scripts/cli/install.ps1 > "$output_dir/install.ps1"
