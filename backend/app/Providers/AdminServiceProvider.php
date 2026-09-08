<?php

declare(strict_types=1);

namespace App\Providers;

use Illuminate\Cache\RateLimiting\Limit;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\RateLimiter;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\ServiceProvider;

class AdminServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        RateLimiter::for('file-reports', fn (Request $request): array => [
            Limit::perMinute(5)->by('minute:'.$request->ip()),
            Limit::perHour(30)->by('hour:'.$request->ip()),
        ]);

        if (! $this->app->routesAreCached()) {
            Route::middleware('web')->group(base_path('routes/reports.php'));
        }
    }
}
