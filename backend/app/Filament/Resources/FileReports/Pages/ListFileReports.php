<?php

declare(strict_types=1);

namespace App\Filament\Resources\FileReports\Pages;

use App\Filament\Resources\FileReports\FileReportResource;
use App\Models\FileReport;
use App\Models\User;
use Filament\Resources\Pages\ListRecords;
use Filament\Schemas\Components\Tabs\Tab;
use Illuminate\Database\Eloquent\Builder;
use Livewire\Attributes\Url;

class ListFileReports extends ListRecords
{
    protected static string $resource = FileReportResource::class;

    #[Url(as: 'tab')]
    public ?string $activeTab = 'active';

    /** @return array<string, Tab> */
    public function getTabs(): array
    {
        $actor = auth('admin')->user();

        if (! $actor instanceof User) {
            return [];
        }

        return [
            'mine' => Tab::make('Mine')->badge(fn (): int => $this->activeReports()->assignedTo($actor->id)->count())->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(FileReport::query()->active()->assignedTo($actor->id)),
            ),
            'unassigned' => Tab::make('Unassigned')->badge(fn (): int => $this->activeReports()->unassigned()->count())->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(FileReport::query()->active()->unassigned()),
            ),
            'active' => Tab::make('Active')->badge(fn (): int => $this->activeReports()->count())->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(FileReport::query()->active()),
            ),
            'closed' => Tab::make('Closed')->badge(fn (): int => FileReport::query()->closed()->count())->modifyQueryUsing(
                fn (Builder $query): Builder => $query->mergeConstraintsFrom(FileReport::query()->closed()),
            ),
        ];
    }

    /** @return Builder<FileReport> */
    private function activeReports(): Builder
    {
        return FileReport::query()->active();
    }
}
