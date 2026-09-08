<?php

declare(strict_types=1);

namespace App\Filament\Resources\Users\Pages;

use App\Filament\Resources\Users\UserResource;
use Filament\Actions\Action;
use Filament\Resources\Pages\ListRecords;

class ListUsers extends ListRecords
{
    protected static string $resource = UserResource::class;

    /** @return array<Action> */
    protected function getHeaderActions(): array
    {
        return [
            UserResource::createUserAction(),
            UserResource::inviteUserAction(),
        ];
    }
}
