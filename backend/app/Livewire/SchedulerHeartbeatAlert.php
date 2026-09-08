<?php

declare(strict_types=1);

namespace App\Livewire;

use Illuminate\Contracts\View\View;
use Illuminate\Support\Carbon;
use Illuminate\Support\Facades\DB;
use Livewire\Component;

class SchedulerHeartbeatAlert extends Component
{
    public function mount(): void
    {
        abort_unless(auth('admin')->check(), 403);
    }

    public function render(): View
    {
        $lastRunAt = DB::table('scheduler_heartbeats')
            ->where('name', 'scheduler')
            ->value('last_run_at');

        $lastRunAt = $lastRunAt === null ? null : Carbon::parse($lastRunAt, 'UTC');

        return view('livewire.scheduler-heartbeat-alert', [
            'lastRunAt' => $lastRunAt,
            'isHealthy' => $lastRunAt?->greaterThanOrEqualTo(now('UTC')->subHour()) ?? false,
        ]);
    }
}
