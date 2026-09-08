<?php

declare(strict_types=1);

namespace App\Filament\Resources\Transfers\Pages;

use App\Filament\Resources\Transfers\TransferResource;
use Filament\Actions\Action;
use Filament\Resources\Pages\ViewRecord;
use Illuminate\Support\Str;

class ViewTransfer extends ViewRecord
{
    protected static string $resource = TransferResource::class;

    public function getTitle(): string
    {
        return 'Upload '.Str::substr($this->getRecord()->getKey(), -8);
    }

    /** @return array<Action> */
    protected function getHeaderActions(): array
    {
        return [
            TransferResource::takedownAction(),
            TransferResource::retryCleanupAction(),
        ];
    }
}
