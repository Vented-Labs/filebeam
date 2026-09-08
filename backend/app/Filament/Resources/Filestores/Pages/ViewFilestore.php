<?php

declare(strict_types=1);

namespace App\Filament\Resources\Filestores\Pages;

use App\Filament\Resources\Filestores\FilestoreResource;
use Filament\Actions\EditAction;
use Filament\Resources\Pages\ViewRecord;

class ViewFilestore extends ViewRecord
{
    protected static string $resource = FilestoreResource::class;

    protected function getHeaderActions(): array
    {
        return [EditAction::make()->visible(fn (): bool => ! FilestoreResource::environmentManaged())];
    }

    public function getSubheading(): ?string
    {
        return FilestoreResource::environmentManaged()
            ? 'Storage definitions are controlled by the environment and are view-only in Admin.'
            : null;
    }
}
