<?php

declare(strict_types=1);

namespace App\Console\Commands;

use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;
use Illuminate\Support\Facades\DB;

#[Signature('filebeam:prune-transfer-history')]
#[Description('Remove transfer history summaries after their 90-day retention window')]
class PruneTransferHistory extends Command
{
    public function handle(): int
    {
        DB::table('transfer_history_entries')->where('purge_at', '<=', now())->select('id')->chunkById(100, function ($entries): void {
            DB::table('transfer_history_entries')->whereIn('id', $entries->pluck('id'))->delete();
        });

        return self::SUCCESS;
    }
}
