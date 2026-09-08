<?php

declare(strict_types=1);

namespace Database\Factories;

use App\Models\User;
use App\Models\UserInvitation;
use Illuminate\Database\Eloquent\Factories\Factory;

/**
 * @extends Factory<UserInvitation>
 */
class UserInvitationFactory extends Factory
{
    public function definition(): array
    {
        return [
            'invited_by' => User::factory(),
            'email' => fake()->unique()->safeEmail(),
            'token_hash' => hash('sha256', fake()->uuid()),
            'expires_at' => now()->addHours(72),
        ];
    }
}
