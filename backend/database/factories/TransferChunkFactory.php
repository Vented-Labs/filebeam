<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\TransferChunk;
use App\Models\TransferItem;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<TransferChunk>
 */
class TransferChunkFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'transfer_item_id' => TransferItem::factory(),
            'position' => 0,
            'ciphertext_bytes' => 1024,
            'checksum' => hash('sha256', fake()->uuid()),
        ];
    }
}
