<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Enums\TransferDelivery;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<Transfer>
 */
class TransferFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        $filestore = Filestore::query()->firstOrCreate(
            ['disk_name' => 'transfers'],
            ['name' => 'Transfers', 'source' => 'laravel', 'placement_enabled' => true],
        );

        return [
            'kind' => TransferKind::Files,
            'delivery' => TransferDelivery::Link,
            'plan_id' => Plan::factory(),
            'status' => TransferStatus::Pending,
            'protocol_version' => 1,
            'chunk_bytes' => 25_000_000 - 16,
            'retention_hours' => 24,
            'placement_mode' => 'distribute',
            'filestore_ids' => [$filestore->id],
            'burn_on_read' => false,
            'declared_ciphertext_bytes' => 1024,
            'item_count' => 1,
            'upload_token_hash' => hash('sha256', fake()->uuid()),
            'delete_token_hash' => hash('sha256', fake()->uuid()),
            'read_token_hash' => null,
            'expires_at' => now()->addDay(),
        ];
    }
}
