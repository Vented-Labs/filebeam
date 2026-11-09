<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Support\InstanceSettings;
use Inertia\Inertia;
use Inertia\Response;

class FeaturesController extends Controller
{
    public function __invoke(InstanceSettings $settings): Response
    {
        return Inertia::render('Features', [
            'devices_enabled' => $settings->boolean('device_rendezvous'),
        ]);
    }
}
