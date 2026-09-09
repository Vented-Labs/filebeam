<?php

declare(strict_types=1);

use App\Http\Middleware\RequireInstallation;
use App\Models\InstanceSetting;
use App\Models\InstanceTransportPolicy;
use App\Models\Plan;

beforeEach(function (): void {
    $this->withoutMiddleware(RequireInstallation::class);
});

test('anonymous clients receive the effective default transfer configuration', function (): void {
    config()->set('app.name', 'CLI Filebeam');
    config()->set('filebeam.transfers.chunk_bytes', 1_048_560);
    config()->set('filebeam.transfers.upload_concurrency', 3);
    config()->set('filebeam.transfers.download_concurrency', 5);
    Plan::factory()->create([
        'slug' => config('filebeam.transfers.default_plan'),
        'maximum_transfer_bytes' => 123_456,
        'maximum_file_count' => 4,
        'webrtc_maximum_transfer_bytes' => 987_654,
        'webrtc_maximum_file_count' => 9,
        'default_file_retention_hours' => 12,
        'maximum_file_retention_hours' => 168,
    ]);
    InstanceTransportPolicy::query()->findOrFail(1)->update([
        'enabled_drivers' => ['http', 'webrtc'],
        'default_driver' => 'webrtc',
    ]);
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);

    $this->getJson('/api/v1/info')
        ->assertOk()
        ->assertExactJson(['data' => [
            'name' => 'CLI Filebeam',
            'protocol_versions' => [1],
            'anonymous_uploads_enabled' => false,
            'default_driver' => 'webrtc',
            'enabled_drivers' => ['http', 'webrtc'],
            'chunk_bytes' => 1_048_560,
            'upload_concurrency' => 3,
            'download_concurrency' => 5,
            'maximum_transfer_bytes' => 123_456,
            'maximum_file_count' => 4,
            'file_retention_hours' => 12,
            'file_retention_options' => [1, 6, 12, 24, 72, 168],
        ]]);
});
