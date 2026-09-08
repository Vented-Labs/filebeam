<?php

declare(strict_types=1);

namespace App\Filament\Resources\Filestores\Pages;

use App\Filament\Resources\Filestores\FilestoreResource;
use Filament\Actions\CreateAction;
use Filament\Resources\Pages\ListRecords;

class ListFilestores extends ListRecords
{
    protected static string $resource = FilestoreResource::class;

    protected function getHeaderActions(): array
    {
        return [CreateAction::make()->visible(fn (): bool => ! FilestoreResource::environmentManaged())];
    }

    public function getSubheading(): ?string
    {
        return FilestoreResource::environmentManaged()
            ? 'Storage definitions are controlled by the environment and are view-only in Admin.'
            : null;
    }
}
