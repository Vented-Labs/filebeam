<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\Filestore;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<Filestore>
 */
class FilestoreFactory extends Factory
{
    public function definition(): array
    {
        return [
            'name' => 'Transfers',
            'source' => 'laravel',
            'disk_name' => 'transfers',
            'driver' => null,
            'configuration' => null,
            'placement_enabled' => true,
        ];
    }
}
