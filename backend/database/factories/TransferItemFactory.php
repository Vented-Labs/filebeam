<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\Transfer;
use App\Models\TransferItem;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<TransferItem>
 */
class TransferItemFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'transfer_id' => Transfer::factory(),
            'position' => 0,
            'chunk_count' => 1,
            'declared_ciphertext_bytes' => 1024,
        ];
    }
}
