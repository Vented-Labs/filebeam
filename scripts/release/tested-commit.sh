#!/usr/bin/env bash
# Prints tested-by=<run URL> when the Tests workflow already passed for COMMIT on master,
# so a release reuses that result instead of running the suite again. Prints an empty
# value, and the release runs the suite, when no such run exists or GitHub is unavailable.
set -euo pipefail

commit=${1:-}
if [[ ! $commit =~ ^[0-9a-f]{40}$ ]]; then
    printf 'Usage: %s COMMIT\n' "$0" >&2
    exit 64
fi
repository=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}

run=$(gh api "repos/$repository/actions/workflows/tests.yml/runs?head_sha=$commit&branch=master&event=push&status=success&per_page=1" \
    --jq ".workflow_runs[] | select(.head_sha == \"$commit\" and .conclusion == \"success\") | .html_url" 2>/dev/null) || run=''
printf 'tested-by=%s\n' "$(head -n 1 <<<"$run")"
