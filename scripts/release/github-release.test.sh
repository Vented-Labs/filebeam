#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-github-release.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
tag=v1.2.3
commit=0123456789abcdef0123456789abcdef01234567
app="$tmp/app"
cli="$tmp/cli"
bin="$tmp/bin"
mkdir -p "$app" "$cli" "$bin"
printf 'verified application asset\n' > "$app/filebeam-$tag.zip"
sha=$(sha256sum "$app/filebeam-$tag.zip" | cut -d' ' -f1)
size=$(stat --format=%s "$app/filebeam-$tag.zip")
printf '{"tag":"%s","commit":"%s","package":{"path":"versions/%s/filebeam-%s.zip","sha256":"%s","size":%s}}\n' "$tag" "$commit" "$tag" "$tag" "$sha" "$size" > "$app/release.json"
cli_tag="beam-$tag"
for target in linux-x86_64.tar.gz linux-aarch64.tar.gz macos-x86_64.tar.gz macos-aarch64.tar.gz windows-x86_64.zip; do printf '%s\n' "$target" > "$cli/$cli_tag-$target"; done
(cd "$cli" && sha256sum "$cli_tag"-*.tar.gz "$cli_tag"-*.zip > checksums.txt)
printf '%s\n' "${tag#v}" > "$cli/version"
printf 'public key\n' > "$cli/public-key"
printf 'installer\n' > "$cli/install.sh"
printf 'installer\n' > "$cli/install.ps1"
SOURCE_DATE_EPOCH=1700000000 php "$root/scripts/release/cli-write-release.php" "$cli_tag" "$cli" "$cli/release.json"

cat > "$bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$MOCK_LOG"
case "$1 $2" in
  'api repos/Vented-Labs/filebeam/git/ref/tags/v1.2.3')
    count=0; [[ -f $MOCK_TAG_COUNT ]] && count=$(<"$MOCK_TAG_COUNT")
    count=$((count + 1)); printf '%s' "$count" > "$MOCK_TAG_COUNT"
    resolved=$MOCK_COMMIT
    [[ ${MOCK_RETAG_AFTER:-99} -lt $count ]] && resolved=$MOCK_RETAG_COMMIT
    printf 'commit\t%s\n' "$resolved"
    ;;
  'release view')
    [[ ${MOCK_STATE:-missing} != missing ]] || { printf 'release not found\n' >&2; exit 1; }
    [[ ${MOCK_STATE:-missing} == draft ]] && printf '{"isDraft":true}\n' || printf '{"isDraft":false}\n'
    ;;
  'release create') : ;;
  'release upload') [[ ${MOCK_FAIL_UPLOAD:-false} != true ]] ;;
  'release download')
    for ((i = 1; i <= $#; i++)); do
      [[ ${!i} == --pattern ]] && { j=$((i + 1)); pattern=${!j}; }
      [[ ${!i} == --dir ]] && { j=$((i + 1)); dir=${!j}; }
    done
    mkdir -p "$dir"
    [[ ${MOCK_MISMATCH:-} != "$pattern" ]] || { printf 'wrong bytes\n' > "$dir/$pattern"; exit 0; }
    [[ ${MOCK_MISSING:-} != "$pattern" ]] || { printf 'asset not found\n' >&2; exit 1; }
    cp "$MOCK_SOURCE/$pattern" "$dir/$pattern"
    ;;
  'release edit') : ;;
  *) exit 64 ;;
esac
EOF
chmod +x "$bin/gh"
source="$tmp/source"
mkdir "$source"
cp "$app"/* "$source/"
for asset in "$cli"/*; do
    name=${asset##*/}
    [[ $name == release.json ]] && name="$cli_tag-release.json"
    cp "$asset" "$source/$name"
done

run_case() {
    : > "$tmp/log"
    : > "$tmp/tag-count"
    MOCK_STATE=$1 MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_TAG_COUNT="$tmp/tag-count" MOCK_SOURCE=$source GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$app" "$cli"
}
release_order() { awk '$1 == "release" { printf "%s %s ", $1, $2 }' "$tmp/log"; }

# One draft contains both application and CLI assets, then is verified and published once.
run_case missing
order=$(release_order)
[[ $order == "release view release create release upload "*"release edit " ]]
[[ $(grep -o 'release download' <<<"$order" | wc -l) -eq 13 ]]
[[ $(grep -o 'release upload' <<<"$order" | wc -l) -eq 1 && $(grep -o 'release edit' <<<"$order" | wc -l) -eq 1 ]]

# A failed combined upload leaves the draft unpublished.
: > "$tmp/log"
if MOCK_STATE=draft MOCK_FAIL_UPLOAD=true MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_TAG_COUNT="$tmp/tag-count" MOCK_SOURCE=$source GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$app" "$cli"; then exit 1; fi
[[ $(<"$tmp/log") != *'release edit'* ]]

# Any downloaded-byte mismatch prevents publication.
: > "$tmp/log"
if MOCK_STATE=draft MOCK_MISMATCH=checksums.txt MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_TAG_COUNT="$tmp/tag-count" MOCK_SOURCE=$source GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$app" "$cli"; then exit 1; fi
[[ $(<"$tmp/log") != *'release edit'* ]]

# Published releases are immutable: retries only download and verify, never upload or edit.
: > "$tmp/log"
if MOCK_STATE=published MOCK_MISSING=release.json MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_TAG_COUNT="$tmp/tag-count" MOCK_SOURCE=$source GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$app" "$cli" 2>"$tmp/error"; then exit 1; fi
[[ $(<"$tmp/error") == *'Recovery requires a new release tag'* ]]
[[ $(<"$tmp/log") != *'release upload'* && $(<"$tmp/log") != *'release edit'* && $(<"$tmp/log") != *'release create'* ]]

# A tag move after draft verification prevents the only publish operation.
: > "$tmp/log"; : > "$tmp/tag-count"
if MOCK_STATE=draft MOCK_RETAG_AFTER=1 MOCK_RETAG_COMMIT=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa MOCK_LOG="$tmp/log" MOCK_COMMIT=$commit MOCK_TAG_COUNT="$tmp/tag-count" MOCK_SOURCE=$source GH_REPO=Vented-Labs/filebeam PATH="$bin:$PATH" "$root/scripts/release/github-release.sh" "$tag" "$commit" "$app" "$cli"; then exit 1; fi
[[ $(<"$tmp/log") != *'release edit'* ]]

printf 'Combined GitHub release draft, verification, and immutable retry checks passed.\n'
