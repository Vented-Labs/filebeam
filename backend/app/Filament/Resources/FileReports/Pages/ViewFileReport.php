<?php

declare(strict_types=1);

namespace App\Filament\Resources\FileReports\Pages;

use App\Filament\Resources\FileReports\FileReportResource;
use Filament\Actions\Action;
use Filament\Actions\ActionGroup;
use Filament\Resources\Pages\ViewRecord;
use Illuminate\Support\Str;
use Livewire\Attributes\Url;

class ViewFileReport extends ViewRecord
{
    protected static string $resource = FileReportResource::class;

    #[Url(as: 'queue')]
    public ?string $queueTab = null;

    public function getTitle(): string
    {
        return 'Report '.Str::substr($this->getRecord()->getKey(), -8);
    }

    public function mount(int|string $record): void
    {
        parent::mount($record);

        if (! in_array($this->queueTab, ['mine', 'unassigned', 'active', 'closed'], true)) {
            $this->queueTab = null;
        }
    }

    /** @return array<Action|ActionGroup> */
    protected function getHeaderActions(): array
    {
        return [
            Action::make('returnToQueue')
                ->label('Return to queue')
                ->visible(fn (): bool => $this->queueTab !== null)
                ->url(fn (): string => FileReportResource::getUrl('index', ['tab' => $this->queueTab])),
            FileReportResource::assignToMeAction(),
            FileReportResource::startReviewAction(),
            ActionGroup::make([
                FileReportResource::assignAction(),
                FileReportResource::unassignAction(),
                FileReportResource::resolveAction(),
                FileReportResource::dismissAction(),
                FileReportResource::reopenAction(),
                FileReportResource::takeDownAction(),
            ])->label('Decisions')->button(),
        ];
    }
}
