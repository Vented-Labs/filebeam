<?php

declare(strict_types=1);

namespace App\Filament\Resources\Filestores\Pages;

use App\Actions\Admin\ManageFilestore;
use App\Filament\Resources\Filestores\FilestoreResource;
use App\Models\Filestore;
use App\Models\User;
use Filament\Actions\DeleteAction;
use Filament\Facades\Filament;
use Filament\Resources\Pages\EditRecord;
use Illuminate\Database\Eloquent\Model;

class EditFilestore extends EditRecord
{
    protected static string $resource = FilestoreResource::class;

    /**
     * @param  array<string, mixed>  $data
     * @return array<string, mixed>
     */
    protected function mutateFormDataBeforeFill(array $data): array
    {
        /** @var Filestore $filestore */
        $filestore = $this->getRecord();
        $configuration = is_array($filestore->configuration) ? $filestore->configuration : [];
        $data['configuration'] = collect($configuration)->except(['key', 'secret'])->all();

        return $data;
    }

    /**
     * @param  array<string, mixed>  $data
     */
    protected function handleRecordUpdate(Model $record, array $data): Model
    {
        $actor = Filament::auth()->user();
        abort_unless($actor instanceof User && $record instanceof Filestore, 403);

        return app(ManageFilestore::class)->update($actor, $record, $data);
    }

    protected function getHeaderActions(): array
    {
        return [DeleteAction::make()->using(function (Filestore $record): bool {
            $actor = Filament::auth()->user();
            abort_unless($actor instanceof User, 403);
            app(ManageFilestore::class)->delete($actor, $record);

            return true;
        })];
    }
}
