#!/usr/bin/env bash
set -euo pipefail
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
exec "$root/scripts/desktop/run.sh" python3 /workspace/scripts/desktop/transfer.test.py
