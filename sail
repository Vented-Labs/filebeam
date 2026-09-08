#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT/backend"

if [ ! -f vendor/bin/sail ]; then
    printf '%s\n' 'Install backend Composer dependencies first; see README.md.' >&2
    exit 1
fi

export APP_PORT="${APP_PORT:-8000}"
exec bash vendor/bin/sail "$@"
