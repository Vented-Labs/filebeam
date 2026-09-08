<?php

declare(strict_types=1);

namespace App\Filament\Resources\Transfers\Pages;

use App\Enums\TransferStatus;
use App\Filament\Resources\Transfers\TransferResource;
use App\Models\FileReport;
use App\Models\Transfer;
use Filament\Resources\Pages\ListRecords;
use Filament\Schemas\Components\Tabs\Tab;
use Illuminate\Database\Eloquent\Builder;

class ListTransfers extends ListRecords
{
    protected static string $resource = TransferResource::class;

    /** @return array<string, Tab> */
    public function getTabs(): array
    {
        $statusCounts = Transfer::query()
            ->selectRaw('status, count(*) as aggregate')
            ->groupBy('status')
            ->pluck('aggregate', 'status');
        $expiredCount = Transfer::query()->where('expires_at', '<=', now())->count();
        $availableCount = Transfer::query()->availableAndUnexpired()->count();

        return [
            'all' => Tab::make('All')->badge((string) $statusCounts->sum()),
            'available' => Tab::make('Available')->badge((string) $availableCount)->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(Transfer::query()->availableAndUnexpired()),
            ),
            'pending' => Tab::make('Uploading')->badge((string) ($statusCounts[TransferStatus::Pending->value] ?? 0))->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(Transfer::query()->pending()),
            ),
            'reported' => Tab::make('Reported')->modifyQueryUsing(
                fn (Builder $query): Builder => $query->whereHas('reports',
                    fn (Builder $query): Builder => $query->mergeConstraintsFrom(FileReport::query()->active()),
                ),
            ),
            'deleting' => Tab::make('Awaiting cleanup')->badge((string) ($statusCounts[TransferStatus::Deleting->value] ?? 0))->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(Transfer::query()->awaitingCleanup()),
            ),
            'expired' => Tab::make('Expired')->badge((string) $expiredCount)->modifyQueryUsing(fn (Builder $query): Builder => $query->where('expires_at', '<=', now())),
        ];
    }
}
