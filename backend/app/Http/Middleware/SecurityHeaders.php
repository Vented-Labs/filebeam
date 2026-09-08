<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Vite;
use Symfony\Component\HttpFoundation\Response;

class SecurityHeaders
{
    /**
     * Handle an incoming request.
     *
     * @param  Closure(Request): (Response)  $next
     */
    public function handle(Request $request, Closure $next): Response
    {
        Vite::useCspNonce();
        $response = $next($request);
        $response->headers->set('Referrer-Policy', 'no-referrer');
        $response->headers->set('X-Content-Type-Options', 'nosniff');
        $response->headers->set('X-Frame-Options', 'DENY');
        $response->headers->set('Permissions-Policy', 'camera=(), microphone=(), geolocation=()');
        $response->headers->set('Cache-Control', 'private, no-store');

        if (app()->isProduction()) {
            $nonce = Vite::cspNonce();
            // Filament's inline boot scripts and Alpine expressions require a panel-only policy.
            $scripts = $request->is('admin', 'admin/*')
                ? "'self' 'unsafe-inline' 'unsafe-eval'"
                : "'self' 'nonce-{$nonce}' 'wasm-unsafe-eval'";
            $response->headers->set('Content-Security-Policy', "default-src 'self'; script-src {$scripts}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'");

            if ($request->isSecure()) {
                $response->headers->set('Strict-Transport-Security', 'max-age=31536000');
            }
        }

        return $response;
    }
}
