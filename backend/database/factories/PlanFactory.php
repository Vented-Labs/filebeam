<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\Plan;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<Plan>
 */
class PlanFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'slug' => fake()->unique()->slug(2),
            'name' => fake()->words(2, true),
            'maximum_transfer_bytes' => 2 * 1024 * 1024 * 1024,
            'maximum_file_count' => 20,
            'maximum_note_bytes' => 1024 * 1024,
            'default_file_retention_hours' => 24,
            'maximum_file_retention_hours' => 24,
            'default_note_retention_hours' => 30 * 24,
            'maximum_note_retention_hours' => 30 * 24,
            'is_active' => true,
        ];
    }
}
