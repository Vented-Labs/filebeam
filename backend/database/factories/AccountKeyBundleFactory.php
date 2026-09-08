<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\AccountKeyBundle;
use App\Models\User;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<AccountKeyBundle>
 */
class AccountKeyBundleFactory extends Factory
{
    /** @return array<string, mixed> */
    public function definition(): array
    {
        return [
            'user_id' => User::factory(),
            'version' => 1,
            'public_key' => fake()->regexify('[A-Za-z0-9_-]{43}'),
            'fingerprint' => hash('sha256', fake()->uuid()),
            'encrypted_private_key' => fake()->regexify('[A-Za-z0-9_-]{128}'),
            'is_active' => true,
        ];
    }
}
