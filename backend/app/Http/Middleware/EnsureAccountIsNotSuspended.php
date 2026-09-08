<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use App\Models\User;
use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class EnsureAccountIsNotSuspended
{
    public function handle(Request $request, Closure $next): Response
    {
        $user = $request->user('web');

        if (! $request->routeIs('logout') && $user instanceof User && $user->suspended_at !== null) {
            abort(403, 'This account has been suspended.');
        }

        return $next($request);
    }
}
