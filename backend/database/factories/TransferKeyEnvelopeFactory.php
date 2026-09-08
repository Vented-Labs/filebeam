<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\AccountKeyBundle;
use App\Models\Transfer;
use App\Models\TransferKeyEnvelope;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<TransferKeyEnvelope>
 */
class TransferKeyEnvelopeFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'transfer_id' => Transfer::factory(),
            'account_key_bundle_id' => AccountKeyBundle::factory(),
            'role' => 'recipient',
            'encrypted_key' => fake()->regexify('[A-Za-z0-9_-]{128}'),
        ];
    }
}
