<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Enums\AppearanceMode;
use App\Enums\ThemePreset;
use App\Support\Theming\Appearance;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Validation\Rule;

final class AppearanceController extends Controller
{
    public function __invoke(Request $request, Appearance $appearance): JsonResponse
    {
        $actor = $appearance->actor($request);
        abort_unless($actor !== null, 401);
        $data = $request->validate([
            'account' => ['required', 'integer'],
            'mode' => ['required', Rule::enum(AppearanceMode::class)],
            'preset' => ['required', Rule::enum(ThemePreset::class)],
            'adopt' => ['sometimes', 'boolean'],
        ]);
        abort_unless((int) $data['account'] === $actor->id, 409, 'The signed-in account has changed.');

        return response()->json([
            'account' => $actor->id,
            'preference' => $appearance->save($actor, ['mode' => $data['mode'], 'preset' => $data['preset']], $request->boolean('adopt')),
        ]);
    }
}
