<?php

declare(strict_types=1);

namespace App\Filament\Resources\Users\Pages;

use App\Filament\Resources\Users\UserResource;
use Filament\Actions\Action;
use Filament\Resources\Pages\ViewRecord;

class ViewUser extends ViewRecord
{
    protected static string $resource = UserResource::class;

    /** @return array<Action> */
    protected function getHeaderActions(): array
    {
        return [
            UserResource::changeRoleAction(),
            UserResource::changePlanAction(),
            UserResource::suspensionAction(),
        ];
    }
}
