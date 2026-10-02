<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Support\Theming\Appearance;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

final class ThemeStylesheetController extends Controller
{
    public function __invoke(Request $request, string $version): Response
    {
        abort_unless(hash_equals(Appearance::revision(), $version), 404);
        $response = response('', 200, ['Content-Type' => 'text/css; charset=UTF-8', 'Cache-Control' => 'public, max-age=31536000, immutable']);
        $response->setEtag($version);
        if (! $response->isNotModified($request)) {
            $response->setContent(Appearance::stylesheet());
        }

        return $response;
    }
}
