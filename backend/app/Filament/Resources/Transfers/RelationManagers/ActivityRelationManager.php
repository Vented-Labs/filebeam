<?php

declare(strict_types=1);

namespace App\Filament\Resources\Transfers\RelationManagers;

use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Support\AuditPresentation;
use App\Models\Transfer;
use App\Models\User;
use Filament\Facades\Filament;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;

class ActivityRelationManager extends RelationManager
{
    protected static string $relationship = 'moderationActivity';

    protected static ?string $title = 'Moderation activity';

    public function mount(): void
    {
        abort_unless(static::canViewForRecord($this->ownerRecord, $this->pageClass ?? static::class), 403);

        parent::mount();
    }

    public static function canViewForRecord(Model $ownerRecord, string $pageClass): bool
    {
        $actor = Filament::auth()->user();

        return $ownerRecord instanceof Transfer
            && $actor instanceof User
            && $actor->isStaff()
            && TransferResource::canView($ownerRecord);
    }

    public function table(Table $table): Table
    {
        return $table
            ->modifyQueryUsing(fn (Builder $query): Builder => $query->where('action', 'like', 'transfer.%'))
            ->columns([
                TextColumn::make('created_at')->label('When')->dateTime(),
                TextColumn::make('actor.email')->label('Actor')->placeholder('Deleted user'),
                TextColumn::make('action')->formatStateUsing(fn (string $state): string => AuditPresentation::label($state)),
                TextColumn::make('reason')->placeholder('-')->wrap(),
            ])
            ->defaultSort('created_at', 'desc');
    }
}
