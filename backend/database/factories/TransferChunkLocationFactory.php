<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\Filestore;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<TransferChunkLocation>
 */
class TransferChunkLocationFactory extends Factory
{
    public function definition(): array
    {
        $filestore = Filestore::query()->first();

        return [
            'transfer_chunk_id' => TransferChunk::factory(),
            'filestore_id' => $filestore->id ?? Filestore::factory(),
            'storage_path' => 'transfers/'.fake()->uuid().'.bin',
            'ciphertext_bytes' => 1024,
        ];
    }
}
