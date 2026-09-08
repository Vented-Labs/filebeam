<div wire:poll.60s class="fi-scheduler-heartbeat-alert w-full min-w-0">
    @unless ($isHealthy)
        <x-filament::callout color="danger" icon="heroicon-o-exclamation-triangle" heading="Scheduler heartbeat is unhealthy." role="alert" class="my-6 w-full min-w-0">
            <x-slot:footer>
                <div class="min-w-0">
                    <p>
                        @if ($lastRunAt)
                            Last heartbeat: {{ $lastRunAt->diffForHumans() }} ({{ $lastRunAt->format('Y-m-d H:i:s T') }}).
                        @else
                            No scheduler heartbeat has been recorded.
                        @endif
                        Expired transfers and abandoned upload attempts can remain until the next 15-minute cleanup runs after the scheduler is restored.
                    </p>

                    <details style="margin-top: 0.75rem;">
                        <summary>Enable the scheduler with cron</summary>
                        <p style="margin-top: 0.5rem;">Run it with the same PHP binary, runtime user, and environment as Filebeam:</p>
                        <code class="mt-2 block max-w-full overflow-x-auto">* * * * * cd /path/to/filebeam/backend && php artisan schedule:run >> /dev/null 2>&1</code>
                        <p style="margin-top: 0.5rem;">The bundled Docker image already runs <code>php artisan schedule:work</code> through Supervisor as the application user. Check the scheduler process and container logs instead of adding a duplicate cron entry.</p>
                    </details>
                </div>
            </x-slot:footer>
        </x-filament::callout>
    @endunless
</div>
