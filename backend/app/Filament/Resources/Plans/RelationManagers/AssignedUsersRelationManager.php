<?php

declare(strict_types=1);

namespace App\Filament\Resources\Plans\RelationManagers;

use App\Filament\Resources\Users\UserResource;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\IconColumn;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;

class AssignedUsersRelationManager extends RelationManager
{
    protected static string $relationship = 'users';

    protected static ?string $title = 'Assigned users';

    public function table(Table $table): Table
    {
        return $table
            ->defaultSort('email')
            ->paginated([10, 25, 50])
            ->columns([
                TextColumn::make('email')->searchable(),
                TextColumn::make('username')->placeholder('Not provided'),
                TextColumn::make('role')->badge(),
                IconColumn::make('email_verified_at')->label('Verified')->boolean()->state(fn (User $record): bool => $record->email_verified_at !== null),
                TextColumn::make('suspended_at')->label('Status')->state(fn (User $record): string => $record->suspended_at === null ? 'Active' : 'Suspended')->badge(),
            ])
            ->recordActions([
                Action::make('view')->label('View')->url(fn (User $record): string => UserResource::getUrl('view', ['record' => $record])),
            ]);
    }
}
