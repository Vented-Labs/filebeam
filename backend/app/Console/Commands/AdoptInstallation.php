<?php

declare(strict_types=1);

namespace App\Console\Commands;

use App\Enums\UserRole;
use App\Models\User;
use App\Support\Installation\InstallationState;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;
use Illuminate\Support\Facades\Schema;
use Illuminate\Support\Str;

#[Signature('filebeam:installation:adopt')]
#[Description('Permanently close the web installer for an existing Filebeam deployment')]
class AdoptInstallation extends Command
{
    public function handle(InstallationState $state): int
    {
        return $state->locked(function () use ($state): int {
            if ($state->isCompleted()) {
                $this->info('This deployment is already marked installed.');

                return self::SUCCESS;
            }
            if ($state->read() !== null || ! Schema::hasTable('users') || ! User::query()->activeStaff()->where('role', UserRole::Admin)->exists()) {
                $this->error('Adoption requires an existing verified administrator and no incomplete installation state.');

                return self::FAILURE;
            }
            $state->write(['id' => (string) Str::uuid(), 'status' => 'completed', 'adopted_at' => now()->toIso8601String()]);
            $this->info('Installation marked completed. The web installer is permanently unavailable.');

            return self::SUCCESS;
        });
    }
}
