<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use Illuminate\Http\JsonResponse;

class AndroidAssetLinksController extends Controller
{
    public function __invoke(): JsonResponse
    {
        $apps = config('filebeam.android.apps');

        abort_unless(is_array($apps) && $apps !== [], 404);

        return response()->json(array_map(fn (array $app): array => [
            'relation' => ['delegate_permission/common.handle_all_urls'],
            'target' => [
                'namespace' => 'android_app',
                'package_name' => $app['package'],
                'sha256_cert_fingerprints' => $app['fingerprints'],
            ],
        ], $apps), 200, ['Cache-Control' => 'public, max-age=3600, s-maxage=3600, must-revalidate']);
    }
}
