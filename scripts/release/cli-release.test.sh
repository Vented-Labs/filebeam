#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/beam-release-test.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
git -C "$temporary" init --quiet
printf 'fixture\n' > "$temporary/source"
git -C "$temporary" add source
git -C "$temporary" -c user.name=Fixture -c user.email=fixture@example.test commit --quiet -m fixture
commit=$(git -C "$temporary" rev-parse HEAD)
git -C "$temporary" update-ref refs/remotes/origin/master "$commit"
git -C "$temporary" tag v0.2.0
git -C "$temporary" tag beam-v0.2.0
[[ $(cd "$temporary" && bash "$root/scripts/release/cli-validate-ref.sh" beam-v0.2.0) == "$commit" ]]

tag=beam-v0.2.0
for target in linux-x86_64.tar.gz linux-aarch64.tar.gz macos-x86_64.tar.gz macos-aarch64.tar.gz windows-x86_64.zip; do printf '%s\n' "$target" > "$temporary/$tag-$target"; done
(cd "$temporary" && sha256sum "$tag"-*.tar.gz "$tag"-*.zip > checksums.txt)
printf '0.2.0\n' > "$temporary/version"
printf 'key\n' > "$temporary/public-key"
printf 'installer\n' > "$temporary/install.sh"
printf 'installer\n' > "$temporary/install.ps1"
SOURCE_DATE_EPOCH=1700000000 php "$root/scripts/release/cli-write-release.php" "$tag" "$temporary" "$temporary/release.json"
mkdir "$temporary/bin" "$temporary/source-assets"
cp "$temporary"/beam-* "$temporary"/{checksums.txt,version,public-key,install.sh,install.ps1,release.json} "$temporary/source-assets/"
cat > "$temporary/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$MOCK_LOG"
case "$1 $2" in
  'api repos/{owner}/{repo}/commits/beam-v0.2.0') printf '%s\n' "$MOCK_COMMIT" ;;
  'release view') [[ $MOCK_STATE != missing ]] || { printf 'release not found\n' >&2; exit 1; }; [[ $MOCK_STATE == draft ]] && printf '{"isDraft":true}\n' || printf '{"isDraft":false}\n' ;;
  'release create'|'release upload'|'release edit') : ;;
  'release download')
    for ((i = 1; i <= $#; i++)); do [[ ${!i} == --pattern ]] && { j=$((i + 1)); pattern=${!j}; }; [[ ${!i} == --dir ]] && { j=$((i + 1)); dir=${!j}; }; done
    mkdir -p "$dir"; cp "$MOCK_SOURCE/$pattern" "$dir/$pattern"
    ;;
  *) exit 64 ;;
esac
EOF
chmod +x "$temporary/bin/gh"
: > "$temporary/gh.log"
(
  cd "$temporary"
  MOCK_STATE=missing MOCK_LOG="$temporary/gh.log" MOCK_COMMIT=$commit MOCK_SOURCE="$temporary/source-assets" PATH="$temporary/bin:$PATH" bash "$root/scripts/release/cli-github-release.sh" "$tag" "$commit" "$temporary"
)
order=$(awk '$1 == "release" { printf "%s %s ", $1, $2 }' "$temporary/gh.log")
[[ $order == "release view release create release upload "*"release edit " ]]
[[ $(grep -o 'release download' <<<"$order" | wc -l) -eq 11 ]]
[[ $(grep -o 'release upload' <<<"$order" | wc -l) -eq 1 && $(grep -o 'release edit' <<<"$order" | wc -l) -eq 1 ]]
printf 'Standalone CLI draft creation, complete upload, verification, and single publish passed.\n'
bash "$root/scripts/release/cli-key.test.sh"
