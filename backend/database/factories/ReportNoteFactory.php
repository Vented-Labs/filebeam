<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\FileReport;
use App\Models\ReportNote;
use App\Models\User;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<ReportNote>
 */
class ReportNoteFactory extends Factory
{
    public function definition(): array
    {
        return [
            'file_report_id' => FileReport::factory(),
            'author_id' => User::factory(),
            'body' => fake()->paragraph(),
        ];
    }
}
