<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use Closure;
use Illuminate\Foundation\Http\Middleware\PreventRequestForgery;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class ProtectAuthenticatedTransferCreation
{
    /** @param Closure(Request): Response $next */
    public function handle(Request $request, Closure $next): Response
    {
        // The public API remains usable without a session. Cookie-authenticated creation is stateful.
        if ($request->user() === null) {
            return $next($request);
        }

        return app(PreventRequestForgery::class)->handle($request, $next);
    }
}
