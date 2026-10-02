#!/usr/bin/env bash
set -euo pipefail
root=$(git rev-parse --show-toplevel)
mkdir -p "$root/.filebeam"
state=$(mktemp -d "$root/.filebeam/theme-browser.XXXXXX")
port=${THEME_BROWSER_PORT:-$(node -e 'const server = require("node:net").createServer(); server.listen(0, "127.0.0.1", () => { process.stdout.write(String(server.address().port)); server.close(); });')}
version=$(node -p 'require(process.argv[1]).version' "$root/node_modules/playwright/package.json")
container=''
cleanup() {
    if [[ -n "$container" ]]; then
        docker logs "$container" > "$state/browser.log" 2>&1 || true
        docker stop "$container" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# Match the browser and native font fallback environment used by theme screenshot CI.
container=$(docker run --detach --rm --network host --ipc host \
    --mount "type=bind,source=$root/node_modules,target=/node_modules,readonly" \
    "mcr.microsoft.com/playwright:v${version}-noble" \
    node /node_modules/playwright/cli.js run-server --host 127.0.0.1 --port "$port")
for _ in {1..60}; do
    if curl --fail --silent --max-time 1 "http://127.0.0.1:$port/" >/dev/null; then break; fi
    sleep .5
done
curl --fail --silent --show-error --max-time 5 "http://127.0.0.1:$port/" >/dev/null
THEME_BROWSER_WS="ws://127.0.0.1:$port/" bash "$root/scripts/themes/test.sh" "$@"
