<?php

declare(strict_types=1);

namespace App\Console\Commands;

use App\Support\Installation\EnvironmentWriter;
use App\Support\Installation\InstallationState;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;

#[Signature('filebeam:installation:bootstrap')]
#[Description('Initialize pending web setup without replacing an existing installation')]
class BootstrapInstallation extends Command
{
    public function handle(InstallationState $state, EnvironmentWriter $writer): int
    {
        if (! $state->requiresSetup()) {
            $this->info('This deployment is already initialized.');

            return self::SUCCESS;
        }
        if ($state->isPending() && is_file($state->environmentPath())) {
            $this->info('Web setup is already pending.');

            return self::SUCCESS;
        }
        if (! $state->canBootstrap()) {
            $this->error('Existing data or inconsistent installation state prevents bootstrap. Restore the matching configuration instead.');

            return self::FAILURE;
        }

        $state->bootstrap($writer);
        $this->info('Web setup initialized. The container supervisor logs the installation token while setup is pending.');

        return self::SUCCESS;
    }
}
