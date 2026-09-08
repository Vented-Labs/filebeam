<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\Filestore;
use App\Models\Transfer;
use App\Models\TransferChunkUpload;
use App\Models\TransferItem;
use Illuminate\Database\Eloquent\Factories\Factory;
use Illuminate\Support\Str;

/**
 * @extends Factory<TransferChunkUpload>
 */
class TransferChunkUploadFactory extends Factory
{
    public function definition(): array
    {
        $filestore = Filestore::query()->firstOrCreate(
            ['disk_name' => 'transfers'],
            ['name' => 'Transfers', 'source' => 'laravel', 'placement_enabled' => true],
        );

        return [
            'id' => (string) Str::ulid(),
            'transfer_id' => Transfer::factory(),
            'transfer_item_id' => TransferItem::factory(),
            'position' => 0,
            'filestore_id' => $filestore->id,
            'storage_path' => 'transfers/'.fake()->uuid().'.bin',
            'ciphertext_bytes' => 1024,
            'checksum' => hash('sha256', fake()->uuid()),
        ];
    }
}
