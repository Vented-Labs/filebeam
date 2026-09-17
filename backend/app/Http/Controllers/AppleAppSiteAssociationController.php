<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use Illuminate\Http\JsonResponse;

class AppleAppSiteAssociationController extends Controller
{
    public function __invoke(): JsonResponse
    {
        $appIds = config('filebeam.apple.app_ids');

        abort_unless(is_array($appIds) && $appIds !== [], 404);

        return response()->json([
            'applinks' => [
                'details' => array_map(fn (string $appId): array => [
                    'appIDs' => [$appId],
                    'components' => [
                        // AASA cannot express the ULID alphabet or its leading 0..7 restriction.
                        ['/' => '/??????????????????????????'],
                        ['/' => '/u/*'],
                        ['/' => '/invitations/*'],
                        ['/' => '/verify-email/*'],
                        ['/' => '/reset-password/*'],
                    ],
                ], $appIds),
            ],
        ], 200, ['Cache-Control' => 'public, max-age=3600, s-maxage=3600, must-revalidate']);
    }
}
