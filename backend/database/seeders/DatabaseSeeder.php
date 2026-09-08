<?php

declare(strict_types=1);

namespace Database\Seeders;

use App\Enums\UserRole;
use App\Models\User;
use Illuminate\Database\Console\Seeds\WithoutModelEvents;
use Illuminate\Database\Seeder;

class DatabaseSeeder extends Seeder
{
    use WithoutModelEvents;

    public function run(): void
    {
        $this->call([
            PlanSeeder::class,
            FilestoreSeeder::class,
        ]);

        if (app()->isLocal()) {
            User::query()->firstOrCreate(
                ['email' => 'admin@filebeam.test'],
                [
                    'name' => 'Admin',
                    'username' => 'local_admin',
                    'normalized_username' => 'local_admin',
                    'password' => 'password',
                    'role' => UserRole::Admin,
                    'email_verified_at' => now(),
                ],
            );
        }
    }
}
