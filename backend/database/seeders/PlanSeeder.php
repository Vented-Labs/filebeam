<?php

declare(strict_types=1);

namespace Database\Seeders;

use App\Models\Plan;
use Illuminate\Database\Seeder;

class PlanSeeder extends Seeder
{
    public function run(): void
    {
        Plan::query()->updateOrCreate(
            ['slug' => 'default'],
            [
                'name' => 'Default',
                'maximum_transfer_bytes' => config('filebeam.default_plan.maximum_transfer_bytes'),
                'maximum_file_count' => config('filebeam.default_plan.maximum_file_count'),
                'maximum_note_bytes' => config('filebeam.default_plan.maximum_note_bytes'),
                'webrtc_maximum_transfer_bytes' => config('filebeam.default_plan.maximum_transfer_bytes'),
                'webrtc_maximum_file_count' => config('filebeam.default_plan.maximum_file_count'),
                'webrtc_maximum_note_bytes' => config('filebeam.default_plan.maximum_note_bytes'),
                'default_file_retention_hours' => config('filebeam.default_plan.file_retention_hours'),
                'maximum_file_retention_hours' => config('filebeam.default_plan.file_retention_hours'),
                'default_note_retention_hours' => config('filebeam.default_plan.note_retention_hours'),
                'maximum_note_retention_hours' => config('filebeam.default_plan.note_retention_hours'),
                'is_active' => true,
            ],
        );
    }
}
