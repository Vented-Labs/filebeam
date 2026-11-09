<?php

declare(strict_types=1);

use App\Models\InstanceSetting;
use Inertia\Testing\AssertableInertia as Assert;

it('reports device transfers as disabled by default', function (): void {
    $this->get('/features')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('Features')
        ->where('devices_enabled', false));
});

it('reports device transfers when registered-device rendezvous is enabled', function (): void {
    InstanceSetting::query()->create(['key' => 'device_rendezvous', 'value' => true]);

    $this->get('/features')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('Features')
        ->where('devices_enabled', true));
});
