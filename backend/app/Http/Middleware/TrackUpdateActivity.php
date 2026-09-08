<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use Closure;
use Filebeam\Updater\ActivityLock;
use Illuminate\Http\Request;
use RuntimeException;
use Symfony\Component\HttpFoundation\Response;

class TrackUpdateActivity
{
    private const ATTRIBUTE = 'filebeam.update_activity_lock';

    /**
     * @param  Closure(Request): Response  $next
     */
    public function handle(Request $request, Closure $next): Response
    {
        if (config('version.distribution') !== 'package' || $this->isReadOnlyRequest($request)) {
            return $next($request);
        }

        try {
            $request->attributes->set(self::ATTRIBUTE, $this->lock()->acquireShared());
        } catch (RuntimeException) {
            return response('Service Unavailable', 503);
        }

        return $next($request);
    }

    public function terminate(Request $request, Response $response): void
    {
        $handle = $request->attributes->get(self::ATTRIBUTE);
        $request->attributes->remove(self::ATTRIBUTE);

        if (is_resource($handle)) {
            $this->lock()->release($handle);
        }
    }

    private function isReadOnlyRequest(Request $request): bool
    {
        if ($request->method() !== 'GET') {
            return false;
        }

        if ($request->path() === 'updater/probe') {
            return true;
        }

        if (! app()->maintenanceMode()->active()) {
            return false;
        }

        $secret = app()->maintenanceMode()->data()['secret'] ?? null;

        return is_string($secret) && hash_equals($secret, $request->path());
    }

    private function lock(): ActivityLock
    {
        return new ActivityLock(storage_path('app/update-activity.lock'));
    }
}
