#!/usr/bin/env bash
# Shared lifecycle assertions for Docker acceptance containers.

stop_after_runtime_log_check() {
    local timeout=$1 container=$2

    assert_clean_logs || return
    docker stop -t "$timeout" "$container" >/dev/null || return
    if [[ $(docker inspect --format '{{.State.ExitCode}}' "$container") != 0 ]]; then
        printf '%s\n' 'Container did not exit cleanly' >&2
        return 1
    fi
}

wait_worker_ready() {
    local container=$1 config_ready=false processes_ready=false config
    local startup_deadline=$((SECONDS + 60)) health_deadline

    while ((SECONDS < startup_deadline)); do
        # Capture the whole response: piping curl to grep -q can close the pipe early.
        if config=$(docker exec "$container" curl --silent --fail --max-time 5 http://127.0.0.1:2019/config/); then
            [[ $config == *'/opt/filebeam/backend/public/frankenphp-worker.php'* ]] && config_ready=true || config_ready=false
        else
            config_ready=false
        fi

        # shellcheck disable=SC2016 # The process probe needs literal PHP variables and NULs.
        if docker exec --user 10001:10001 "$container" php -d display_errors=0 -r 'foreach (glob("/proc/[0-9]*/cmdline") as $path) { $command = @file_get_contents($path); if (!is_string($command)) continue; if (str_contains($command, "\0artisan\0queue:work\0")) $queue = true; if (str_contains($command, "\0artisan\0schedule:work\0")) $scheduler = true; } exit(($queue ?? false) && ($scheduler ?? false) ? 0 : 1);'; then
            processes_ready=true
        else
            processes_ready=false
        fi

        [[ $config_ready == true && $processes_ready == true ]] && break
        sleep 2
    done

    [[ $config_ready == true ]] || { printf '%s\n' 'Worker Caddy configuration did not become ready' >&2; return 1; }
    [[ $processes_ready == true ]] || { printf '%s\n' 'Worker queue or scheduler process did not become ready' >&2; return 1; }

    # schedule:work emits its first heartbeat at the next minute boundary.
    # That wait must not consume the worker-startup budget above.
    health_deadline=$((SECONDS + 120))
    while ((SECONDS < health_deadline)); do
        docker exec "$container" /usr/local/bin/filebeam-healthcheck && return 0
        sleep 2
    done

    printf '%s\n' 'Application health check did not become ready' >&2
    return 1
}
