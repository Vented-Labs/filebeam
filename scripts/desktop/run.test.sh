#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-desktop-run.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin"

cat >"$tmp/bin/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%q\n' "$@" >> "$MOCK_DOCKER_LOG"
if [[ $1 == image && $2 == inspect ]]; then
    [[ ${MOCK_IMAGE_EXISTS:-1} == 1 ]]
fi
EOF
cat >"$tmp/bin/flock" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'flock=%q\n' "$1" >> "$MOCK_DOCKER_LOG"
EOF
chmod +x "$tmp/bin/docker" "$tmp/bin/flock"

run_wrapper() {
    : >"$tmp/docker.log"
    PATH="$tmp/bin:$PATH" MOCK_DOCKER_LOG="$tmp/docker.log" MOCK_IMAGE_EXISTS=1 \
        FILEBEAM_DESKTOP_CACHE_DIR="$tmp/cache dir" \
        FILEBEAM_RELEASE_VERSION=1.2.3 \
        FILEBEAM_RELEASE_TAG='v1.2.3 candidate' \
        FILEBEAM_RELEASE_SHA='abc def' \
        FILEBEAM_RELEASE_PUBLIC_KEY='public key with spaces' \
        FILEBEAM_RELEASE_SIGNING_KEY=must-not-cross-boundary \
        CARGO_BUILD_JOBS=99 \
        "$@"
}

run_wrapper bash "$root/scripts/desktop/build.sh" --release --features 'native test'
grep -Fq 'flock=' "$tmp/docker.log"
! grep -Fxq 'build' "$tmp/docker.log"
grep -Fxq -- 'run' "$tmp/docker.log"
grep -Fxq -- '--memory' "$tmp/docker.log"
grep -Fxq -- '4g' "$tmp/docker.log"
grep -Fxq -- '--memory-swap' "$tmp/docker.log"
grep -Fxq -- '--cpus' "$tmp/docker.log"
grep -Fxq -- '2' "$tmp/docker.log"
grep -Fxq -- 'CARGO_BUILD_JOBS=1' "$tmp/docker.log"
grep -Fxq -- 'CARGO_TARGET_DIR=/target' "$tmp/docker.log"
grep -Fxq -- "$root:/workspace" "$tmp/docker.log"
grep -Fxq -- "$tmp/cache\\ dir/cargo:/cargo" "$tmp/docker.log"
grep -Fxq -- "$tmp/cache\\ dir/target:/target" "$tmp/docker.log"
grep -Fxq -- '/workspace/desktop/Cargo.toml' "$tmp/docker.log"
grep -Fxq -- '--release' "$tmp/docker.log"
grep -Fxq -- 'native\ test' "$tmp/docker.log"
grep -Fxq -- 'FILEBEAM_RELEASE_TAG=v1.2.3\ candidate' "$tmp/docker.log"
grep -Fxq -- 'FILEBEAM_RELEASE_PUBLIC_KEY=public\ key\ with\ spaces' "$tmp/docker.log"
! grep -Fq 'FILEBEAM_RELEASE_SIGNING_KEY' "$tmp/docker.log"
! grep -Fq "$root/desktop/Cargo.toml" "$tmp/docker.log"

for wrapper in check.sh dev.sh; do
    run_wrapper bash "$root/scripts/desktop/$wrapper"
    grep -Fxq -- '/workspace/desktop/Cargo.toml' "$tmp/docker.log"
done

: >"$tmp/docker.log"
PATH="$tmp/bin:$PATH" MOCK_DOCKER_LOG="$tmp/docker.log" MOCK_IMAGE_EXISTS=0 \
    FILEBEAM_DESKTOP_CACHE_DIR="$tmp/cache-build" "$root/scripts/desktop/run.sh" true
grep -Fxq -- 'build' "$tmp/docker.log"

: >"$tmp/docker.log"
PATH="$tmp/bin:$PATH" MOCK_DOCKER_LOG="$tmp/docker.log" MOCK_IMAGE_EXISTS=1 \
    FILEBEAM_DESKTOP_CACHE_DIR="$tmp/cache-rebuild" FILEBEAM_DESKTOP_BUILD_IMAGE=true \
    "$root/scripts/desktop/run.sh" true
grep -Fxq -- 'build' "$tmp/docker.log"

bash "$root/scripts/desktop/ci-build.test.sh"
