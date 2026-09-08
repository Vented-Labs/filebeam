#!/usr/bin/env bash
# Shared lifecycle assertion for Docker acceptance containers.

stop_after_runtime_log_check() {
    local timeout=$1 container=$2

    assert_clean_logs || return
    docker stop -t "$timeout" "$container" >/dev/null || return
    if [[ $(docker inspect --format '{{.State.ExitCode}}' "$container") != 0 ]]; then
        printf '%s\n' 'Container did not exit cleanly' >&2
        return 1
    fi
}
