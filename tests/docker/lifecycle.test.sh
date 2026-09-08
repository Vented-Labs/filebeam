#!/usr/bin/env bash
# Regression coverage for startup readiness and log checks before an intentional stop.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=scripts/docker/lifecycle.sh
source "$root/scripts/docker/lifecycle.sh"

container=lifecycle-test
logs='normal runtime output'
stopped=false
exit_code=0
scenario=shutdown
config_ready_at=0
processes_ready_at=0
health_ready_at=0

assert_clean_logs() {
    ! printf '%s\n' "$logs" | grep -Ei '"level":"error"|PHP Fatal error|FATAL:|ERROR:'
}

docker() {
    case "$1" in
        exec)
            [[ $scenario == readiness ]] || { printf 'Unexpected docker command: %s\n' "$1" >&2; return 1; }
            if [[ $* == *curl* ]]; then
                ((SECONDS >= config_ready_at)) || return 1
                printf '%s\n' '/opt/filebeam/backend/public/frankenphp-worker.php'
            elif [[ $* == *filebeam-healthcheck* ]]; then
                ((health_ready_at >= 0 && SECONDS >= health_ready_at))
            elif [[ $* == *php* ]]; then
                ((SECONDS >= processes_ready_at))
            else
                printf 'Unexpected docker exec: %s\n' "$*" >&2
                return 1
            fi
            ;;
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

# Advance the shell clock without waiting or requiring Docker.
sleep() { SECONDS=$((SECONDS + $1)); }

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

run_readiness() {
    local expected=$1 output
    if output=$(wait_worker_ready "$container" 2>&1); then
        [[ -z $expected ]] || { printf 'Readiness unexpectedly passed: %s\n' "$expected" >&2; return 1; }
    else
        [[ -n $expected ]] || { printf 'Readiness unexpectedly failed: %s\n' "$output" >&2; return 1; }
        [[ $output == *"$expected"* ]] || { printf 'Unexpected readiness diagnostic: %s\n' "$output" >&2; return 1; }
    fi
}

# The health deadline starts only after worker transition, not at installation completion.
scenario=readiness
SECONDS=1 config_ready_at=5 processes_ready_at=5 health_ready_at=65
run_readiness ''

# A transition near the old 60-second boundary still receives a fresh health budget.
SECONDS=0 config_ready_at=58 processes_ready_at=58 health_ready_at=120
run_readiness ''

SECONDS=0 config_ready_at=999 processes_ready_at=0 health_ready_at=0
run_readiness 'Worker Caddy configuration did not become ready'

SECONDS=0 config_ready_at=0 processes_ready_at=999 health_ready_at=0
run_readiness 'Worker queue or scheduler process did not become ready'

SECONDS=0 config_ready_at=0 processes_ready_at=0 health_ready_at=-1
run_readiness 'Application health check did not become ready'

printf '%s\n' 'lifecycle readiness and log ordering: passed'
