<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use App\Models\Transfer;
use App\Support\InstanceSettings;
use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class EnsureAnonymousTransferUploadsAreEnabled
{
    /**
     * @param  Closure(Request): Response  $next
     */
    public function handle(Request $request, Closure $next): Response
    {
        $transfer = $request->route('transfer');

        abort_unless(
            $transfer instanceof Transfer
                && ($transfer->owner_id !== null || app(InstanceSettings::class)->boolean('anonymous_uploads')),
            403,
        );

        return $next($request);
    }
}
