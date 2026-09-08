<?php

declare(strict_types=1);

namespace App\Providers;

use Carbon\CarbonImmutable;
use Filebeam\Updater\ActivityLock;
use Illuminate\Cache\RateLimiting\Limit;
use Illuminate\Http\Request;
use Illuminate\Queue\Events\JobAttempted;
use Illuminate\Queue\Events\JobProcessing;
use Illuminate\Queue\Events\Looping;
use Illuminate\Queue\Events\WorkerIdle;
use Illuminate\Queue\Events\WorkerStopping;
use Illuminate\Support\Facades\Date;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Queue;
use Illuminate\Support\Facades\RateLimiter;
use Illuminate\Support\ServiceProvider;
use Illuminate\Validation\Rules\Password;

class AppServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        $this->configureDefaults();
        $this->configureRateLimiting();
        $this->configureUpdateActivityTracking();
    }

    protected function configureRateLimiting(): void
    {
        RateLimiter::for('transfer-creation', fn (Request $request): Limit => Limit::perHour((int) config('filebeam.rate_limits.creations_per_hour'))->by($request->ip()));
        RateLimiter::for('transfer-writing', fn (Request $request): Limit => Limit::perMinute((int) config('filebeam.rate_limits.writes_per_minute'))->by($request->ip()));
        RateLimiter::for('transfer-reading', fn (Request $request): Limit => Limit::perMinute((int) config('filebeam.rate_limits.reads_per_minute'))->by($request->ip()));
        RateLimiter::for('transfer-monitoring', fn (Request $request): Limit => Limit::perMinute((int) config('filebeam.rate_limits.monitor_per_minute'))->by($request->ip()));
        RateLimiter::for('download-session-registration', fn (Request $request): Limit => Limit::perMinute((int) config('filebeam.rate_limits.session_registration_per_minute'))->by($request->ip()));
        RateLimiter::for('download-session-reporting', fn (Request $request): Limit => Limit::perMinute((int) config('filebeam.rate_limits.session_reporting_per_minute'))->by($request->ip()));
        RateLimiter::for('account-key-writing', fn (Request $request): Limit => Limit::perMinute(5)->by((string) $request->user()?->getAuthIdentifier()));
    }

    protected function configureDefaults(): void
    {
        Date::use(CarbonImmutable::class);

        DB::prohibitDestructiveCommands(
            app()->isProduction(),
        );

        Password::defaults(fn (): Password => Password::min(8)
            ->when(app()->isProduction(), fn (Password $rule): Password => $rule
                ->mixedCase()
                ->letters()
                ->numbers()
                ->symbols()
                ->uncompromised()),
        );
    }

    protected function configureUpdateActivityTracking(): void
    {
        if (config('version.distribution') !== 'package') {
            return;
        }

        $lock = new ActivityLock(storage_path('app/update-activity.lock'));
        $handles = new \WeakMap;
        $loopState = new class
        {
            public mixed $handle = null;
        };
        $release = function (object $job) use (&$handles, $lock): void {
            if (! isset($handles[$job])) {
                return;
            }

            $handle = $handles[$job];
            unset($handles[$job]);

            if (is_resource($handle)) {
                $lock->release($handle);
            }
        };
        $releaseLoop = function () use ($loopState, $lock): void {
            if (is_resource($loopState->handle)) {
                $lock->release($loopState->handle);
            }

            $loopState->handle = null;
        };

        Queue::looping(function (Looping $event) use ($loopState, $lock): ?bool {
            if (config('version.distribution') !== 'package' || $loopState->handle !== null) {
                return null;
            }

            try {
                $loopState->handle = $lock->acquireShared();
            } catch (\RuntimeException) {
                return false;
            }

            return null;
        });

        Queue::before(function (JobProcessing $event) use (&$handles, $loopState, $lock): void {
            if (config('version.distribution') !== 'package') {
                return;
            }

            if (is_resource($loopState->handle)) {
                $handles[$event->job] = $loopState->handle;
                $loopState->handle = null;

                return;
            }

            $handles[$event->job] = $lock->acquireShared();
        });

        Event::listen(JobAttempted::class, function (JobAttempted $event) use ($release): void {
            if (config('version.distribution') === 'package') {
                $release($event->job);
            }
        });

        Event::listen(WorkerIdle::class, function (WorkerIdle $event) use ($releaseLoop): void {
            if (config('version.distribution') === 'package') {
                $releaseLoop();
            }
        });

        Event::listen(WorkerStopping::class, function (WorkerStopping $event) use ($releaseLoop): void {
            if (config('version.distribution') === 'package') {
                $releaseLoop();
            }
        });
    }
}
