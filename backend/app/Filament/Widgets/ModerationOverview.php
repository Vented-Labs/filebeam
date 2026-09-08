<?php

declare(strict_types=1);

namespace App\Filament\Widgets;

use App\Filament\Resources\FileReports\FileReportResource;
use App\Filament\Resources\Transfers\TransferResource;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Filament\Facades\Filament;
use Filament\Widgets\StatsOverviewWidget;
use Filament\Widgets\StatsOverviewWidget\Stat;

class ModerationOverview extends StatsOverviewWidget
{
    protected ?string $heading = 'Needs attention';

    protected static ?int $sort = 1;

    protected ?string $pollingInterval = '60s';

    public static function canView(): bool
    {
        $actor = Filament::auth()->user();

        return $actor instanceof User && $actor->isStaff();
    }

    /** @return array<Stat> */
    protected function getStats(): array
    {
        $actor = Filament::auth()->user();

        if (! $actor instanceof User) {
            return [];
        }

        $activeReports = FileReport::query()->active();

        return [
            Stat::make('My queue', (clone $activeReports)->assignedTo($actor->id)->count())
                ->description('Continue your investigations')->color('primary')
                ->url(FileReportResource::getUrl('index', ['tab' => 'mine'])),
            Stat::make('Unassigned', (clone $activeReports)->unassigned()->count())
                ->description('Reports waiting for an owner')->color('warning')
                ->url(FileReportResource::getUrl('index', ['tab' => 'unassigned'])),
            Stat::make('All active reports', (clone $activeReports)->count())
                ->description('Open and in review')
                ->url(FileReportResource::getUrl('index', ['tab' => 'active'])),
            Stat::make('Awaiting cleanup', Transfer::query()->awaitingCleanup()->count())
                ->description('Unavailable uploads, not confirmed failures')->color('info')
                ->url(TransferResource::getUrl('index', ['tab' => 'deleting'])),
        ];
    }
}
