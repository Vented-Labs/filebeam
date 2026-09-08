#!/usr/bin/env bash
# Regression coverage for log checks that must precede an intentional stop.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=scripts/docker/lifecycle.sh
source "$root/scripts/docker/lifecycle.sh"

container=lifecycle-test
logs='normal runtime output'
stopped=false
exit_code=0

assert_clean_logs() {
    ! printf '%s\n' "$logs" | grep -Ei '"level":"error"|PHP Fatal error|FATAL:|ERROR:'
}

docker() {
    case "$1" in
        stop)
            stopped=true
            # PostgreSQL writes this expected message only while stopping.
            logs='FATAL: the database system is shutting down'
            ;;
        inspect)
            printf '%s\n' "$exit_code"
            ;;
        *)
            printf 'Unexpected docker command: %s\n' "$1" >&2
            return 1
            ;;
    esac
}

# A late, expected shutdown log must not retroactively fail a clean stop.
stop_after_runtime_log_check 120 "$container"
[[ $stopped == true ]]

# A runtime fatal error must fail before Docker is asked to stop the container.
logs='PHP Fatal error: runtime failure'
stopped=false
if stop_after_runtime_log_check 120 "$container" >/dev/null 2>&1; then
    printf '%s\n' 'Runtime fatal error was accepted' >&2
    exit 1
fi
[[ $stopped == false ]]

# A clean runtime log still requires an exit code of zero after stopping.
logs='normal runtime output'
stopped=false
exit_code=1
if stop_after_runtime_log_check 120 "$container" >/dev/null 2>&1; then
    printf '%s\n' 'Nonzero container exit was accepted' >&2
    exit 1
fi
[[ $stopped == true ]]

printf '%s\n' 'lifecycle log ordering: passed'
