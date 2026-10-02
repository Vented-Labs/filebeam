<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Support\Theming\Assets;
use App\Support\Theming\Palette;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

final class ThemeAssetController extends Controller
{
    public function __invoke(Request $request, Assets $assets, string $version, string $primary, string $mode, string $asset): Response
    {
        abort_unless(filled(config('app.key')) && $request->hasValidRelativeSignature(), 403);
        $bytes = $assets->render($asset, new Palette('#'.$primary), $mode, $version);
        $type = match (pathinfo($asset, PATHINFO_EXTENSION)) {
            'svg' => 'image/svg+xml',
            'ico' => 'image/vnd.microsoft.icon',
            default => 'image/png',
        };
        $response = response($bytes, 200, ['Content-Type' => $type, 'Cache-Control' => 'public, max-age=31536000, immutable']);
        $response->setEtag(hash('sha256', $bytes));
        $response->isNotModified($request);

        return $response;
    }
}
