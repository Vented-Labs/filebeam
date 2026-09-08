<?php

declare(strict_types=1);

namespace App\Console\Commands;

use Database\Seeders\FilestoreSeeder;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;

#[Signature('filebeam:sync-filestores')]
#[Description('Register configured filestores and initialize plans without assignments')]
class SyncFilestores extends Command
{
    public function handle(): int
    {
        app(FilestoreSeeder::class)->run();

        $this->info('Configured filestores are synchronized.');

        return self::SUCCESS;
    }
}
