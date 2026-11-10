#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/filebeam-tested-commit.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

commit=0123456789abcdef0123456789abcdef01234567
other=fedcba9876543210fedcba9876543210fedcba98
mkdir "$tmp/bin"
cat > "$tmp/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "$GH_ARGS"
[[ $FAKE_RESPONSE != fail ]] || exit 1
jq -r "${4:?}" <<<"$FAKE_RESPONSE"
EOF
chmod +x "$tmp/bin/gh"
export PATH="$tmp/bin:$PATH" GITHUB_REPOSITORY=Vented-Labs/filebeam GH_ARGS="$tmp/args"

check() {
    local expected=$1 response=$2 actual
    actual=$(FAKE_RESPONSE=$response bash "$root/scripts/release/tested-commit.sh" "$commit")
    if [[ $actual != "$expected" ]]; then
        printf 'Expected %q, got %q for %s\n' "$expected" "$actual" "$response" >&2
        exit 1
    fi
}

url=https://github.com/Vented-Labs/filebeam/actions/runs/1
check "tested-by=$url" "{\"workflow_runs\":[{\"head_sha\":\"$commit\",\"conclusion\":\"success\",\"html_url\":\"$url\"}]}"
grep -Fq "repos/Vented-Labs/filebeam/actions/workflows/tests.yml/runs?head_sha=$commit&branch=master&event=push&status=success" "$tmp/args"
check 'tested-by=' '{"workflow_runs":[]}'
check 'tested-by=' "{\"workflow_runs\":[{\"head_sha\":\"$other\",\"conclusion\":\"success\",\"html_url\":\"$url\"}]}"
check 'tested-by=' "{\"workflow_runs\":[{\"head_sha\":\"$commit\",\"conclusion\":\"failure\",\"html_url\":\"$url\"}]}"
check 'tested-by=' fail

if bash "$root/scripts/release/tested-commit.sh" v1.2.3 >/dev/null 2>&1; then
    printf 'Expected a tag name to be rejected.\n' >&2
    exit 1
fi
printf 'Tested commit lookup checks passed.\n'
