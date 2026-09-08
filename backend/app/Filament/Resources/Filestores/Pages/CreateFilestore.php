<?php

declare(strict_types=1);

namespace App\Filament\Resources\Filestores\Pages;

use App\Actions\Admin\ManageFilestore;
use App\Filament\Resources\Filestores\FilestoreResource;
use App\Models\User;
use Filament\Facades\Filament;
use Filament\Resources\Pages\CreateRecord;
use Illuminate\Database\Eloquent\Model;

class CreateFilestore extends CreateRecord
{
    protected static string $resource = FilestoreResource::class;

    protected function handleRecordCreation(array $data): Model
    {
        $actor = Filament::auth()->user();
        abort_unless($actor instanceof User, 403);

        return app(ManageFilestore::class)->create($actor, $data);
    }
}
