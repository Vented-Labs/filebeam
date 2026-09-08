<?php

declare(strict_types=1);

namespace App\Filament\Resources\Users\RelationManagers;

use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Resources\Users\UserResource;
use App\Models\Transfer;
use App\Models\User;
use Filament\Actions\Action;
use Filament\Resources\RelationManagers\RelationManager;
use Filament\Tables\Columns\TextColumn;
use Filament\Tables\Table;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Number;

class OwnedTransfersRelationManager extends RelationManager
{
    protected static string $relationship = 'ownedTransfers';

    protected static ?string $title = 'Owned transfers';

    public function mount(): void
    {
        abort_unless(static::canViewForRecord($this->ownerRecord, $this->pageClass ?? static::class), 403);

        parent::mount();
    }

    public static function canViewForRecord(Model $ownerRecord, string $pageClass): bool
    {
        return $ownerRecord instanceof User && UserResource::canView($ownerRecord);
    }

    public function table(Table $table): Table
    {
        return $table
            ->defaultSort('created_at', 'desc')
            ->paginated([10, 25, 50])
            ->columns([
                TextColumn::make('id')->label('ID')->copyable(),
                TextColumn::make('status')->badge(),
                TextColumn::make('item_count')->label('Items')->numeric(),
                TextColumn::make('ciphertext_bytes')->label('Uploaded size')->formatStateUsing(fn (int $state): string => Number::fileSize($state, precision: 2))->placeholder('-'),
                TextColumn::make('expires_at')->label('Expires')->dateTime(),
                TextColumn::make('created_at')->label('Created')->dateTime(),
            ])
            ->recordUrl(fn (Transfer $record): string => TransferResource::getUrl('view', ['record' => $record]))
            ->emptyStateHeading('No uploads from this account')
            ->recordActions([
                Action::make('view')->label('View')->url(fn (Transfer $record): string => TransferResource::getUrl('view', ['record' => $record])),
            ]);
    }
}
