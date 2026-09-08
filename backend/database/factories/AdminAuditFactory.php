<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\AdminAudit;
use App\Models\Transfer;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<AdminAudit>
 */
class AdminAuditFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'action' => fake()->word(),
            'target_type' => Transfer::class,
            'target_id' => Transfer::factory(),
            'reason' => fake()->sentence(),
            'changes' => ['status' => 'deleting'],
        ];
    }
}
