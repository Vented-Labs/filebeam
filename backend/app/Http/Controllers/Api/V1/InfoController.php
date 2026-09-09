<?php

declare(strict_types=1);

namespace App\Http\Controllers\Api\V1;

use App\Http\Controllers\Controller;
use App\Http\Resources\InfoResource;
use App\Support\EffectivePlan;
use App\Support\InstanceSettings;
use App\Support\TransportPolicy;

class InfoController extends Controller
{
    public function __invoke(EffectivePlan $plans, InstanceSettings $settings, TransportPolicy $transport): InfoResource
    {
        $plan = $plans->default();
        $policy = $transport->configuration(null);
        $limits = $policy['limits']['http'];
        $fileRetentionHours = $plan?->default_file_retention_hours ?? config('filebeam.default_plan.file_retention_hours');
        $maximumFileRetentionHours = $plan?->maximum_file_retention_hours ?? $fileRetentionHours;

        return new InfoResource([
            'name' => config('app.name'),
            'protocol_versions' => [1],
            'anonymous_uploads_enabled' => $settings->boolean('anonymous_uploads'),
            'default_driver' => $policy['default_driver'],
            'enabled_drivers' => $policy['enabled_drivers'],
            'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
            'upload_concurrency' => config('filebeam.transfers.upload_concurrency'),
            'download_concurrency' => config('filebeam.transfers.download_concurrency'),
            'maximum_transfer_bytes' => $limits['maximum_transfer_bytes'],
            'maximum_file_count' => $limits['maximum_file_count'],
            'file_retention_hours' => $fileRetentionHours,
            'file_retention_options' => $this->retentionOptions($fileRetentionHours, $maximumFileRetentionHours),
        ]);
    }

    /** @return list<int> */
    private function retentionOptions(int $defaultHours, int $maximumHours): array
    {
        $options = array_filter(
            [1, 6, 12, 24, 72, 168, 720, 2160, 8760, $defaultHours, $maximumHours],
            fn (int $hours): bool => $hours <= $maximumHours,
        );
        sort($options);

        return array_values(array_unique($options));
    }
}
