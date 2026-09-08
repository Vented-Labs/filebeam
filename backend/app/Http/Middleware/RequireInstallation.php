<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use App\Support\Installation\InstallationState;
use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

readonly class RequireInstallation
{
    public function __construct(private InstallationState $state) {}

    /** @param Closure(Request): Response $next */
    public function handle(Request $request, Closure $next): Response
    {
        if ($request->is('install', 'install/*')) {
            // Do this before database, host, session, and request-size middleware.
            abort_unless($this->state->isPending() || $this->state->canBootstrap(), 404);
            config()->set('app.debug', false);

            return $next($request);
        }
        if ($this->state->requiresSetup() && ! $request->is('up')) {
            if ($request->isMethod('GET') && ! $request->is('api/*') && ! $request->expectsJson()) {
                return redirect('/install');
            }

            return response()->json(['message' => 'Filebeam installation is not complete.'], 503);
        }

        return $next($request);
    }
}
