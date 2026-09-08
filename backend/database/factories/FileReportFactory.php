<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Enums\ReportStatus;
use App\Models\FileReport;
use App\Models\Transfer;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<FileReport>
 */
class FileReportFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'transfer_id' => Transfer::factory(),
            'transfer_identifier' => fn (array $attributes): string => $attributes['transfer_id'],
            'reporter_email' => fake()->safeEmail(),
            'category' => fake()->word(),
            'description' => fake()->paragraph(),
            'status' => ReportStatus::Open,
        ];
    }
}
