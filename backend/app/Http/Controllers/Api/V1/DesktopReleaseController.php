<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Services\DesktopReleases;
use Illuminate\Http\JsonResponse;

class DesktopReleaseController
{
    public function __invoke(DesktopReleases $releases): JsonResponse
    {
        $result = $releases->latest();

        return response()->json($result, $result['state'] === 'error' ? 503 : 200, [
            'Cache-Control' => 'no-store',
            ...($result['state'] === 'error' ? ['Retry-After' => '30'] : []),
        ]);
    }
}
