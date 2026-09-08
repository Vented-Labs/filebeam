#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-github-release.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

tag=v1.2.3
commit=0123456789abcdef0123456789abcdef01234567
release_dir="$tmp/release"
bin="$tmp/bin"
mkdir -p "$release_dir" "$bin"
printf 'verified release asset\n' > "$release_dir/filebeam-$tag.zip"
sha=$(sha256sum "$release_dir/filebeam-$tag.zip" | cut -d' ' -f1)
size=$(stat --format=%s "$release_dir/filebeam-$tag.zip")
printf '{"tag":"%s","commit":"%s","package":{"path":"versions/%s/filebeam-%s.zip","sha256":"%s","size":%s}}\n' "$tag" "$commit" "$tag" "$tag" "$sha" "$size" > "$release_dir/release.json"

cat > "$bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$MOCK_LOG"
case "$1 $2" in
  'api repos/Vented-Labs/filebeam/git/ref/tags/v1.2.3')
    [[ ${MOCK_REF_FAIL:-false} == true ]] && { printf 'gh: Not Found (HTTP 404)\n' >&2; exit 1; }
    count_file=$MOCK_API_COUNT
    count=0
    [[ -f $count_file ]] && count=$(<"$count_file")
    count=$((count + 1))
    printf '%s' "$count" > "$count_file"
    resolved=$MOCK_COMMIT
    [[ -n ${MOCK_RETAG_AFTER:-} && $count -gt $MOCK_RETAG_AFTER ]] && resolved=$MOCK_RETAG_COMMIT
    [[ ${MOCK_ANNOTATED:-false} == true ]] && printf 'tag\taaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n' || printf 'commit\t%s\n' "$resolved"
    ;;
  'api repos/Vented-Labs/filebeam/git/tags/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')
    printf 'commit\t%s\n' "$MOCK_COMMIT"
    ;;
  'release view')
    if [[ $MOCK_STATE == missing ]]; then printf 'release not found\n' >&2; exit 1; fi
    if [[ ${MOCK_VIEW_ERROR:-false} == true ]]; then printf 'simulated release API failure\n' >&2; exit 1; fi
    [[ $MOCK_STATE == draft ]] && printf '{"isDraft":true}\n' || printf '{"isDraft":false}\n'
    ;;
  'release create') ;;
  'release upload')
    [[ ${MOCK_FAIL_UPLOAD:-false} == true ]] && exit 1
    :
    ;;
  'release download')
    for ((i=1; i <= $#; i++)); do
      if [[ ${!i} == --pattern ]]; then pattern_index=$((i + 1)); pattern=${!pattern_index}; fi
      if [[ ${!i} == --dir ]]; then dir_index=$((i + 1)); dir=${!dir_index}; fi
    done
    mkdir -p "$dir"
    case $pattern in
      filebeam-v1.2.3.zip) cp "$MOCK_RELEASE_ASSET" "$dir/$pattern" ;;
      release.json) cp "$MOCK_RELEASE_METADATA" "$dir/$pattern" ;;
      *) exit 64 ;;
    esac
    ;;
  'release edit') ;;
  *) exit 64 ;;
esac
EOF
chmod +x "$bin/gh"

run_case() {
    local state=$1 annotated=${2:-false}
    : > "$tmp/log"
    : > "$tmp/api-count"
    MOCK_STATE=$state MOCK_ANNOTATED=$annotated MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_API_COUNT="$tmp/api-count" MOCK_RELEASE_ASSET="$release_dir/filebeam-$tag.zip" MOCK_RELEASE_METADATA="$release_dir/release.json" GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$release_dir"
}

release_calls() {
    mapfile -t calls < <(while read -r command subcommand _; do [[ $command == release ]] && printf '%s %s\n' "$command" "$subcommand"; done < "$tmp/log")
}

# Download directories are random; validate ordered command types separately.
run_case missing
release_calls
[[ ${calls[*]} == 'release view release create release upload release download release edit' ]]
[[ $(<"$tmp/log") == *'api repos/Vented-Labs/filebeam/git/ref/tags/v1.2.3'* ]]
run_case draft true
release_calls
[[ ${calls[*]} == 'release view release upload release download release edit' ]]
[[ $(<"$tmp/log") == *'api repos/Vented-Labs/filebeam/git/tags/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'* ]]
run_case published
release_calls
[[ ${calls[*]} == 'release view release download release download' ]]

printf 'different metadata\n' > "$tmp/different-release.json"
: > "$tmp/log"
if MOCK_STATE=published MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_API_COUNT="$tmp/api-count" MOCK_RELEASE_ASSET="$release_dir/filebeam-$tag.zip" MOCK_RELEASE_METADATA="$tmp/different-release.json" GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$release_dir"; then
    printf 'Expected mismatched public metadata to fail.\n' >&2
    exit 1
fi
[[ $(<"$tmp/log") != *'release upload'* ]]
[[ $(<"$tmp/log") != *'release edit'* ]]

: > "$tmp/log"
if MOCK_STATE=draft MOCK_FAIL_UPLOAD=true MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_API_COUNT="$tmp/api-count" MOCK_RELEASE_ASSET="$release_dir/filebeam-$tag.zip" MOCK_RELEASE_METADATA="$release_dir/release.json" GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$release_dir"; then
    printf 'Expected failed draft upload to fail.\n' >&2
    exit 1
fi
[[ $(<"$tmp/log") != *'release edit'* ]]

: > "$tmp/log"
if MOCK_STATE=draft MOCK_RETAG_AFTER=1 MOCK_RETAG_COMMIT=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_API_COUNT="$tmp/api-count" MOCK_RELEASE_ASSET="$release_dir/filebeam-$tag.zip" MOCK_RELEASE_METADATA="$release_dir/release.json" GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$release_dir"; then
    printf 'Expected changed remote tag to fail.\n' >&2
    exit 1
fi
[[ $(<"$tmp/log") != *'release edit'* ]]

: > "$tmp/log"
if MOCK_STATE=missing MOCK_REF_FAIL=true MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_API_COUNT="$tmp/api-count" MOCK_RELEASE_ASSET="$release_dir/filebeam-$tag.zip" MOCK_RELEASE_METADATA="$release_dir/release.json" GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$release_dir"; then
    printf 'Expected remote tag lookup failure to fail.\n' >&2
    exit 1
fi
[[ $(<"$tmp/log") != *'release create'* ]]

: > "$tmp/log"
if MOCK_STATE=draft MOCK_VIEW_ERROR=true MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_API_COUNT="$tmp/api-count" MOCK_RELEASE_ASSET="$release_dir/filebeam-$tag.zip" MOCK_RELEASE_METADATA="$release_dir/release.json" GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$release_dir" 2>"$tmp/error"; then
    printf 'Expected release lookup failure to fail.\n' >&2
    exit 1
fi
[[ $(<"$tmp/error") == *'simulated release API failure'* ]]
[[ $(<"$tmp/log") != *'release create'* ]]
